use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use uuid::Uuid;
use crate::models::Settings;

fn load_settings(app_handle: &AppHandle) -> Option<Settings> {
    let data_dir = app_handle.path().app_data_dir().ok()?;
    let settings_path = data_dir.join("config").join("settings.json");
    let data = std::fs::read_to_string(settings_path).ok()?;
    let mut settings: Settings = serde_json::from_str(&data).ok()?;
    crate::utils::config::apply_env_defaults(&mut settings);
    Some(settings)
}

fn load_recent_chat_context(
    conn: &rusqlite::Connection,
    session_id: &str,
    limit: i64,
) -> Result<Vec<crate::services::query_engine::ChatMessage>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT role, content
             FROM chat_messages
             WHERE session_id = ?1
             ORDER BY created_at DESC
             LIMIT ?2",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(rusqlite::params![session_id, limit], |row| {
            Ok(crate::services::query_engine::ChatMessage {
                role: row.get::<_, String>(0)?,
                content: row.get::<_, String>(1)?,
            })
        })
        .map_err(|e| e.to_string())?;

    Ok(rows.filter_map(|r| r.ok()).collect())
}

// ─── Types ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageResponse {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub tool_calls: Option<Vec<crate::services::query_engine::AgentStep>>,
    pub activities: Option<Vec<serde_json::Value>>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentModel {
    pub id: String,
    pub name: String,
    pub use_count: i32,
    pub last_used: i64,
}

// ─── Commands ───

#[tauri::command]
pub async fn create_chat_session(app_handle: AppHandle) -> Result<ChatSession, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;

    let session = ChatSession {
        id: Uuid::new_v4().to_string(),
        title: "New Chat".to_string(),
        created_at: Utc::now().timestamp(),
        updated_at: Utc::now().timestamp(),
    };

    conn.execute(
        "INSERT INTO chat_sessions (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![session.id, session.title, session.created_at, session.updated_at],
    ).map_err(|e| e.to_string())?;

    Ok(session)
}

#[tauri::command]
pub async fn get_chat_sessions(app_handle: AppHandle) -> Result<Vec<ChatSession>, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;

    let mut stmt = conn.prepare(
        "SELECT s.id, s.title, s.created_at, s.updated_at
         FROM chat_sessions s
         INNER JOIN (
            SELECT session_id, COUNT(*) as msg_count
            FROM chat_messages
            GROUP BY session_id
         ) m ON m.session_id = s.id
         WHERE m.msg_count > 0
         ORDER BY s.updated_at DESC"
    ).map_err(|e| e.to_string())?;

    let sessions = stmt.query_map([], |row| {
        Ok(ChatSession {
            id: row.get(0)?,
            title: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
        })
    }).map_err(|e| e.to_string())?
    .filter_map(|r| r.ok())
    .collect();

    Ok(sessions)
}

