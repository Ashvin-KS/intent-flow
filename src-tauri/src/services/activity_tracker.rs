use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager};

use crate::models::ActivityEvent;

static TRACKING_ENABLED: AtomicBool = AtomicBool::new(true);

pub fn start_tracking(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last_activity: Option<ActivityEvent> = None;
        
        loop {
            if !TRACKING_ENABLED.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }

            // Get active window
            match get_active_window() {
                Ok(Some(window)) => {
                    let now = chrono::Utc::now().timestamp();
                    
                    // Create activity event
                    let activity = ActivityEvent::new(
                        window.app_name,
                        window.title,
                        window.category_id,
                        now,
                        now + 5, // 5 second interval
                    );
                    
                    let mut activity = activity;
                    
                    // Attach latest OCR screen text to activity metadata
                    let screen_text = super::screen_capture::get_latest_screen_text();
                    activity.metadata.screen_text = screen_text;

                    // Attach currently open background windows (for music tracking etc)
                    let bg_windows = crate::utils::windows::get_open_windows();
                    if !bg_windows.is_empty() {
                         activity.metadata.background_windows = Some(bg_windows);
                    }

                    // Attach current media info (SMTC - Spotify, YouTube, etc.)
                    // Run synchrounous Windows API call in blocking thread
                    let media_info = match tokio::task::spawn_blocking(|| {
                        crate::utils::windows::get_media_info()
                    }).await {
                        Ok(info) => info,
                        Err(e) => {
                            println!("[Tracker] ⚠️ SMTC spawn_blocking failed: {:?}", e);
                            None
                        }
                    };
                    
                    activity.metadata.media_info = media_info;

                    // Check if we should merge with last activity
                    if let Some(ref last) = last_activity {
                        // Merge only if app + window title are the same.
                        // Background music and open windows are CONTEXT, not the activity itself.
                        // A song changing or a window opening shouldn't split the activity.
                        let is_same_app = last.app_name == activity.app_name && last.window_title == activity.window_title;

                        if is_same_app {
                            // Merge: extend duration, keep LATEST metadata
                            // (latest screen text, latest media info, latest bg windows)
                            let mut merged = ActivityEvent {
                                end_time: activity.end_time,
                                duration_seconds: last.duration_seconds + 5,
                                ..last.clone()
                            };
                            // Always keep the freshest metadata snapshot
                            merged.metadata = activity.metadata;
                            // But preserve screen_text from last if current is empty
                            if merged.metadata.screen_text.is_none() {
                                merged.metadata.screen_text = last.metadata.screen_text.clone();
                            }
                            
                            last_activity = Some(merged);
                        } else {
                            // Store the previous activity
                            if let Err(e) = store_activity(&app_handle, last) {
                                log::error!("Failed to store activity: {}", e);
                            }
                            last_activity = Some(activity);
                        }
                    } else {
                        last_activity = Some(activity);
                    }
                }
                Ok(None) => {
                    // No active window (idle or locked)
                    if let Some(last) = last_activity.take() {
                        if let Err(e) = store_activity(&app_handle, &last) {
                            log::error!("Failed to store activity: {}", e);
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to get active window: {}", e);
                }
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

struct ActiveWindow {
    app_name: String,
    title: String,
    category_id: i32,
}

fn get_active_window() -> Result<Option<ActiveWindow>, String> {
    // Use active-win-pos-rs to get the active window
    match active_win_pos_rs::get_active_window() {
        Ok(window) => {
            // Sanitize app_name - remove control characters and normalize
            let app_name = sanitize_app_name(&window.app_name);
            let title = window.title;
            
            // Categorize the window
            let category_id = categorize_window(&app_name, &title);
            
            Ok(Some(ActiveWindow {
                app_name,
                title,
                category_id,
            }))
        }
        Err(_) => Ok(None),
    }
}

/// Sanitize app name by removing control characters and normalizing known apps
fn sanitize_app_name(name: &str) -> String {
    // Remove control characters (ASCII 0-31) and other non-printable chars
    let cleaned: String = name.chars()
        .filter(|c| c.is_ascii() && (*c as u8) >= 32 || !c.is_ascii())
        .collect();
    
    // Normalize known app names
    let cleaned_lower = cleaned.to_lowercase();
    
    // Spotify can appear as "Spotify8FileV" or similar with embedded chars
    if cleaned_lower.starts_with("spotify") || cleaned.contains("Spotify") {
        return "Spotify".to_string();
    }
    
    cleaned
}

fn categorize_window(app_name: &str, title: &str) -> i32 {
    let app_lower = app_name.to_lowercase();
    let title_lower = title.to_lowercase();
    
    // Development (category 1)
    if app_lower.contains("code") || 
       app_lower.contains("vscode") ||
       app_lower.contains("antigravity") ||
       app_lower.contains("cursor") ||
       app_lower.contains("idea") ||
       app_lower.contains("pycharm") ||
       app_lower.contains("webstorm") ||
       app_lower.contains("phpstorm") ||
       app_lower.contains("rider") ||
       app_lower.contains("clion") ||
       app_lower.contains("goland") ||
       app_lower.contains("android studio") ||
       app_lower.contains("eclipse") ||
       app_lower.contains("sublime") ||
       app_lower.contains("atom") ||
       app_lower.contains("vim") ||
       app_lower.contains("neovim") ||
       app_lower.contains("emacs") ||
       app_lower.contains("git") ||
       app_lower.contains("terminal") ||
       app_lower.contains("powershell") ||
       app_lower.contains("cmd") ||
       app_lower.contains("windowsterminal") ||
       app_lower.contains("wt") ||
       app_lower.contains("postman") ||
       app_lower.contains("insomnia") ||
       app_lower.contains("docker") ||
       title_lower.contains("visual studio") ||
       title_lower.contains("- antigravity") {
        return 1;
    }
    
    // Check title for code file extensions → Development
    let code_extensions = [".ts", ".tsx", ".js", ".jsx", ".py", ".rs", ".go", 
                           ".java", ".cpp", ".c", ".cs", ".rb", ".php", ".vue",
                           ".svelte", ".html", ".css", ".scss", ".json", ".toml",
                           ".yaml", ".yml", ".md", ".sql"];
    for ext in code_extensions {
        if title_lower.contains(ext) {
            return 1;
        }
    }
    
    // Entertainment (category 4) — check TITLE first (before browser) so
    // Spotify/YouTube playing inside a browser gets tagged as Entertainment
    // Note: Spotify web shows playing tracks as "Song • Artist - Browser"
    if app_lower.contains("spotify") || 
       app_lower.contains("netflix") ||
       app_lower.contains("youtube") ||
       app_lower.contains("vlc") ||
       app_lower.contains("media player") ||
       title_lower.contains("spotify") ||
       title_lower.contains("youtube") ||
       title_lower.contains("netflix") ||
       title_lower.contains("twitch") ||
       title_lower.contains("soundcloud") ||
       title_lower.contains("apple music") ||
       title_lower.contains("liked songs") ||
       title_lower.contains("\u{2022}") {  // "•" bullet — Spotify uses "Song • Artist" format
        return 4;
    }
    
    // Browser (category 2) — only if title didn't match entertainment above
    if app_lower.contains("chrome") || 
       app_lower.contains("firefox") ||
       app_lower.contains("edge") ||
       app_lower.contains("brave") ||
       app_lower.contains("opera") ||
       app_lower.contains("vivaldi") ||
       app_lower.contains("safari") ||
       app_lower.contains("webview2") ||
       app_lower.contains("msedgewebview") {
        return 2;
    }
    
    // Communication (category 3)
    if app_lower.contains("slack") || 
       app_lower.contains("discord") ||
       app_lower.contains("teams") ||
       app_lower.contains("zoom") ||
       app_lower.contains("telegram") ||
       app_lower.contains("whatsapp") ||
       app_lower.contains("signal") ||
       app_lower.contains("skype") ||
       app_lower.contains("outlook") ||
       app_lower.contains("thunderbird") ||
       app_lower.contains("gmail") {
        return 3;
    }
    
    // Productivity (category 5)
    if app_lower.contains("notion") || 
       app_lower.contains("obsidian") ||
       app_lower.contains("todo") ||
       app_lower.contains("word") ||
       app_lower.contains("excel") ||
       app_lower.contains("powerpoint") ||
       app_lower.contains("onenote") ||
       app_lower.contains("notepad") ||
       app_lower.contains("figma") ||
       title_lower.contains("notion") ||
       title_lower.contains("google docs") ||
       title_lower.contains("google sheets") {
        return 5;
    }
    
    // System (category 6)
    if app_lower.contains("explorer") || 
       app_lower.contains("settings") ||
       app_lower.contains("task manager") ||
       app_lower.contains("control panel") ||
       app_lower.contains("systemsettings") {
        return 6;
    }
    
    // Other (category 7)
    7
}

fn store_activity(app_handle: &AppHandle, activity: &ActivityEvent) -> Result<(), String> {
    // Get database path
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    // Open connection and store
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    let metadata_blob = serde_json::to_vec(&activity.metadata).map_err(|e| e.to_string())?;
    
    conn.execute(
        "INSERT INTO activities 
         (app_name, app_hash, window_title, window_title_hash, category_id, 
          start_time, end_time, duration_seconds, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            &activity.app_name,
            activity.app_hash as i64,
            &activity.window_title,
            activity.window_title_hash as i64,
            activity.category_id,
            activity.start_time,
            activity.end_time,
            activity.duration_seconds,
            &metadata_blob,
        ],
    ).map_err(|e| e.to_string())?;

    Ok(())
}

pub fn set_tracking_enabled(enabled: bool) {
    TRACKING_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn is_tracking_enabled() -> bool {
    TRACKING_ENABLED.load(Ordering::Relaxed)
}
