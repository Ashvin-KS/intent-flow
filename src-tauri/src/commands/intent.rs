use tauri::{AppHandle, Manager};
use crate::models::{Intent, IntentType, Action, ActionType, Settings};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// ─── NVIDIA OpenAI-compatible API types ───

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatSendMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Serialize)]
struct ChatSendMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatRecvMessage,
}

#[derive(Deserialize)]
struct ChatRecvMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
}

#[derive(Deserialize)]
struct AiIntentResponse {
    intent_type: String,
    confidence: f32,
    actions: Vec<AiAction>,
    reasoning: Option<String>,
}

#[derive(Deserialize)]
struct AiAction {
    action_type: String,
    target: String,
    #[serde(default)]
    args: Vec<String>,
}

const SYSTEM_PROMPT: &str = r#"You are IntentFlow's intent parser. Given user input, determine their intent and suggest actions.

Respond ONLY with valid JSON (no markdown fences, no extra text):
{
  "intent_type": "work_start" | "entertainment" | "focus" | "learning" | "wind_down" | "query" | "unknown",
  "confidence": 0.0-1.0,
  "actions": [
    {"action_type": "launch_app" | "open_url" | "open_file" | "show_notification" | "toggle_focus_mode", "target": "...", "args": []}
  ],
  "reasoning": "brief explanation"
}

Examples:
- "let's code" → work_start, launch VS Code
- "I'm bored" → entertainment, show notification with suggestions  
- "focus time" → focus, toggle_focus_mode, show_notification about focus mode
- "time to learn" → learning, open a learning resource URL
- "I'm done for today" → wind_down, show wind-down notification, toggle_focus_mode (off)
- "what did I do yesterday" → query, no actions (handled by query engine)"#;

#[tauri::command]
pub async fn parse_intent(
    app_handle: AppHandle,
    input: String,
) -> Result<Intent, String> {
    // Try AI-powered parsing first
    let settings = load_settings(&app_handle).unwrap_or_default();
    
    if settings.ai.enabled {
        let api_key = settings.ai.api_key.clone();
        let model = settings.ai.model.clone();
        
        if !api_key.is_empty() {
            match ai_parse_intent(&input, &api_key, &model).await {
                Ok(intent) => return Ok(intent),
                Err(e) => {
                    log::warn!("AI intent parsing failed, falling back to local: {}", e);
                }
            }
        }
    }
    
    // Fallback to local keyword matching
    Ok(local_parse_intent(&app_handle, &input).await)
}

async fn ai_parse_intent(input: &str, api_key: &str, model: &str) -> Result<Intent, String> {
    let client = reqwest::Client::new();
    
    let request = ChatRequest {
        model: model.to_string(),
        messages: vec![
            ChatSendMessage {
                role: "system".to_string(),
                content: SYSTEM_PROMPT.to_string(),
            },
            ChatSendMessage {
                role: "user".to_string(),
                content: input.to_string(),
            },
        ],
        temperature: 0.3,
        max_tokens: 512,
    };
    
    let response = client
        .post("https://integrate.api.nvidia.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("API request failed: {}", e))?;
    
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("API returned error {}: {}", status, body));
    }
    
    let body_text = response.text().await
        .map_err(|e| format!("Failed to read response: {}", e))?;
    
    let chat_response: ChatResponse = serde_json::from_str(&body_text)
        .map_err(|e| format!("Failed to parse API response: {}", e))?;
    
    let choice = chat_response.choices.first()
        .ok_or_else(|| "Empty AI response".to_string())?;
    
    // Try content first, then reasoning_content (for reasoning models like GLM)
    let content = choice.message.content.clone()
        .or_else(|| choice.message.reasoning_content.clone())
        .unwrap_or_default();
    
    // Clean possible markdown code fences
    let cleaned = content
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    
    let ai_response: AiIntentResponse = serde_json::from_str(cleaned)
        .map_err(|e| format!("Failed to parse AI JSON: {} — raw: {}", e, cleaned))?;
    
    // Convert AI response to our Intent model
    let intent_type = match ai_response.intent_type.as_str() {
        "work_start" => IntentType::WorkStart,
        "entertainment" => IntentType::Entertainment,
        "focus" => IntentType::Focus,
        "learning" => IntentType::Learning,
        "wind_down" => IntentType::WindDown,
        "query" => IntentType::Query,
        _ => IntentType::Unknown,
    };
    
    let actions = ai_response.actions.into_iter().map(|a| {
        let action_type = match a.action_type.as_str() {
            "launch_app" => ActionType::LaunchApp,
            "open_url" => ActionType::OpenUrl,
            "open_file" => ActionType::OpenFile,
            "close_app" => ActionType::CloseApp,
            "show_notification" => ActionType::ShowNotification,
            "execute_workflow" => ActionType::ExecuteWorkflow,
            "toggle_focus_mode" => ActionType::ToggleFocusMode,
            _ => ActionType::ShowNotification,
        };
        Action {
            action_type,
            target: a.target,
            args: a.args,
        }
    }).collect();
    
    Ok(Intent {
        intent_type,
        confidence: ai_response.confidence,
        parameters: HashMap::new(),
        suggested_actions: actions,
    })
}

