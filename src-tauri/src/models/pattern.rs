use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: i64,
    pub pattern_type: String,
    pub pattern_data: PatternData,
    pub confidence: f32,
    pub last_observed: i64,
    pub occurrence_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternData {
    // Time-based pattern
    pub hour: Option<i32>,
    pub day_of_week: Option<i32>,
    pub likely_activities: Option<Vec<String>>,
    
    // Sequence pattern
    pub sequence: Option<Vec<String>>,
    pub avg_gap_seconds: Option<i64>,
    
    // Context pattern
    pub trigger_activity: Option<String>,
    pub following_activities: Option<Vec<String>>,
    
    // Mood pattern
    pub idle_duration: Option<i64>,
    pub time_of_day: Option<i32>,
    pub likely_intent: Option<String>,
}
