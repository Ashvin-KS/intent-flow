use tauri::{AppHandle, Manager};
use crate::models::StorageStats;

#[tauri::command]
pub async fn get_storage_stats(
    app_handle: AppHandle,
) -> Result<StorageStats, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    // Get database file size
    let total_size_bytes = db_path.metadata()
        .map(|m| m.len() as i64)
        .unwrap_or(0);
    
    // Get counts
    let activities_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM activities",
        [],
        |row| row.get(0),
    ).unwrap_or(0);
    
    let summaries_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM activity_summaries",
        [],
        |row| row.get(0),
    ).unwrap_or(0);
    
    let patterns_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM patterns",
        [],
        |row| row.get(0),
    ).unwrap_or(0);
    
    let entries_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM manual_entries",
        [],
        |row| row.get(0),
    ).unwrap_or(0);
    
    // Get oldest and newest activity
    let oldest_activity: i64 = conn.query_row(
        "SELECT MIN(start_time) FROM activities",
        [],
        |row| row.get(0),
    ).unwrap_or(0);
    
    let newest_activity: i64 = conn.query_row(
        "SELECT MAX(start_time) FROM activities",
        [],
        |row| row.get(0),
    ).unwrap_or(0);
    
    Ok(StorageStats {
        total_size_bytes,
        activities_count,
        summaries_count,
        patterns_count,
        entries_count,
        oldest_activity,
        newest_activity,
    })
}

#[tauri::command]
pub async fn cleanup_old_data(
    app_handle: AppHandle,
    retention_days: i32,
) -> Result<i64, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    let cutoff = chrono::Utc::now().timestamp() - (retention_days as i64 * 86400);
    
    let deleted = conn.execute(
        "DELETE FROM activities WHERE start_time < ?1",
        [&cutoff],
    ).map_err(|e| e.to_string())?;
    
    Ok(deleted as i64)
}

#[tauri::command]
pub async fn export_data(
    app_handle: AppHandle,
) -> Result<String, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    // Export activities
    let activities = crate::database::queries::get_activities(&conn, 0, i64::MAX, None)
        .map_err(|e| e.to_string())?;
    
    let export = serde_json::json!({
        "version": "1.0.0",
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "activities": activities,
    });
    
    let export_path = data_dir.join("exports").join(format!(
        "intentflow_export_{}.json",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    ));
    
    std::fs::create_dir_all(export_path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&export_path, serde_json::to_string_pretty(&export).unwrap())
        .map_err(|e| e.to_string())?;
    
    Ok(export_path.to_string_lossy().to_string())
}
