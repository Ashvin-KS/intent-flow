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

                    // Check if we should merge with last activity
                    if let Some(ref last) = last_activity {
                        if last.app_hash == activity.app_hash && 
                           last.window_title_hash == activity.window_title_hash {
                            // Merge: extend duration
                            let merged = ActivityEvent {
                                end_time: activity.end_time,
                                duration_seconds: last.duration_seconds + 5,
                                ..last.clone()
                            };
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
            let app_name = window.app_name;
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

fn categorize_window(app_name: &str, title: &str) -> i32 {
    let app_lower = app_name.to_lowercase();
    let title_lower = title.to_lowercase();
    
    // Development
    if app_lower.contains("code") || 
       app_lower.contains("vscode") ||
       app_lower.contains("idea") ||
       app_lower.contains("pycharm") ||
       app_lower.contains("git") ||
       title_lower.contains("visual studio code") {
        return 1;
    }
    
    // Browser
    if app_lower.contains("chrome") || 
       app_lower.contains("firefox") ||
       app_lower.contains("edge") ||
       app_lower.contains("brave") {
        return 2;
    }
    
    // Communication
    if app_lower.contains("slack") || 
       app_lower.contains("discord") ||
       app_lower.contains("teams") ||
       app_lower.contains("zoom") ||
       app_lower.contains("telegram") {
        return 3;
    }
    
    // Entertainment
    if app_lower.contains("spotify") || 
       app_lower.contains("netflix") ||
       app_lower.contains("youtube") ||
       title_lower.contains("youtube") {
        return 4;
    }
    
    // Productivity
    if app_lower.contains("notion") || 
       app_lower.contains("obsidian") ||
       app_lower.contains("todo") ||
       title_lower.contains("notion") {
        return 5;
    }
    
    // System
    if app_lower.contains("explorer") || 
       app_lower.contains("settings") ||
       app_lower.contains("task manager") {
        return 6;
    }
    
    // Other
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