async fn local_parse_intent(app_handle: &AppHandle, input: &str) -> Intent {
    let input_lower = input.to_lowercase();
    
    // 1. Check for Pattern-based intents (Dynamic)
    // If input contains "work", check if we have a "time_of_day" pattern for now
    if input_lower.contains("work") || input_lower.contains("start") {
         if let Ok(suggestions) = check_time_patterns(app_handle).await {
             if !suggestions.is_empty() {
                 return Intent {
                     intent_type: IntentType::WorkStart,
                     confidence: 0.95,
                     parameters: HashMap::new(),
                     suggested_actions: suggestions,
                 };
             }
         }
    }

    // 2. Hardcoded fallback (existing logic)
    let (intent_type, confidence, actions) = if 
        input_lower.contains("bored") || 
        input_lower.contains("break") ||
        input_lower.contains("entertain")
    {
        (
            IntentType::Entertainment,
            0.85,
            vec![
                Action {
                    action_type: ActionType::ShowNotification,
                    target: "Entertainment suggestions".to_string(),
                    args: vec!["Here are some things you might enjoy".to_string()],
                },
            ],
        )
    } else if 
        input_lower.contains("code") || 
        input_lower.contains("webdev") ||
        input_lower.contains("develop") ||
        input_lower.contains("work")
    {
        (
            IntentType::WorkStart,
            0.90,
            vec![
                Action {
                    action_type: ActionType::LaunchApp,
                    target: "code".to_string(),
                    args: vec![],
                },
            ],
        )
    } else if 
        input_lower.contains("focus") || 
        input_lower.contains("concentrate")
    {
        (
            IntentType::Focus,
            0.85,
            vec![
                Action {
                    action_type: ActionType::ShowNotification,
                    target: "Focus mode".to_string(),
                    args: vec!["Starting focus session".to_string()],
                },
            ],
        )
    } else if 
        input_lower.contains("research") || 
        input_lower.contains("folder")
    {
        (
            IntentType::Learning,
            0.85,
            vec![
                Action {
                    action_type: ActionType::LaunchApp,
                    target: "explorer.exe".to_string(),
                    args: vec!["Documents".to_string()],
                },
            ],
        )
    } else if 
        input_lower.contains("note") || 
        input_lower.contains("write")
    {
        (
            IntentType::WorkStart,
            0.85,
            vec![
                Action {
                    action_type: ActionType::LaunchApp,
                    target: "notepad.exe".to_string(),
                    args: vec![],
                },
            ],
        )
    } else if 
        input_lower.contains("learn") || 
        input_lower.contains("study")
    {
        (
            IntentType::Learning,
            0.85,
            vec![
                Action {
                    action_type: ActionType::OpenUrl,
                    target: "https://github.com".to_string(),
                    args: vec![],
                },
            ],
        )
    } else if 
        input_lower.contains("done") || 
        input_lower.contains("relax") ||
        input_lower.contains("wind down")
    {
        (
            IntentType::WindDown,
            0.85,
            vec![
                Action {
                    action_type: ActionType::ShowNotification,
                    target: "Wind down".to_string(),
                    args: vec!["Time to relax".to_string()],
                },
            ],
        )
    } else {
        (
            IntentType::Unknown,
            0.5,
            vec![],
        )
    };
    
    Intent {
        intent_type,
        confidence,
        parameters: HashMap::new(),
        suggested_actions: actions,
    }
}

