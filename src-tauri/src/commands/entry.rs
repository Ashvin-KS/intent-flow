use tauri::{AppHandle, Manager};
use crate::models::ManualEntry;

#[tauri::command]
pub async fn create_entry(
    app_handle: AppHandle,
    entry_type: String,
    title: String,
    content: Option<String>,
    tags: Option<Vec<String>>,
) -> Result<String, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    let now = chrono::Utc::now().timestamp();
    let tags_blob = serde_json::to_vec(&tags.unwrap_or_default()).map_err(|e| e.to_string())?;
    
    conn.execute(
        "INSERT INTO manual_entries (entry_type, title, content, tags, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?5)",
        rusqlite::params![&entry_type, &title, &content.unwrap_or_default(), &tags_blob, now],
    ).map_err(|e| e.to_string())?;
    
    Ok(conn.last_insert_rowid().to_string())
}

#[tauri::command]
pub async fn get_entries(
    app_handle: AppHandle,
    entry_type: Option<String>,
    status: Option<String>,
    limit: Option<i32>,
) -> Result<Vec<ManualEntry>, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    let limit_clause = limit.map(|l| format!("LIMIT {}", l)).unwrap_or_default();
    let query = format!(
        "SELECT id, entry_type, title, content, tags, status, created_at, updated_at, completed_at
         FROM manual_entries
         WHERE (?1 IS NULL OR entry_type = ?1)
         AND (?2 IS NULL OR status = ?2)
         ORDER BY created_at DESC
         {}",
        limit_clause
    );
    
    let mut stmt = conn.prepare(&query).map_err(|e| e.to_string())?;
    
    let entries = stmt.query_map([&entry_type, &status], |row| {
        let tags_blob: Option<Vec<u8>> = row.get(4)?;
        let tags: Vec<String> = tags_blob
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        
        Ok(ManualEntry {
            id: row.get(0)?,
            entry_type: row.get(1)?,
            title: row.get(2)?,
            content: row.get(3)?,
            tags,
            status: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
            completed_at: row.get(8)?,
        })
    }).map_err(|e| e.to_string())?
    .collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    
    Ok(entries)
}

#[tauri::command]
pub async fn update_entry_status(
    app_handle: AppHandle,
    id: i64,
    status: String,
) -> Result<(), String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    let now = chrono::Utc::now().timestamp();
    let completed_at = if status == "completed" { Some(now) } else { None };
    
    conn.execute(
        "UPDATE manual_entries SET status = ?1, updated_at = ?2, completed_at = ?3 WHERE id = ?4",
        [&status, &now.to_string(), &completed_at.map(|t| t.to_string()).unwrap_or_default(), &id.to_string()],
    ).map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
pub async fn delete_entry(
    app_handle: AppHandle,
    id: i64,
) -> Result<(), String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    conn.execute(
        "DELETE FROM manual_entries WHERE id = ?1",
        [&id.to_string()],
    ).map_err(|e| e.to_string())?;
    
    Ok(())
}
