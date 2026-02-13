use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub query: String,
    pub results: Vec<QueryItem>,
    pub summary: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryItem {
    pub timestamp: i64,
    pub time_str: String,
    pub activity: String,
    pub duration: String,
    pub details: Option<String>,
}
