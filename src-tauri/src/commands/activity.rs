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
    app_handle: AppHandle,
) -> Result<Option<Activity>, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    let mut stmt = conn.prepare(
        "SELECT id, app_name, app_hash, window_title, window_title_hash, category_id,
                start_time, end_time, duration_seconds, metadata
         FROM activities
         ORDER BY start_time DESC
         LIMIT 1"
    ).map_err(|e| e.to_string())?;
    
    let result = stmt.query_row([], |row| {
        let metadata_blob: Option<Vec<u8>> = row.get(9)?;
        let metadata = metadata_blob
            .and_then(|b| serde_json::from_slice(&b).ok());
        
        Ok(Activity {
            id: row.get(0)?,
            app_name: row.get(1)?,
            app_hash: row.get::<_, i64>(2)? as u64,
            window_title: row.get::<_, String>(3).unwrap_or_default(),
            window_title_hash: row.get::<_, i64>(4).unwrap_or(0) as u64,
            category_id: row.get(5)?,
            start_time: row.get(6)?,
            end_time: row.get(7)?,
            duration_seconds: row.get(8)?,
            metadata,
        })
    });
    
    match result {
        Ok(activity) => Ok(Some(activity)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
