use crate::models::{Intent, IntentType, Action, ActionType};
use std::collections::HashMap;

#[tauri::command]
pub async fn parse_intent(
    input: String,
) -> Result<Intent, String> {
    let input_lower = input.to_lowercase();
    
    // Simple pattern matching for intents
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
    
    Ok(Intent {
        intent_type,
        confidence,
        parameters: HashMap::new(),
        suggested_actions: actions,
    })
}

#[tauri::command]
pub async fn execute_intent(
    intent: Intent,
) -> Result<(), String> {
    // Execute the intent actions
    for action in intent.suggested_actions {
        match action.action_type {
            ActionType::LaunchApp => {
                // Launch the application
                if cfg!(target_os = "windows") {
                    std::process::Command::new("cmd")
                        .args(["/C", "start", &action.target])
                        .spawn()
                        .map_err(|e| e.to_string())?;
                }
            }
            ActionType::OpenUrl => {
                // Open URL in default browser
                open::that(&action.target).map_err(|e| e.to_string())?;
            }
            ActionType::ShowNotification => {
                // Show notification (handled by frontend)
                log::info!("Notification: {} - {:?}", action.target, action.args);
            }
            _ => {}
        }
    }
    
    Ok(())
}
