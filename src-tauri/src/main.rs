// Prevents additional console window on Windows (silent launch).
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod commands;
mod database;
mod models;
mod services;
mod utils;

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Emitter, Manager};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri_plugin_autostart::ManagerExt;

static GAME_MODE_ENABLED: AtomicBool = AtomicBool::new(false);
static INCOGNITO_ENABLED: AtomicBool = AtomicBool::new(false);

fn main() {
    utils::config::load_dotenv();

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .setup(|app| {
            // Initialize database
            let app_handle = app.handle();
            let data_dir = app_handle.path().app_data_dir().expect("Failed to get app data dir");
            
            // Create data directory if it doesn't exist
            std::fs::create_dir_all(&data_dir).expect("Failed to create data directory");
            
            // Initialize database
            let db_path = data_dir.join("intentflow.db");
            database::init_database(&db_path).expect("Failed to initialize database");
            
            // Start activity tracker
            services::activity_tracker::start_tracking(app_handle.clone());
            
            // Start screen capture + OCR service (every ~10s, non-blocking)
            services::screen_capture::start_screen_capture(app_handle.clone());

            // Start code file monitor (for coding-context enrichment)
            services::file_monitor::start_file_monitor(app_handle.clone());
            
            // Start pattern engine (background analysis)
            services::pattern_engine::start_pattern_engine(app_handle.clone());

            // Start daily dashboard engine (today-focused summaries)
            services::dashboard_engine::start_dashboard_engine(app_handle.clone());

            // Apply startup enable/disable on Windows from settings.
            #[cfg(all(target_os = "windows", not(debug_assertions)))]
            {
                let autostart = app_handle.autolaunch();
                let startup_enabled = read_settings(&app_handle)
                    .map(|s| s.general.enable_startup)
                    .unwrap_or(true);
                if startup_enabled {
                    let _ = autostart.enable();
                } else {
                    let _ = autostart.disable();
                }
            }
            #[cfg(all(target_os = "windows", debug_assertions))]
            {
                // Never keep autostart pointing to dev/debug binaries (they depend on localhost dev server).
                let _ = app_handle.autolaunch().disable();
            }

            apply_startup_behavior(&app_handle);
            apply_monitoring_state(&app_handle);
            setup_tray(app)?;
            
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if should_close_to_tray(window.app_handle()) {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            // Activity commands
            commands::activity::get_activities,
            commands::activity::get_activity_stats,
            commands::activity::get_current_activity,
            // Query commands
            commands::query::execute_query,
            commands::query::get_query_history,
            // Intent commands
            commands::intent::parse_intent,
            commands::intent::execute_intent,
            // Workflow commands
            commands::workflow::get_workflows,
            commands::workflow::create_workflow,
            commands::workflow::update_workflow,
            commands::workflow::delete_workflow,
            commands::workflow::execute_workflow,
            commands::workflow::get_workflow_suggestions,
            // Entry commands
            commands::entry::create_entry,
            commands::entry::get_entries,
            commands::entry::update_entry_status,
            commands::entry::delete_entry,
            // Settings commands
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::get_categories,
            commands::settings::update_categories,
            commands::settings::get_nvidia_models,
            // Storage commands
            commands::storage::get_storage_stats,
            commands::storage::cleanup_old_data,
            commands::storage::export_data,
            // App control commands
            commands::app_control::minimize_to_tray,
            commands::app_control::show_window,
            commands::app_control::quit_app,
            // Chat commands
            commands::chat::create_chat_session,
            commands::chat::get_chat_sessions,
            commands::chat::delete_chat_session,
            commands::chat::get_chat_messages,
            commands::chat::send_chat_message,
            commands::chat::get_recent_models,
            commands::chat::remove_recent_model,
            // Dashboard commands
            commands::dashboard::get_dashboard_overview,
            commands::dashboard::refresh_dashboard_overview,
            commands::dashboard::summarize_contact,
            commands::dashboard::summarize_project,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let launch_item = MenuItem::with_id(app, "launch_app", "Launch App", true, None::<&str>)?;
    let open_chat_item = MenuItem::with_id(app, "open_chat", "Open Chat", true, None::<&str>)?;
    let game_mode_item = MenuItem::with_id(app, "toggle_game_mode", "Game Mode: OFF", true, None::<&str>)?;
    let incognito_item = MenuItem::with_id(app, "toggle_incognito", "Incognito: OFF", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &launch_item,
            &open_chat_item,
            &PredefinedMenuItem::separator(app)?,
            &game_mode_item,
            &incognito_item,
            &PredefinedMenuItem::separator(app)?,
            &quit_item,
        ],
    )?;

    let game_mode_item_handle = game_mode_item.clone();
    let incognito_item_handle = incognito_item.clone();

    let mut tray_builder = TrayIconBuilder::new()
        .menu(&menu)
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref();
            match id {
                "launch_app" => {
                    show_window_and_navigate(app, "home");
                }
                "open_chat" => {
                    show_window_and_navigate(app, "chat");
                }
                "toggle_game_mode" => {
                    let next = !GAME_MODE_ENABLED.load(Ordering::Relaxed);
                    GAME_MODE_ENABLED.store(next, Ordering::Relaxed);
                    let _ = game_mode_item_handle.set_text(if next {
                        "Game Mode: ON"
                    } else {
                        "Game Mode: OFF"
                    });
                    apply_monitoring_state(app);
                }
                "toggle_incognito" => {
                    let next = !INCOGNITO_ENABLED.load(Ordering::Relaxed);
                    INCOGNITO_ENABLED.store(next, Ordering::Relaxed);
                    let _ = incognito_item_handle.set_text(if next {
                        "Incognito: ON"
                    } else {
                        "Incognito: OFF"
                    });
                    apply_monitoring_state(app);
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window_and_navigate(tray.app_handle(), "home");
            }
        });

    if let Some(icon) = app.default_window_icon() {
        tray_builder = tray_builder.icon(icon.clone());
    }

    tray_builder.build(app)?;

    Ok(())
}

fn apply_startup_behavior(app_handle: &tauri::AppHandle) {
    let args: Vec<String> = std::env::args().collect();
    let is_autostart = args.iter().any(|arg| arg == "--autostart");

    let behavior = read_settings(app_handle)
        .map(|s| s.general.startup_behavior.to_lowercase())
        .unwrap_or_else(|| "normal".to_string());

    if is_autostart && (behavior == "minimized_to_tray" || behavior == "hidden") {
        // Keep it hidden (it's hidden by default in tauri.conf.json)
    } else {
        // Show the window for manual launch or if startup behavior is normal
        if let Some(window) = app_handle.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

fn apply_monitoring_state(app_handle: &tauri::AppHandle) {
    let settings_enabled = read_settings(app_handle)
        .map(|s| s.tracking.enabled)
        .unwrap_or(true);
    let effective_enabled = settings_enabled
        && !GAME_MODE_ENABLED.load(Ordering::Relaxed)
        && !INCOGNITO_ENABLED.load(Ordering::Relaxed);
    services::activity_tracker::set_tracking_enabled(effective_enabled);
    services::screen_capture::set_capture_enabled(effective_enabled);
}

fn show_window_and_navigate(app_handle: &tauri::AppHandle, page: &str) {
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit("tray:navigate", page.to_string());
    }
}

fn should_close_to_tray(app_handle: &tauri::AppHandle) -> bool {
    read_settings(app_handle)
        .map(|s| s.general.close_to_tray)
        .unwrap_or(true)
}

fn read_settings(app_handle: &tauri::AppHandle) -> Option<models::Settings> {
    let data_dir = app_handle.path().app_data_dir().ok()?;
    let config_path = data_dir.join("config").join("settings.json");
    if !config_path.exists() {
        let mut settings = models::Settings::default();
        utils::config::apply_env_defaults(&mut settings);
        return Some(settings);
    }
    let content = std::fs::read_to_string(config_path).ok()?;
    let mut settings = serde_json::from_str::<models::Settings>(&content).ok()?;
    utils::config::apply_env_defaults(&mut settings);
    Some(settings)
}