async fn check_time_patterns(app_handle: &AppHandle) -> Result<Vec<Action>, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    // Get current hour
    let now = chrono::Local::now();
    let current_hour = now.format("%H").to_string().parse::<u32>().unwrap_or(9);

    let mut stmt = conn.prepare(
        "SELECT pattern_data FROM patterns 
         WHERE pattern_type = 'time_of_day' AND is_active = 1"
    ).map_err(|e| e.to_string())?;
    
    let rows = stmt.query_map([], |row| {
        let blob: Vec<u8> = row.get(0)?;
        Ok(blob)
    }).map_err(|e| e.to_string())?;
    
    let mut actions = Vec::new();
    
    for blob in rows {
        if let Ok(data) = blob {
           // Parse JSON manually or use a struct
           if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&data) {
               if let Some(h) = value.get("hour").and_then(|v| v.as_u64()) {
                   if h as u32 == current_hour {
                       if let Some(app) = value.get("app_name").and_then(|v| v.as_str()) {
                           actions.push(Action {
                               action_type: ActionType::LaunchApp,
                               target: app.to_string(),
                               args: vec![],
                           });
                       }
                   }
               }
           }
        }
    }
    
    if !actions.is_empty() {
        // Add a notification explaining why
        actions.insert(0, Action {
            action_type: ActionType::ShowNotification,
            target: "Routine Detected".to_string(),
            args: vec![format!("It's {} - usually you use these apps:", now.format("%I %p"))], 
        });
    }
    
    Ok(actions)
}

fn load_settings(app_handle: &AppHandle) -> Option<Settings> {
    let data_dir = app_handle.path().app_data_dir().ok()?;
    let config_path = data_dir.join("config").join("settings.json");
    
    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path).ok()?;
        serde_json::from_str(&content).ok()
    } else {
        None
    }
}

#[tauri::command]
pub async fn execute_intent(
    app_handle: AppHandle,
    intent: Intent,
) -> Result<(), String> {
    for action in intent.suggested_actions {
        match action.action_type {
            ActionType::LaunchApp => {
                if cfg!(target_os = "windows") {
                    std::process::Command::new("cmd")
                        .args(["/C", "start", &action.target])
                        .spawn()
                        .map_err(|e| e.to_string())?;
                }
            }
            ActionType::OpenUrl => {
                open::that(&action.target).map_err(|e| e.to_string())?;
            }
            ActionType::ShowNotification => {
                crate::utils::windows::show_system_notification(&app_handle, "IntentFlow", &action.target);
            }
            ActionType::ToggleFocusMode => {
                let enabled = action.target.to_lowercase() == "on" || action.target.to_lowercase() == "true" || action.target.is_empty();
                crate::utils::windows::set_focus_assist(enabled)?;
                
                let msg = if enabled { "Focus mode enabled" } else { "Focus mode disabled" };
                crate::utils::windows::show_system_notification(&app_handle, "Focus Assist", msg);
            }
            _ => {
                log::warn!("Action type {:?} not implemented", action.action_type);
            }
        }
    }
    
    Ok(())
}
