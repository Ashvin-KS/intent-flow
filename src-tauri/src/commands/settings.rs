use tauri::{AppHandle, Manager};
use crate::models::{Settings, Category};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
}

#[tauri::command]
pub async fn get_nvidia_models(api_key: String) -> Result<Vec<ModelInfo>, String> {
    let client = reqwest::Client::new();
    
    let response = client
        .get("https://integrate.api.nvidia.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch models: {}", e))?;
    
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("API Error {}: {}", status, text));
    }
    
    #[derive(Deserialize)]
    struct ModelsResponse {
        data: Vec<ModelData>,
    }
    
    #[derive(Deserialize)]
    struct ModelData {
        id: String,
    }
    
    let models_response: ModelsResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;
    
    let models: Vec<ModelInfo> = models_response
        .data
        .into_iter()
        .map(|m| ModelInfo {
            id: m.id.clone(),
            name: m.id,
        })
        .collect();
    
    Ok(models)
}

#[tauri::command]
pub async fn get_settings(
    app_handle: AppHandle,
) -> Result<Settings, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let config_path = data_dir.join("config").join("settings.json");
    
    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())
    } else {
        Ok(Settings::default())
    }
}

#[tauri::command]
pub async fn update_settings(
    app_handle: AppHandle,
    settings: Settings,
) -> Result<(), String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let config_dir = data_dir.join("config");
    
    std::fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;
    
    let config_path = config_dir.join("settings.json");
    let content = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    
    std::fs::write(&config_path, content).map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
pub async fn get_categories(
    app_handle: AppHandle,
) -> Result<Vec<Category>, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    let mut stmt = conn.prepare(
        "SELECT id, name, icon, color, keywords, apps FROM categories ORDER BY id"
    ).map_err(|e| e.to_string())?;
    
    let categories = stmt.query_map([], |row| {
        let keywords: Option<String> = row.get(4)?;
        let apps: Option<String> = row.get(5)?;
        
        Ok(Category {
            id: row.get(0)?,
            name: row.get(1)?,
            icon: row.get(2)?,
            color: row.get(3)?,
            keywords: keywords.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
            apps: apps.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
        })
    }).map_err(|e| e.to_string())?
    .collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    
    Ok(categories)
}

#[tauri::command]
pub async fn update_categories(
    app_handle: AppHandle,
    categories: Vec<Category>,
) -> Result<(), String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    for category in categories {
        let keywords = serde_json::to_string(&category.keywords).map_err(|e| e.to_string())?;
        let apps = serde_json::to_string(&category.apps).map_err(|e| e.to_string())?;
        
        conn.execute(
            "UPDATE categories SET name = ?1, icon = ?2, color = ?3, keywords = ?4, apps = ?5 WHERE id = ?6",
            [
                &category.name,
                &category.icon,
                &category.color,
                &keywords,
                &apps,
                &category.id.to_string(),
            ],
        ).map_err(|e| e.to_string())?;
    }
    
    Ok(())
}
