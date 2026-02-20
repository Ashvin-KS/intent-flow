use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::TimeZone;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::models::{DashboardOverview, DashboardTask, ProjectOverview, ContactOverview, Settings, ActivityMetadata};

const DASHBOARD_REFRESH_SECS: u64 = 15 * 60;

#[derive(Debug, Clone, Serialize)]
struct DashboardChatRequest {
    model: String,
    messages: Vec<DashboardChatMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Debug, Clone, Serialize)]
struct DashboardChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DashboardChatResponse {
    choices: Vec<DashboardChoice>,
}

#[derive(Debug, Clone, Deserialize)]
struct DashboardChoice {
    message: DashboardMessage,
}

#[derive(Debug, Clone, Deserialize)]
struct DashboardMessage {
    content: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DashboardLLMOutput {
    summary: Option<String>,
    focus_points: Option<Vec<String>>,
    deadlines: Option<Vec<DashboardTask>>,
    projects: Option<Vec<ProjectOverview>>,
    contacts: Option<Vec<ContactOverview>>,
}

pub fn start_dashboard_engine(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let _ = refresh_dashboard_snapshot(&app_handle).await;
        loop {
            tokio::time::sleep(Duration::from_secs(DASHBOARD_REFRESH_SECS)).await;
            let _ = refresh_dashboard_snapshot(&app_handle).await;
        }
    });
}

pub async fn refresh_dashboard_snapshot(app_handle: &AppHandle) -> Result<DashboardOverview, String> {
    let (date_key, day_start, day_end) = today_bounds_local();
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    let previous_snapshot = load_snapshot_for_date(&conn, &date_key);

    let context = build_today_context(&conn, day_start, day_end)?;
    let settings = load_settings(app_handle).unwrap_or_default();
    let api_key = crate::utils::config::resolve_api_key(&settings.ai.api_key);
    let model = settings.ai.model.clone();
    let now = chrono::Utc::now().timestamp();

    let mut overview = if settings.ai.enabled && !api_key.is_empty() {
        match ai_dashboard_summary(&context, &api_key, &model).await {
            Ok(o) => o,
            Err(e) => {
                let mut fallback = fallback_dashboard_summary(&context);
                fallback.summary = format!("{}\n\n(AI Summary Failed: {})", fallback.summary, e);
                fallback
            }
        }
    } else {
        let mut fallback = fallback_dashboard_summary(&context);
        if api_key.is_empty() {
            fallback.summary = format!("{}\n\n(AI Summary disabled: No API key provided)", fallback.summary);
        } else if !settings.ai.enabled {
            fallback.summary = format!("{}\n\n(AI Summary disabled in settings)", fallback.summary);
        }
        fallback
    };

    overview.projects = enrich_projects_with_file_upgrades(&context, overview.projects);

    let derived_contacts = derive_contacts_from_context(&context);
    if overview.contacts.is_empty() {
        overview.contacts = derived_contacts;
    } else {
        for contact in derived_contacts {
            let exists = overview
                .contacts
                .iter()
                .any(|c| c.name.eq_ignore_ascii_case(&contact.name));
            if !exists {
                overview.contacts.push(contact);
            }
        }
        overview.contacts.truncate(12);
    }

    if let Some(previous) = previous_snapshot {
        overview = merge_dashboard_overview(previous, overview);
    }

    overview.date_key = date_key.clone();
    overview.updated_at = now;

    let serialized = serde_json::to_string(&overview).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO dashboard_snapshots (date_key, summary_json, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(date_key) DO UPDATE SET
             summary_json = excluded.summary_json,
             updated_at = excluded.updated_at",
        rusqlite::params![date_key, serialized, now],
    )
    .map_err(|e| e.to_string())?;

    Ok(overview)
}

