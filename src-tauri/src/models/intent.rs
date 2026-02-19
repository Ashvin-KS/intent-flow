use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    pub intent_type: IntentType,
    pub confidence: f32,
    pub parameters: HashMap<String, String>,
    pub suggested_actions: Vec<Action>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentType {
    WorkStart,
    Entertainment,
    Focus,
    Learning,
    WindDown,
    Query,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub action_type: ActionType,
    pub target: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    LaunchApp,
    OpenUrl,
    OpenFile,
    CloseApp,
    ShowNotification,
    ExecuteWorkflow,
    ToggleFocusMode,
}
