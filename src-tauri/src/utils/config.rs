use crate::models::Settings;

const ENV_API_KEY: &str = "NVIDIA_API_KEY";

pub fn load_dotenv() {
    let _ = dotenvy::dotenv();
}

pub fn api_key_from_env() -> Option<String> {
    std::env::var(ENV_API_KEY).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

pub fn resolve_api_key(explicit_key: &str) -> String {
    let trimmed = explicit_key.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    api_key_from_env().unwrap_or_default()
}

pub fn apply_env_defaults(settings: &mut Settings) {
    if settings.ai.api_key.trim().is_empty() {
        settings.ai.api_key = api_key_from_env().unwrap_or_default();
    }
}
