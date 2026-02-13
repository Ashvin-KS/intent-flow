use tauri::{AppHandle, Manager};
use crate::models::{Workflow, WorkflowSuggestion, AppLaunch, CreateWorkflowRequest};
use uuid::Uuid;

#[tauri::command]
pub async fn get_workflows(
    app_handle: AppHandle,
) -> Result<Vec<Workflow>, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    let mut stmt = conn.prepare(
        "SELECT uuid, name, description, icon, apps, files, urls, use_count, last_used, created_at
         FROM workflows
         ORDER BY use_count DESC"
    ).map_err(|e| e.to_string())?;
    
    let workflows = stmt.query_map([], |row| {
        let apps_blob: Option<Vec<u8>> = row.get(4)?;
        let files_blob: Option<Vec<u8>> = row.get(5)?;
        let urls_blob: Option<Vec<u8>> = row.get(6)?;
        
        let apps: Vec<AppLaunch> = apps_blob
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let files: Vec<String> = files_blob
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let urls: Vec<String> = urls_blob
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        
        Ok(Workflow {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            icon: row.get(3)?,
            apps,
            files,
            urls,
            use_count: row.get(7)?,
            last_used: row.get(8)?,
            created_at: row.get(9)?,
        })
    }).map_err(|e| e.to_string())?
    .collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    
    Ok(workflows)
}

#[tauri::command]
pub async fn create_workflow(
    app_handle: AppHandle,
    workflow: CreateWorkflowRequest,
) -> Result<String, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    let uuid = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    
    let apps_blob = serde_json::to_vec(&workflow.apps).map_err(|e| e.to_string())?;
    let files_blob = serde_json::to_vec(&workflow.files).map_err(|e| e.to_string())?;
    let urls_blob = serde_json::to_vec(&workflow.urls).map_err(|e| e.to_string())?;
    
    conn.execute(
        "INSERT INTO workflows (uuid, name, description, icon, apps, files, urls, use_count, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
        rusqlite::params![
            &uuid,
            &workflow.name,
            &workflow.description,
            &workflow.icon,
            &apps_blob,
            &files_blob,
            &urls_blob,
            now,
        ],
    ).map_err(|e| e.to_string())?;
    
    Ok(uuid)
}

#[tauri::command]
pub async fn update_workflow(
    app_handle: AppHandle,
    workflow: Workflow,
) -> Result<(), String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    let apps_blob = serde_json::to_vec(&workflow.apps).map_err(|e| e.to_string())?;
    let files_blob = serde_json::to_vec(&workflow.files).map_err(|e| e.to_string())?;
    let urls_blob = serde_json::to_vec(&workflow.urls).map_err(|e| e.to_string())?;
    
    conn.execute(
        "UPDATE workflows SET name = ?1, description = ?2, icon = ?3, apps = ?4, files = ?5, urls = ?6 WHERE uuid = ?7",
        rusqlite::params![
            &workflow.name,
            &workflow.description,
            &workflow.icon,
            &apps_blob,
            &files_blob,
            &urls_blob,
            &workflow.id,
        ],
    ).map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
pub async fn delete_workflow(
    app_handle: AppHandle,
    workflow_id: String,
) -> Result<(), String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    conn.execute(
        "DELETE FROM workflows WHERE uuid = ?1",
        [&workflow_id],
    ).map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
pub async fn execute_workflow(
    app_handle: AppHandle,
    workflow_id: String,
) -> Result<(), String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    // Get workflow
    let mut stmt = conn.prepare(
        "SELECT apps, urls, files FROM workflows WHERE uuid = ?1"
    ).map_err(|e| e.to_string())?;
    
    let (apps, urls, files): (Vec<AppLaunch>, Vec<String>, Vec<String>) = stmt
        .query_row([&workflow_id], |row| {
            let apps_blob: Option<Vec<u8>> = row.get(0)?;
            let urls_blob: Option<Vec<u8>> = row.get(1)?;
            let files_blob: Option<Vec<u8>> = row.get(2)?;
            
            Ok((
                apps_blob.and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default(),
                urls_blob.and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default(),
                files_blob.and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default(),
            ))
        })
        .map_err(|e| e.to_string())?;
    
    // Launch apps
    for app in apps {
        if cfg!(target_os = "windows") {
            std::process::Command::new(&app.path)
                .args(&app.args)
                .spawn()
                .ok();
        }
    }
    
    // Open URLs
    for url in urls {
        open::that(&url).ok();
    }
    
    // Open files
    for file in files {
        open::that(&file).ok();
    }
    
    // Update use count
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "UPDATE workflows SET use_count = use_count + 1, last_used = ?1 WHERE uuid = ?2",
        [&now.to_string(), &workflow_id],
    ).map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
pub async fn get_workflow_suggestions(
    app_handle: AppHandle,
) -> Result<Vec<WorkflowSuggestion>, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    // Suggest workflows based on most-used and recently used
    let mut stmt = conn.prepare(
        "SELECT uuid, name, description, icon, apps, files, urls, use_count, last_used, created_at
         FROM workflows
         WHERE use_count > 0
         ORDER BY use_count DESC
         LIMIT 4"
    ).map_err(|e| e.to_string())?;
    
    let suggestions = stmt.query_map([], |row| {
        let apps_blob: Option<Vec<u8>> = row.get(4)?;
        let files_blob: Option<Vec<u8>> = row.get(5)?;
        let urls_blob: Option<Vec<u8>> = row.get(6)?;
        
        let apps: Vec<AppLaunch> = apps_blob
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let files: Vec<String> = files_blob
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let urls: Vec<String> = urls_blob
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        
        let use_count: i32 = row.get(7)?;
        let workflow = Workflow {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            icon: row.get(3)?,
            apps,
            files,
            urls,
            use_count,
            last_used: row.get(8)?,
            created_at: row.get(9)?,
        };
        
        // Calculate relevance score based on use count (normalized)
        let relevance = (use_count as f32 / 10.0).min(1.0).max(0.1);
        
        Ok(WorkflowSuggestion {
            workflow,
            trigger_type: "pattern".to_string(),
            relevance_score: relevance,
            reason: format!("Used {} times", use_count),
        })
    }).map_err(|e| e.to_string())?
    .collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    
    Ok(suggestions)
}
