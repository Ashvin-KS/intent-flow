// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod database;
mod models;
mod services;
mod utils;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
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
            
            Ok(())
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
            // Storage commands
            commands::storage::get_storage_stats,
            commands::storage::cleanup_old_data,
            commands::storage::export_data,
            // App control commands
            commands::app_control::minimize_to_tray,
            commands::app_control::show_window,
            commands::app_control::quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
