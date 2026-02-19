use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualEntry {
    pub id: i64,
    pub entry_type: String,
    pub title: String,
    pub content: Option<String>,
    pub tags: Vec<String>,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEntryRequest {
    pub entry_type: String,
    pub title: String,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
}
