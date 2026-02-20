use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DashboardTask {
    pub title: String,
    pub due_date: Option<String>,
    pub status: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectOverview {
    pub name: String,
    pub update: String,
    pub files_changed: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContactOverview {
    pub name: String,
    pub context: String,
    pub last_seen: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DashboardOverview {
    pub date_key: String,
    pub summary: String,
    pub focus_points: Vec<String>,
    pub deadlines: Vec<DashboardTask>,
    pub projects: Vec<ProjectOverview>,
    pub contacts: Vec<ContactOverview>,
    pub updated_at: i64,
}
