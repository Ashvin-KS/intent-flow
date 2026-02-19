use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActivityMetadata {
    pub is_idle: bool,
    pub is_fullscreen: bool,
    pub process_id: Option<u32>,
    pub url: Option<String>,
    pub screen_text: Option<String>,
    pub background_windows: Option<Vec<String>>,
    pub media_info: Option<MediaInfo>,
    pub raw_duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct MediaInfo {
    pub title: String,
    pub artist: String,
    pub status: String, // "Playing", "Paused", "Stopped"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    pub id: i64,
    pub app_name: String,
    pub app_hash: u64,
    pub window_title: String,
    pub window_title_hash: u64,
    pub category_id: i32,
    pub start_time: i64,
    pub end_time: i64,
    pub duration_seconds: i32,
    pub metadata: Option<ActivityMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityStats {
    pub total_duration: i64,
    pub total_events: i32,
    pub top_apps: Vec<AppStat>,
    pub top_categories: Vec<CategoryStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStat {
    pub app_name: String,
    pub duration: i64,
    pub count: i32,
    pub percentage: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryStat {
    pub category_id: i32,
    pub category_name: String,
    pub duration: i64,
    pub count: i32,
    pub percentage: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub app_name: String,
    pub app_hash: u64,
    pub window_title: String,
    pub window_title_hash: u64,
    pub category_id: i32,
    pub start_time: i64,
    pub end_time: i64,
    pub duration_seconds: i32,
    pub metadata: ActivityMetadata,
}

impl ActivityEvent {
    pub fn new(
        app_name: String,
        window_title: String,
        category_id: i32,
        start_time: i64,
        end_time: i64,
    ) -> Self {
        use twox_hash::XxHash64;
        use std::hash::Hasher;
        
        let mut hasher = XxHash64::default();
        hasher.write(app_name.as_bytes());
        let app_hash = hasher.finish();
        
        let mut hasher = XxHash64::default();
        hasher.write(window_title.to_lowercase().as_bytes());
        let window_title_hash = hasher.finish();
        
        let duration_seconds = (end_time - start_time) as i32;
        
        Self {
            app_name,
            app_hash,
            window_title,
            window_title_hash,
            category_id,
            start_time,
            end_time,
            duration_seconds,
            metadata: ActivityMetadata {
                is_idle: false,
                is_fullscreen: false,
                process_id: None,
                url: None,
                screen_text: None,
                background_windows: None,
                media_info: None,
                raw_duration_ms: None,
            },
        }
    }
}
