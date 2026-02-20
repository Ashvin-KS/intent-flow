use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub version: String,
    pub general: GeneralSettings,
    pub tracking: TrackingSettings,
    pub storage: StorageSettings,
    pub ai: AISettings,
    pub privacy: PrivacySettings,
    pub notifications: NotificationSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: "1.0.0".to_string(),
            general: GeneralSettings::default(),
            tracking: TrackingSettings::default(),
            storage: StorageSettings::default(),
            ai: AISettings::default(),
            privacy: PrivacySettings::default(),
            notifications: NotificationSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralSettings {
    pub language: String,
    pub theme: String,
    #[serde(default = "default_enable_startup")]
    pub enable_startup: bool,
    pub startup_behavior: String,
    pub minimize_to_tray: bool,
    pub close_to_tray: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            theme: "dark".to_string(),
            enable_startup: true,
            startup_behavior: "minimized_to_tray".to_string(),
            minimize_to_tray: true,
            close_to_tray: true,
        }
    }
}

fn default_enable_startup() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackingSettings {
    pub enabled: bool,
    pub tracking_interval: u64,
    pub idle_timeout: u64,
    pub exclude_apps: Vec<String>,
    pub exclude_urls: Vec<String>,
    pub track_browser: bool,
}

impl Default for TrackingSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            tracking_interval: 10,
            idle_timeout: 300,
            exclude_apps: vec![],
            exclude_urls: vec![],
            track_browser: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSettings {
    pub retention_days: i32,
    pub auto_cleanup: bool,
    pub compression_enabled: bool,
    pub max_cache_size_mb: i32,
}

impl Default for StorageSettings {
    fn default() -> Self {
        Self {
            retention_days: 365,
            auto_cleanup: true,
            compression_enabled: true,
            max_cache_size_mb: 512,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AISettings {
    pub enabled: bool,
    pub provider: String,
    pub api_key: String,
    pub model: String,
    pub local_only: bool,
    pub fallback_to_local: bool,
}

impl Default for AISettings {
    fn default() -> Self {
        let env_key = std::env::var("NVIDIA_API_KEY").unwrap_or_default();
        Self {
            enabled: true,
            provider: "nvidia".to_string(),
            api_key: env_key,
            model: "moonshotai/kimi-k2-instruct-0905".to_string(),
            local_only: false,
            fallback_to_local: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacySettings {
    pub encrypt_database: bool,
    pub exclude_incognito: bool,
    pub anonymize_data: bool,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            encrypt_database: false,
            exclude_incognito: true,
            anonymize_data: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSettings {
    pub workflow_suggestions: bool,
    pub pattern_insights: bool,
    pub daily_summary: bool,
    pub summary_time: String,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            workflow_suggestions: true,
            pattern_insights: true,
            daily_summary: true,
            summary_time: "09:00".to_string(),
        }
    }
}