#[tauri::command]
pub async fn delete_chat_session(app_handle: AppHandle, session_id: String) -> Result<(), String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;

    // Delete messages first, then session
    conn.execute("DELETE FROM chat_messages WHERE session_id = ?1", [&session_id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM chat_sessions WHERE id = ?1", [&session_id])
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn get_chat_messages(
    app_handle: AppHandle,
    session_id: String,
) -> Result<Vec<ChatMessageResponse>, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;

    let mut stmt = conn.prepare(
        "SELECT id, session_id, role, content, agent_steps, activities, created_at 
         FROM chat_messages WHERE session_id = ?1 ORDER BY created_at ASC"
    ).map_err(|e| e.to_string())?;

    let messages = stmt.query_map([&session_id], |row| {
        let steps_json: Option<String> = row.get(4)?;
        let activities_json: Option<String> = row.get(5)?;

        Ok(ChatMessageResponse {
            id: row.get(0)?,
            session_id: row.get(1)?,
            role: row.get(2)?,
            content: row.get(3)?,
            tool_calls: steps_json.and_then(|s| serde_json::from_str(&s).ok()),
            activities: activities_json.and_then(|s| serde_json::from_str(&s).ok()),
            created_at: row.get(6)?,
        })
    }).map_err(|e| e.to_string())?
    .filter_map(|r| r.ok())
    .collect();

    Ok(messages)
}

#[tauri::command]
pub async fn send_chat_message(
    app_handle: AppHandle,
    session_id: String,
    message: String,
    model: Option<String>,
) -> Result<ChatMessageResponse, String> {
    let now = Utc::now().timestamp();
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");

    // 1. Load recent chat context (before inserting this message)
    let recent_context = {
        let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
        load_recent_chat_context(&conn, &session_id, 12)?
    };

    // 2. Store user message
    {
        let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO chat_messages (session_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![session_id, "user", message, now],
        ).map_err(|e| e.to_string())?;

        // Update session title from first message
        let msg_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM chat_messages WHERE session_id = ?1",
            [&session_id],
            |row| row.get(0),
        ).unwrap_or(0);

        if msg_count <= 1 {
            // Use first ~50 chars of first message as title
            let title = if message.len() > 50 {
                let end = message.char_indices().nth(50).map(|(i, _)| i).unwrap_or(message.len());
                format!("{}...", &message[..end])
            } else {
                message.clone()
            };
            conn.execute(
                "UPDATE chat_sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![title, now, session_id],
            ).map_err(|e| e.to_string())?;
        } else {
            conn.execute(
                "UPDATE chat_sessions SET updated_at = ?1 WHERE id = ?2",
                rusqlite::params![now, session_id],
            ).map_err(|e| e.to_string())?;
        }
    }

    // 3. Run agentic search with conversation context
    let mut settings = load_settings(&app_handle).unwrap_or_default();
    if let Some(model_id) = model.as_ref().map(|m| m.trim()).filter(|m| !m.is_empty()) {
        settings.ai.model = model_id.to_string();
    }
    let resolved_api_key = crate::utils::config::resolve_api_key(&settings.ai.api_key);
    
    let agent_result = if settings.ai.enabled && !resolved_api_key.is_empty() {
        crate::services::query_engine::run_agentic_search_with_steps_and_history(
            &app_handle,
            &message,
            &settings,
            &recent_context,
        ).await
            .unwrap_or_else(|e| crate::services::query_engine::AgentResult {
                answer: format!("Sorry, I encountered an error: {}", e),
                steps: vec![],
                activities_referenced: vec![],
            })
    } else {
        crate::services::query_engine::AgentResult {
            answer: "AI is not configured. Please set your API key in Settings.".to_string(),
            steps: vec![],
            activities_referenced: vec![],
        }
    };

    // 4. Store assistant message with steps + activities
    let response_time = Utc::now().timestamp();
    let steps_json = serde_json::to_string(&agent_result.steps).ok();
    let activities_json = serde_json::to_string(&agent_result.activities_referenced).ok();

    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO chat_messages (session_id, role, content, agent_steps, activities, created_at) 
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            session_id,
            "assistant",
            agent_result.answer,
            steps_json,
            activities_json,
            response_time
        ],
    ).map_err(|e| e.to_string())?;

    let msg_id = conn.last_insert_rowid();

    // Track recently used model for quick selection.
    let model_id = settings.ai.model.trim();
    if !model_id.is_empty() {
        let _ = conn.execute(
            "INSERT INTO ai_model_usage (model_id, model_name, use_count, last_used)
             VALUES (?1, ?2, 1, ?3)
             ON CONFLICT(model_id) DO UPDATE SET
                model_name = excluded.model_name,
                use_count = ai_model_usage.use_count + 1,
                last_used = excluded.last_used",
            rusqlite::params![model_id, model_id, response_time],
        );
    }

    Ok(ChatMessageResponse {
        id: msg_id,
        session_id,
        role: "assistant".to_string(),
        content: agent_result.answer,
        tool_calls: Some(agent_result.steps),
        activities: Some(agent_result.activities_referenced),
        created_at: response_time,
    })
}

#[tauri::command]
pub async fn get_recent_models(
    app_handle: AppHandle,
    limit: Option<i32>,
) -> Result<Vec<RecentModel>, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    let row_limit = limit.unwrap_or(5).clamp(1, 20);

    let mut stmt = conn
        .prepare(
            "SELECT model_id, model_name, use_count, last_used
             FROM ai_model_usage
             ORDER BY last_used DESC
             LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([row_limit], |row| {
            Ok(RecentModel {
                id: row.get(0)?,
                name: row.get(1)?,
                use_count: row.get(2)?,
                last_used: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;

    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[tauri::command]
pub async fn remove_recent_model(
    app_handle: AppHandle,
    model_id: String,
) -> Result<(), String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM ai_model_usage WHERE model_id = ?1",
        [&model_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