pub fn get_dashboard_snapshot(app_handle: &AppHandle) -> Result<Option<DashboardOverview>, String> {
    let (date_key, _, _) = today_bounds_local();
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;

    let result: Result<String, _> = conn.query_row(
        "SELECT summary_json FROM dashboard_snapshots WHERE date_key = ?1",
        [date_key],
        |row| row.get(0),
    );

    match result {
        Ok(json) => serde_json::from_str(&json).map(Some).map_err(|e| e.to_string()),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

fn load_settings(app_handle: &AppHandle) -> Option<Settings> {
    let data_dir = app_handle.path().app_data_dir().ok()?;
    let settings_path = data_dir.join("config").join("settings.json");
    if !settings_path.exists() {
        return Some(Settings::default());
    }
    let data = std::fs::read_to_string(settings_path).ok()?;
    let mut settings: Settings = serde_json::from_str(&data).ok()?;
    crate::utils::config::apply_env_defaults(&mut settings);
    Some(settings)
}

#[derive(Default)]
struct TodayContext {
    total_duration: i64,
    top_apps: Vec<(String, i64)>,
    ocr_snippets: Vec<String>,
    entries: Vec<(String, String, String)>,
    file_changes: Vec<(String, String, String, String, String, i64)>, // path, root, entity, change_type, preview, detected_at
    communication_events: Vec<(String, String, String, i64)>,
    chat_turns: Vec<(String, String, i64)>, // user, assistant, assistant timestamp
}

fn build_today_context(conn: &Connection, day_start: i64, day_end: i64) -> Result<TodayContext, String> {
    let mut ctx = TodayContext::default();

    let mut stmt = conn
        .prepare(
            "SELECT app_name, window_title, start_time, duration_seconds, metadata
             FROM activities
             WHERE start_time >= ?1 AND start_time < ?2
             ORDER BY start_time DESC
             LIMIT 400",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(rusqlite::params![day_start, day_end], |row| {
            let app: String = row.get(0)?;
            let window_title: String = row.get(1)?;
            let start_time: i64 = row.get(2)?;
            let duration: i64 = row.get::<_, i32>(3)? as i64;
            let metadata_blob: Option<Vec<u8>> = row.get(4)?;
            let ocr = metadata_blob
                .as_ref()
                .and_then(|blob| serde_json::from_slice::<ActivityMetadata>(blob).ok())
                .and_then(|m| m.screen_text)
                .unwrap_or_default();
            Ok((app, window_title, start_time, duration, ocr))
        })
        .map_err(|e| e.to_string())?;

    let mut app_totals: HashMap<String, i64> = HashMap::new();
    for row in rows.filter_map(|r| r.ok()) {
        let app = row.0;
        let window_title = row.1;
        let start_time = row.2;
        let duration = row.3;
        let ocr = row.4;

        ctx.total_duration += duration;
        *app_totals.entry(app.clone()).or_insert(0) += duration;
        if !ocr.trim().is_empty() && ctx.ocr_snippets.len() < 80 {
            let snippet = ocr.chars().take(180).collect::<String>();
            ctx.ocr_snippets.push(snippet);
        }
        if is_communication_app(&app)
            && (!window_title.trim().is_empty() || !ocr.trim().is_empty())
            && ctx.communication_events.len() < 240
        {
            ctx.communication_events.push((
                app,
                window_title.chars().take(160).collect::<String>(),
                ocr.chars().take(220).collect::<String>(),
                start_time,
            ));
        }
    }

    let mut top_apps: Vec<(String, i64)> = app_totals.into_iter().collect();
    top_apps.sort_by(|a, b| b.1.cmp(&a.1));
    ctx.top_apps = top_apps.into_iter().take(8).collect();

    let mut entry_stmt = conn
        .prepare(
            "SELECT title, COALESCE(content, ''), status
             FROM manual_entries
             WHERE status != 'archived'
             ORDER BY updated_at DESC
             LIMIT 60",
        )
        .map_err(|e| e.to_string())?;
    let entry_rows = entry_stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    ctx.entries = entry_rows.filter_map(|r| r.ok()).collect();

    let mut file_stmt = conn
        .prepare(
            "SELECT path, project_root, entity_type, change_type, COALESCE(content_preview, ''), detected_at
             FROM code_file_events
             WHERE detected_at >= ?1 AND detected_at < ?2
             ORDER BY detected_at DESC
             LIMIT 200",
        )
        .map_err(|e| e.to_string())?;
    let file_rows = file_stmt
        .query_map(rusqlite::params![day_start, day_end], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    ctx.file_changes = file_rows.filter_map(|r| r.ok()).collect();

    let mut chat_stmt = conn
        .prepare(
            "SELECT role, content, created_at
             FROM chat_messages
             WHERE created_at >= ?1 AND created_at < ?2
             ORDER BY created_at ASC
             LIMIT 600",
        )
        .map_err(|e| e.to_string())?;
    let chat_rows = chat_stmt
        .query_map(rusqlite::params![day_start, day_end], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut pending_user: Option<String> = None;
    for row in chat_rows.filter_map(|r| r.ok()) {
        let role = row.0.to_lowercase();
        let content = row.1;
        let ts = row.2;
        if role == "user" {
            pending_user = Some(content);
        } else if role == "assistant" {
            if let Some(user_msg) = pending_user.take() {
                ctx.chat_turns.push((
                    user_msg.chars().take(240).collect::<String>(),
                    content.chars().take(280).collect::<String>(),
                    ts,
                ));
                if ctx.chat_turns.len() >= 80 {
                    break;
                }
            }
        }
    }

    Ok(ctx)
}

async fn ai_dashboard_summary(
    context: &TodayContext,
    api_key: &str,
    model: &str,
) -> Result<DashboardOverview, String> {
    let prompt = format!(
        "Build a personal dashboard from today's data only.\n\
Return strict JSON with keys: summary (string), focus_points (string[]), deadlines ([{{title,due_date,status,source}}]), \
projects ([{{name,update,files_changed}}]), contacts ([{{name,context,last_seen}}]).\n\
Keep response factual and concise. The summary should be a comprehensive paragraph summarizing the user's overall activity, including project updates, file changes, music/songs listened to (if any), ongoing projects, and chat interactions.\n\n\
Top apps: {:?}\n\
Total tracked seconds: {}\n\
Entries: {:?}\n\
Recent file changes: {:?}\n\
OCR snippets: {:?}\n\
Communication events: {:?}\n\
Chat turns: {:?}",
        context.top_apps,
        context.total_duration,
        context.entries,
        context.file_changes.iter().take(80).collect::<Vec<_>>(),
        context.ocr_snippets.iter().take(60).collect::<Vec<_>>(),
        context.communication_events.iter().take(120).collect::<Vec<_>>(),
        context.chat_turns.iter().rev().take(50).collect::<Vec<_>>(),
    );

    let request = DashboardChatRequest {
        model: model.to_string(),
        messages: vec![
            DashboardChatMessage {
                role: "system".to_string(),
                content: "You are an activity analyst that outputs strict JSON only.".to_string(),
            },
            DashboardChatMessage {
                role: "user".to_string(),
                content: prompt,
            },
        ],
        temperature: 0.2,
        max_tokens: 900,
    };

    let client = reqwest::Client::new();
    let response = client
        .post("https://integrate.api.nvidia.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("dashboard API request failed: {}", e))?;

    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("dashboard API error {}: {}", status, text));
    }

    let parsed: DashboardChatResponse = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let content = parsed
        .choices
        .first()
        .and_then(|c| c.message.content.clone())
        .ok_or_else(|| "dashboard AI returned empty content".to_string())?;
        
    let clean_content = content.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    let payload: DashboardLLMOutput = serde_json::from_str(clean_content).map_err(|e| format!("JSON parse error: {} - Content: {}", e, clean_content))?;

    Ok(DashboardOverview {
        date_key: String::new(),
        summary: payload.summary.unwrap_or_else(|| "No summary generated yet.".to_string()),
        focus_points: payload.focus_points.unwrap_or_default(),
        deadlines: payload.deadlines.unwrap_or_default().into_iter().take(10).collect(),
        projects: payload.projects.unwrap_or_default().into_iter().take(10).collect(),
        contacts: payload.contacts.unwrap_or_default().into_iter().take(10).collect(),
        updated_at: 0,
    })
}

fn fallback_dashboard_summary(context: &TodayContext) -> DashboardOverview {
    let summary = if context.top_apps.is_empty() {
        "No tracked activity yet today.".to_string()
    } else {
        let apps = context
            .top_apps
            .iter()
            .take(3)
            .map(|(name, dur)| format!("{} ({}m)", name, dur / 60))
            .collect::<Vec<_>>()
            .join(", ");
        format!("Today you mostly worked in {}.", apps)
    };

    let deadlines = context
        .entries
        .iter()
        .filter(|(_, content, status)| {
            status != "completed"
                && (content.to_lowercase().contains("deadline")
                    || content.to_lowercase().contains("due")
                    || content.to_lowercase().contains("by "))
        })
        .take(8)
        .map(|(title, content, status)| DashboardTask {
            title: title.clone(),
            due_date: extract_due_hint(content),
            status: status.clone(),
            source: "entry".to_string(),
        })
        .collect::<Vec<_>>();

    let projects = summarize_projects_from_file_changes(context, 8);

    let contacts = derive_contacts_from_context(context);
    let comm_events_count = context.communication_events.len();
    let file_change_count = context.file_changes.len();
    let chat_turn_count = context.chat_turns.len();

    DashboardOverview {
        date_key: String::new(),
        summary,
        focus_points: vec![
            "Summary uses only today's tracked activity".to_string(),
            format!("Detected {} communication events today", comm_events_count),
            format!("Tracked {} file/code changes today", file_change_count),
            format!("Reviewed {} chat turn(s) for recap", chat_turn_count),
        ],
        deadlines,
        projects,
        contacts,
        updated_at: 0,
    }
}

fn extract_due_hint(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    for marker in ["due ", "deadline ", "by "] {
        if let Some(idx) = lower.find(marker) {
            let snippet = text[idx..].chars().take(40).collect::<String>();
            return Some(snippet.trim().to_string());
        }
    }
    None
}

fn is_communication_app(app: &str) -> bool {
    let lower = app.to_lowercase();
    lower.contains("whatsapp")
        || lower.contains("teams")
        || lower.contains("telegram")
        || lower.contains("discord")
        || lower.contains("slack")
        || lower.contains("zoom")
}

fn derive_contacts_from_context(context: &TodayContext) -> Vec<ContactOverview> {
    let mut by_name: HashMap<String, (i32, i64, String)> = HashMap::new();

    for (app, title, ocr, start_time) in &context.communication_events {
        let mut candidates = extract_contact_candidates(title);
        candidates.extend(extract_contact_candidates(ocr));

        for candidate in candidates {
            let normalized = normalize_contact_name(&candidate);
            if normalized.is_empty() {
                continue;
            }
            let key = normalized.to_lowercase();
            let entry = by_name.entry(key).or_insert((0, 0, app.clone()));
            entry.0 += 1;
            if *start_time > entry.1 {
                entry.1 = *start_time;
            }
        }
    }

    let mut contacts: Vec<ContactOverview> = by_name
        .into_iter()
        .map(|(key, (count, last_seen, app))| ContactOverview {
            name: key
                .split_whitespace()
                .map(|w| {
                    let mut chars = w.chars();
                    match chars.next() {
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
            context: format!("{} interaction(s) in {}", count, app),
            last_seen: Some(last_seen),
        })
        .collect();

    contacts.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
    contacts.truncate(10);
    contacts
}

fn extract_contact_candidates(text: &str) -> Vec<String> {
    let cleaned = text.replace('\n', " ");
    let mut out = Vec::new();

    if let Some(part) = cleaned.split('|').next() {
        let p = part.trim();
        if looks_like_human_name(p) {
            out.push(p.to_string());
        }
    }

    if let Some((left, right)) = cleaned.split_once(" - ") {
        let app_hint = right.to_lowercase();
        if (app_hint.contains("whatsapp")
            || app_hint.contains("telegram")
            || app_hint.contains("teams")
            || app_hint.contains("discord")
            || app_hint.contains("slack"))
            && looks_like_human_name(left.trim())
        {
            out.push(left.trim().to_string());
        }
    }

    out
}

fn looks_like_human_name(text: &str) -> bool {
    let t = text.trim();
    if t.len() < 3 || t.len() > 64 {
        return false;
    }
    let lower = t.to_lowercase();
    let banned = [
        "intentflow",
        "visual studio",
        "windows explorer",
        "task switching",
        "file edit selection",
        "whatsapp",
        "microsoft teams",
        "telegram",
        "discord",
        "slack",
        "zoom",
        "chrome",
        "brave",
        "edge",
        "firefox",
        "inbox",
        "new tab",
    ];
    if banned.iter().any(|b| lower.contains(b)) {
        return false;
    }

    let words: Vec<&str> = t.split_whitespace().collect();
    if words.len() < 2 || words.len() > 5 {
        return false;
    }
    words.iter().all(|w| w.chars().any(|c| c.is_alphabetic()))
}

fn normalize_contact_name(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '.')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn summarize_projects_from_file_changes(context: &TodayContext, limit: usize) -> Vec<ProjectOverview> {
    #[derive(Default)]
    struct ProjectStat {
        files_changed: i32,
        created: i32,
        modified: i32,
        deleted: i32,
        touched_areas: HashSet<String>,
        update_snippets: Vec<String>,
    }

    let mut by_project: HashMap<String, ProjectStat> = HashMap::new();
    for (path, root, entity_type, change_type, preview, _) in &context.file_changes {
        if is_noise_file_change(path, root, preview) {
            continue;
        }
        let stat = by_project.entry(root.clone()).or_default();
        stat.files_changed += 1;
        match change_type.to_lowercase().as_str() {
            "created" => stat.created += 1,
            "deleted" => stat.deleted += 1,
            _ => stat.modified += 1,
        }
        if let Some(area) = derive_area_from_path(path, root) {
            stat.touched_areas.insert(area);
        }
        if !preview.trim().is_empty() && stat.update_snippets.len() < 3 {
            let snippet = sanitize_preview_snippet(preview);
            if !snippet.is_empty() && !stat.update_snippets.contains(&snippet) {
                let prefixed = format!("{} {}: {}", entity_type, change_type, snippet);
                stat.update_snippets.push(prefixed);
            }
        }
    }

    let mut projects = by_project
        .into_iter()
        .map(|(name, stat)| {
            let top_areas = stat
                .touched_areas
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            let mut update = if top_areas.is_empty() {
                format!(
                    "Upgrades today: {} modified, {} created, {} deleted.",
                    stat.modified, stat.created, stat.deleted
                )
            } else {
                format!(
                    "Upgrades today: {} modified, {} created, {} deleted. Touched: {}.",
                    stat.modified, stat.created, stat.deleted, top_areas
                )
            };
            if !stat.update_snippets.is_empty() {
                let details = stat.update_snippets.join(" | ");
                update.push_str(&format!(" Key updates: {}.", details));
            }
            ProjectOverview {
                name,
                update,
                files_changed: stat.files_changed,
            }
        })
        .collect::<Vec<_>>();

    projects.sort_by(|a, b| b.files_changed.cmp(&a.files_changed));
    projects.truncate(limit);
    projects
}

fn build_chat_recap(context: &TodayContext) -> String {
    if context.chat_turns.is_empty() {
        return String::new();
    }
    let mut lines = Vec::new();
    let mut seen_users: HashSet<String> = HashSet::new();
    for (idx, (user, assistant, _)) in context.chat_turns.iter().rev().take(6).enumerate() {
        let clean_user = sanitize_chat_text(user);
        let clean_assistant = sanitize_chat_text(assistant);
        if clean_user.is_empty() || clean_assistant.is_empty() {
            continue;
        }
        if !seen_users.insert(clean_user.clone()) {
            continue;
        }
        lines.push(format!(
            "{}. You asked: {} | AI answered: {}",
            idx + 1,
            clean_user.chars().take(120).collect::<String>(),
            clean_assistant.chars().take(140).collect::<String>()
        ));
    }
    lines.join(" ")
}

fn derive_area_from_path(path: &str, project_root: &str) -> Option<String> {
    let normalized_path = path.replace('\\', "/");
    let normalized_root = project_root.replace('\\', "/");
    let relative = normalized_path
        .strip_prefix(&(normalized_root.clone() + "/"))
        .unwrap_or(&normalized_path);
    let mut parts = relative.split('/').filter(|p| !p.is_empty());
    let first = parts.next()?;
    let second = parts.next();
    Some(match second {
        Some(s) => format!("{}/{}", first, s),
        None => first.to_string(),
    })
}

fn enrich_projects_with_file_upgrades(
    context: &TodayContext,
    existing: Vec<ProjectOverview>,
) -> Vec<ProjectOverview> {
    let computed = summarize_projects_from_file_changes(context, 12);
    let mut computed_map: HashMap<String, ProjectOverview> = HashMap::new();
    for p in computed {
        computed_map.insert(p.name.clone(), p);
    }

    let mut merged = existing;
    for item in &mut merged {
        // Try to find a matching project by checking if the computed path ends with the LLM project name
        let matched_key = computed_map.keys().find(|k| {
            let k_lower = k.to_lowercase();
            let item_lower = item.name.to_lowercase();
            k_lower.ends_with(&item_lower) || item_lower.ends_with(&k_lower) || k_lower.contains(&item_lower)
        }).cloned();

        if let Some(key) = matched_key {
            if let Some(c) = computed_map.remove(&key) {
                item.files_changed = c.files_changed;
                item.update = c.update.clone();
                // Optionally update the name to the more descriptive one or keep the LLM one
            }
        }
    }

    for (_, c) in computed_map {
        // Double check just in case
        let exists = merged.iter().any(|m| {
            let m_lower = m.name.to_lowercase();
            let c_lower = c.name.to_lowercase();
            c_lower.ends_with(&m_lower) || m_lower.ends_with(&c_lower) || c_lower.contains(&m_lower)
        });
        if !exists {
            merged.push(c);
        }
    }

    merged.sort_by(|a, b| b.files_changed.cmp(&a.files_changed));
    merged.truncate(12);
    merged
}

fn sanitize_chat_text(text: &str) -> String {
    let mut out = text.replace('\n', " ");
    if let Some(start) = out.to_lowercase().find("<think>") {
        if let Some(end_rel) = out.to_lowercase()[start..].find("</think>") {
            let end = start + end_rel + "</think>".len();
            out.replace_range(start..end, " ");
        } else {
            out.truncate(start);
        }
    }
    let lower = out.to_lowercase();
    if lower.contains("\"tool\"") && lower.contains("\"args\"") {
        return String::new();
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn sanitize_preview_snippet(preview: &str) -> String {
    let cleaned = preview
        .replace('\n', " ")
        .replace("Initial content:", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if cleaned.contains("\\\\.cargo\\registry")
        || cleaned.contains("index.crates.io")
        || cleaned.contains("\"rustc\":")
        || cleaned.contains("\"features\":")
        || cleaned.contains("build_script")
    {
        return String::new();
    }
    cleaned.chars().take(140).collect::<String>()
}

fn is_noise_file_change(path: &str, root: &str, preview: &str) -> bool {
    let p = path.to_lowercase();
    let r = root.to_lowercase();
    if p.contains("/target/") || p.contains("\\target\\") || r.ends_with("/target") || r.ends_with("\\target") {
        return true;
    }
    if p.contains("\\\\.cargo\\registry") || p.contains("index.crates.io") || r.contains("\\\\.cargo\\registry") {
        return true;
    }
    if p.contains("node_modules") {
        return true;
    }
    let pv = preview.to_lowercase();
    pv.contains("\"rustc\":") || pv.contains("build_script") || pv.contains("index.crates.io")
}

fn load_snapshot_for_date(conn: &Connection, date_key: &str) -> Option<DashboardOverview> {
    let result: Result<String, _> = conn.query_row(
        "SELECT summary_json FROM dashboard_snapshots WHERE date_key = ?1",
        [date_key],
        |row| row.get(0),
    );
    match result {
        Ok(json) => serde_json::from_str::<DashboardOverview>(&json).ok(),
        Err(_) => None,
    }
}

fn merge_dashboard_overview(previous: DashboardOverview, mut fresh: DashboardOverview) -> DashboardOverview {
    if fresh.summary.trim().is_empty() {
        fresh.summary = previous.summary;
    }

    let mut merged_focus = Vec::new();
    for point in fresh.focus_points.into_iter().chain(previous.focus_points.into_iter()) {
        if !point.trim().is_empty() && !merged_focus.contains(&point) {
            merged_focus.push(point);
        }
    }
    fresh.focus_points = merged_focus.into_iter().take(10).collect();

    let mut merged_deadlines = fresh.deadlines;
    for item in previous.deadlines {
        let exists = merged_deadlines.iter().any(|d| {
            d.title.eq_ignore_ascii_case(&item.title) && d.source.eq_ignore_ascii_case(&item.source)
        });
        if !exists {
            merged_deadlines.push(item);
        }
    }
    fresh.deadlines = merged_deadlines.into_iter().take(12).collect();

    let mut merged_projects = fresh.projects;
    for item in previous.projects {
        if let Some(existing) = merged_projects
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(&item.name))
        {
            if existing.files_changed < item.files_changed {
                existing.files_changed = item.files_changed;
                existing.update = item.update.clone();
            }
        } else {
            merged_projects.push(item);
        }
    }
    merged_projects.sort_by(|a, b| b.files_changed.cmp(&a.files_changed));
    fresh.projects = merged_projects.into_iter().take(12).collect();

    let mut merged_contacts = fresh.contacts;
    for item in previous.contacts {
        let exists = merged_contacts
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(&item.name));
        if !exists {
            merged_contacts.push(item);
        }
    }
    fresh.contacts = merged_contacts.into_iter().take(12).collect();

    fresh
}

fn today_bounds_local() -> (String, i64, i64) {
    let now = chrono::Local::now();
    let date = now.date_naive();
    let start_local = chrono::Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).expect("valid local midnight"))
        .single()
        .unwrap_or(now);
    let start = start_local.timestamp();
    let end = start + 24 * 3600;
    (date.format("%Y-%m-%d").to_string(), start, end)
}

async fn call_llm_for_summary(api_key: &str, model: &str, prompt: &str) -> Result<String, String> {
    let request = DashboardChatRequest {
        model: model.to_string(),
        messages: vec![
            DashboardChatMessage {
                role: "system".to_string(),
                content: "You are a helpful assistant that summarizes context into a concise, readable paragraph. Do not use markdown formatting like bold or italics, just plain text.".to_string(),
            },
            DashboardChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            },
        ],
        temperature: 0.3,
        max_tokens: 300,
    };

    let client = reqwest::Client::new();
    let response = client
        .post("https://integrate.api.nvidia.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("API request failed: {}", e))?;

    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("API error {}: {}", status, text));
    }

    let parsed: DashboardChatResponse = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let content = parsed
        .choices
        .first()
        .and_then(|c| c.message.content.clone())
        .ok_or_else(|| "AI returned empty content".to_string())?;
    
    Ok(content.trim().to_string())
}

pub async fn summarize_contact(app_handle: &AppHandle, name: &str) -> Result<String, String> {
    let settings = load_settings(app_handle).unwrap_or_default();
    let api_key = crate::utils::config::resolve_api_key(&settings.ai.api_key);
    let model = settings.ai.model.clone();

    if !settings.ai.enabled || api_key.is_empty() {
        return Ok(format!("AI is disabled or API key is missing. Cannot summarize {}.", name));
    }

    let mut context_data = Vec::new();
    {
        let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
        let db_path = data_dir.join("intentflow.db");
        let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;

        let now = chrono::Local::now().timestamp();
        let start = now - (3 * 24 * 3600);

        let mut stmt = conn.prepare(
            "SELECT app_name, window_title, metadata 
             FROM activities 
             WHERE start_time >= ?1 AND start_time < ?2 
             AND (window_title LIKE ?3 OR metadata LIKE ?3)
             ORDER BY start_time DESC LIMIT 50"
        ).map_err(|e| e.to_string())?;

        let name_pattern = format!("%{}%", name);
        let rows = stmt.query_map(rusqlite::params![start, now, name_pattern], |row| {
            let app: String = row.get(0)?;
            let title: String = row.get(1)?;
            let metadata: Option<Vec<u8>> = row.get(2)?;
            let ocr = metadata
                .as_ref()
                .and_then(|blob| serde_json::from_slice::<ActivityMetadata>(blob).ok())
                .and_then(|m| m.screen_text)
                .unwrap_or_default();
            Ok((app, title, ocr))
        }).map_err(|e| e.to_string())?;

        for row in rows.filter_map(|r| r.ok()) {
            let ocr_snippet = row.2.chars().take(200).collect::<String>();
            let clean_ocr = ocr_snippet.replace('\n', " ");
            context_data.push(format!("App: {}, Title: {}, OCR: {}", row.0, row.1, clean_ocr));
        }
    }

    if context_data.is_empty() {
        return Ok(format!("No recent interactions found for {}.", name));
    }

    let prompt = format!(
        "Summarize the recent interactions and context for the contact '{}'.\n\
        Keep it concise, factual, and highlight any action items or key topics discussed.\n\n\
        Recent data:\n{}",
        name,
        context_data.join("\n")
    );

    call_llm_for_summary(&api_key, &model, &prompt).await
}

pub async fn summarize_project(app_handle: &AppHandle, name: &str) -> Result<String, String> {
    let settings = load_settings(app_handle).unwrap_or_default();
    let api_key = crate::utils::config::resolve_api_key(&settings.ai.api_key);
    let model = settings.ai.model.clone();

    if !settings.ai.enabled || api_key.is_empty() {
        return Ok(format!("AI is disabled or API key is missing. Cannot summarize {}.", name));
    }

    let mut context_data = Vec::new();
    {
        let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
        let db_path = data_dir.join("intentflow.db");
        let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;

        let now = chrono::Local::now().timestamp();
        let start = now - (3 * 24 * 3600);

        let mut stmt = conn.prepare(
            "SELECT path, entity_type, change_type, COALESCE(content_preview, '')
             FROM code_file_events 
             WHERE detected_at >= ?1 AND detected_at < ?2 
             AND project_root LIKE ?3
             ORDER BY detected_at DESC LIMIT 100"
        ).map_err(|e| e.to_string())?;

        let name_pattern = format!("%{}%", name);
        let rows = stmt.query_map(rusqlite::params![start, now, name_pattern], |row| {
            let path: String = row.get(0)?;
            let entity_type: String = row.get(1)?;
            let change_type: String = row.get(2)?;
            let preview: String = row.get(3)?;
            Ok((path, entity_type, change_type, preview))
        }).map_err(|e| e.to_string())?;

        for row in rows.filter_map(|r| r.ok()) {
            let preview_snippet = row.3.chars().take(150).collect::<String>();
            let clean_preview = preview_snippet.replace('\n', " ");
            context_data.push(format!("File: {} ({}), Change: {}, Preview: {}", row.0, row.1, row.2, clean_preview));
        }
    }

    if context_data.is_empty() {
        return Ok(format!("No recent file changes found for project {}.", name));
    }

    let prompt = format!(
        "Summarize the recent development activity for the project '{}'.\n\
        Keep it concise, factual, and highlight the main areas of work or features being developed based on the file changes.\n\n\
        Recent data:\n{}",
        name,
        context_data.join("\n")
    );

    call_llm_for_summary(&api_key, &model, &prompt).await
}
