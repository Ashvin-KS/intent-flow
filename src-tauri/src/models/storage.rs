use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    pub total_size_bytes: i64,
    pub activities_count: i64,
    pub summaries_count: i64,
    pub patterns_count: i64,
    pub entries_count: i64,
    pub oldest_activity: i64,
    pub newest_activity: i64,
}
