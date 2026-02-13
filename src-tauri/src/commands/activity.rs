use tauri::{AppHandle, Manager};
use crate::models::{Activity, ActivityStats};

#[tauri::command]
pub async fn get_activities(
    app_handle: AppHandle,
    start_time: i64,
    end_time: i64,
    limit: Option<i32>,
) -> Result<Vec<Activity>, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    crate::database::queries::get_activities(&conn, start_time, end_time, limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_activity_stats(
    app_handle: AppHandle,
    start_time: i64,
    end_time: i64,
) -> Result<ActivityStats, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    crate::database::queries::get_activity_stats(&conn, start_time, end_time)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_current_activity(
    _app_handle: AppHandle,
) -> Result<Option<Activity>, String> {
    // Return the current activity (last recorded)
    Ok(None) // Placeholder - implement based on current tracking state
}
