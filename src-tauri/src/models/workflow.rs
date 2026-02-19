use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub apps: Vec<AppLaunch>,
    pub urls: Vec<String>,
    pub files: Vec<String>,
    pub use_count: i32,
    pub last_used: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppLaunch {
    pub path: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSuggestion {
    pub workflow: Workflow,
    pub trigger_type: String,
    pub relevance_score: f32,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWorkflowRequest {
    pub name: String,
    pub description: String,
    pub icon: String,
    pub apps: Vec<AppLaunch>,
    pub urls: Vec<String>,
    pub files: Vec<String>,
}
