use tauri::{AppHandle, Manager};
use crate::models::{QueryResult, QueryItem, Settings};
use chrono::{Datelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── AI types for query summarization ───

#[derive(Serialize)]
struct QueryChatRequest {
    model: String,
    messages: Vec<QueryChatSendMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Serialize)]
struct QueryChatSendMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct QueryChatResponse {
    choices: Vec<QueryChatChoice>,
}

#[derive(Deserialize)]
struct QueryChatChoice {
    message: QueryChatRecvMessage,
}

#[derive(Deserialize)]
struct QueryChatRecvMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
}

// ─── Settings ───

fn load_settings(app_handle: &AppHandle) -> Option<Settings> {
    let data_dir = app_handle.path().app_data_dir().ok()?;
    let settings_path = data_dir.join("settings.json");
    let data = std::fs::read_to_string(settings_path).ok()?;
    serde_json::from_str(&data).ok()
}

// ─── AI System Prompt ───

const SYSTEM_PROMPT: &str = r#"You are IntentFlow's AI activity analyst — a smart, conversational assistant embedded inside the IntentFlow desktop app. IntentFlow tracks the user's active window on their computer every 5 seconds and logs what apps and windows they use throughout the day.

## Your Purpose
Analyze the user's tracked activity data and answer their questions about how they spent their time. You should provide insightful, specific, conversational answers — like a helpful productivity coach.

## Data You Receive
You will receive:
- Aggregated stats: time per app, time per category, and activity timeline blocks
- Raw activity entries (most recent, up to ~50)
- OCR screen text: text extracted from the user's screen via periodic screenshots (this may include contact names, chat messages, webpage content, code, etc.)
- The user's original question (which may contain typos — interpret intent, don't be literal)

## Activity Categories
1 = Development (VS Code, Antigravity/Cursor IDE, terminals, code editors)
2 = Browser (Chrome, Brave, Firefox, Edge — general web browsing)
3 = Communication (Slack, Discord, Teams, WhatsApp, email)
4 = Entertainment (Spotify, YouTube, Netflix, music/video streaming)
5 = Productivity (Notion, Obsidian, Word, Excel, note-taking)
6 = System (File Explorer, Settings, Task Manager)
7 = Other/Unknown

## Response Rules
1. Be concise but insightful — 2-4 sentences, conversational tone
2. Always mention specific times, apps, and durations when relevant
3. If the user asks about productivity, analyze the ratio of productive (Development, Productivity) vs non-productive (Entertainment, Browser) time
4. If no activities match, say so helpfully — suggest what they might search for instead
5. Do NOT use markdown formatting (no **, ##, -, etc.)
6. Handle typos gracefully — "prodcutivity" means "productivity", "musics" means "music", etc.
7. If the query is unclear or gibberish, give a general summary of the time period
8. When the user asks "what did I do", give a narrative overview of their session
9. For music queries, mention song names from window titles if visible
10. Use 12-hour time format (e.g., 9:06 PM, not 21:06)
11. When OCR screen text is available, USE it to provide richer answers — it contains actual on-screen content like contact names in chat apps, webpage text, code being written, etc.
12. For WhatsApp or messaging queries, look at the OCR screen text for contact names, message snippets, and conversation details"#;

// ─── AI call ───

async fn ai_summarize_query(
    query: &str,
    structured_data: &str,
    api_key: &str,
    model: &str,
) -> Result<String, String> {
    let user_message = format!(
        "User's question: \"{}\"\n\n{}",
        query, structured_data
    );

    let request = QueryChatRequest {
        model: model.to_string(),
        messages: vec![
            QueryChatSendMessage {
                role: "system".to_string(),
                content: SYSTEM_PROMPT.to_string(),
            },
            QueryChatSendMessage {
                role: "user".to_string(),
                content: user_message,
            },
        ],
        temperature: 0.7,
        max_tokens: 512,
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
    let body_text = response.text().await
        .map_err(|e| format!("Failed to read response: {}", e))?;
    
    if !status.is_success() {
        return Err(format!("API returned {}: {}", status, &body_text[..body_text.len().min(300)]));
    }

    let chat_resp: QueryChatResponse = serde_json::from_str(&body_text)
        .map_err(|e| format!("JSON parse error: {} | Body: {}", e, &body_text[..body_text.len().min(300)]))?;

    let choice = chat_resp.choices.first()
        .ok_or_else(|| "Empty AI response".to_string())?;
    
    // Try content first, then reasoning_content (for reasoning models like GLM)
    choice.message.content.clone()
        .or_else(|| choice.message.reasoning_content.clone())
        .ok_or_else(|| "AI returned null content".to_string())
}

// ─── Build structured data for AI ───

fn build_structured_data(
    filtered: &[crate::models::Activity],
    total_duration: i32,
    time_range_label: &str,
) -> String {
    if filtered.is_empty() {
        return format!("Time range: {}\nNo activities found in this time range.", time_range_label);
    }
    
    let local_tz = chrono::Local::now().timezone();
    let mut data = String::new();
    
    // Header
    data.push_str(&format!(
        "Time range: {}\nTotal: {} activities, {} tracked time\n\n",
        time_range_label, filtered.len(), format_duration(total_duration)
    ));
    
    // ── Per-app breakdown ──
    let mut app_stats: HashMap<String, (i32, i32)> = HashMap::new(); // (total_seconds, count)
    for a in filtered {
        let entry = app_stats.entry(a.app_name.clone()).or_insert((0, 0));
        entry.0 += a.duration_seconds;
        entry.1 += 1;
    }
    let mut app_list: Vec<_> = app_stats.into_iter().collect();
    app_list.sort_by(|a, b| b.1.0.cmp(&a.1.0)); // sort by duration desc
    
    data.push_str("=== TIME PER APP ===\n");
    for (app, (dur, count)) in app_list.iter().take(10) {
        data.push_str(&format!("  {} — {} ({} sessions)\n", app, format_duration(*dur), count));
    }
    
    // ── Per-category breakdown ──
    let mut cat_stats: HashMap<i32, i32> = HashMap::new();
    for a in filtered {
        *cat_stats.entry(a.category_id).or_insert(0) += a.duration_seconds;
    }
    let mut cat_list: Vec<_> = cat_stats.into_iter().collect();
    cat_list.sort_by(|a, b| b.1.cmp(&a.1));
    
    data.push_str("\n=== TIME PER CATEGORY ===\n");
    for (cat_id, dur) in &cat_list {
        let name = category_name(*cat_id);
        let pct = if total_duration > 0 { (*dur as f64 / total_duration as f64 * 100.0) as i32 } else { 0 };
        data.push_str(&format!("  {} — {} ({}%)\n", name, format_duration(*dur), pct));
    }
    
    // ── Recent activity timeline (up to 50 entries) ──
    // DB returns DESC (newest first), so .take(50) gets the 50 most recent
    // Then collect and reverse for chronological display
    let recent: Vec<_> = filtered.iter().take(50).collect();
    data.push_str("\n=== ACTIVITY TIMELINE ===\n");
    for a in recent.iter().rev() {
        let dt = chrono::DateTime::from_timestamp(a.start_time, 0)
            .unwrap_or_default()
            .with_timezone(&local_tz);
        
        // Include OCR screen text if available
        let screen_info = if let Some(ref meta) = a.metadata {
            if let Some(ref text) = meta.screen_text {
                if !text.trim().is_empty() {
                    // Show first ~150 chars of screen text (char-boundary safe)
                    let preview = if text.len() > 150 {
                        let end = text.char_indices().nth(150).map(|(i, _)| i).unwrap_or(text.len());
                        format!(" | Screen: {}...", &text[..end])
                    } else {
                        format!(" | Screen: {}", text)
                    };
                    preview
                } else { String::new() }
            } else { String::new() }
        } else { String::new() };
        
        data.push_str(&format!(
            "  [{}] {} — {} ({}){}\n",
            dt.format("%I:%M %p"),
            a.app_name,
            a.window_title,
            format_duration(a.duration_seconds),
            screen_info,
        ));
    }
    if filtered.len() > 50 {
        data.push_str(&format!("  ... and {} more activities\n", filtered.len() - 50));
    }
    
    // ── OCR screen text context (unique texts, most recent first) ──
    let mut seen_texts: Vec<String> = Vec::new();
    let mut ocr_entries: Vec<(String, String)> = Vec::new(); // (time, text)
    for a in filtered.iter().take(50) {
        if let Some(ref meta) = a.metadata {
            if let Some(ref text) = meta.screen_text {
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() && !seen_texts.iter().any(|s| s == &trimmed) {
                    let dt = chrono::DateTime::from_timestamp(a.start_time, 0)
                        .unwrap_or_default()
                        .with_timezone(&local_tz);
                    let time_str = dt.format("%I:%M %p").to_string();
                    // Truncate to ~500 chars per entry (char-boundary safe)
                    let entry_text = if trimmed.len() > 500 {
                        let end = trimmed.char_indices().nth(500).map(|(i, _)| i).unwrap_or(trimmed.len());
                        format!("{}...", &trimmed[..end])
                    } else {
                        trimmed.clone()
                    };
                    ocr_entries.push((time_str, entry_text));
                    seen_texts.push(trimmed);
                }
            }
        }
    }
    if !ocr_entries.is_empty() {
        data.push_str("\n=== SCREEN TEXT (OCR) ===\n");
        data.push_str("(Text extracted from screen captures — includes visible contact names, messages, code, etc.)\n");
        for (time, text) in ocr_entries.iter().take(10) {
            data.push_str(&format!("  [{}] {}\n", time, text));
        }
    }
    
    // ── Unique Titles (for music/videos that might be pushed out of the 50-entry timeline) ──
    let mut title_stats: HashMap<String, (i32, i32)> = HashMap::new(); // (duration, count)
    for a in filtered {
        let entry = title_stats.entry(a.window_title.clone()).or_insert((0, 0));
        entry.0 += a.duration_seconds;
        entry.1 += 1;
    }
    let mut title_list: Vec<_> = title_stats.into_iter().collect();
    title_list.sort_by(|a, b| b.1.0.cmp(&a.1.0)); // sort by duration

    if !title_list.is_empty() {
        data.push_str("\n=== UNIQUE TITLES & CONTEXT ===\n");
        for (title, (dur, count)) in title_list.iter().take(40) { // Top 40 titles
            if !title.is_empty() {
                data.push_str(&format!("  • \"{}\" ({}s, {}x)\n", title, dur, count));
            }
        }
    }

    // ── Media Playback (SMTC - Spotify/YouTube/etc) ──
    // Group by "Artist - Title"
    let mut media_stats: HashMap<String, (i32, i32)> = HashMap::new();
    for a in filtered {
        if let Some(meta) = &a.metadata {
            if let Some(media) = &meta.media_info {
                if media.status == "Playing" {
                    let key = format!("{} - {}", media.artist, media.title);
                    let entry = media_stats.entry(key).or_insert((0, 0));
                    entry.0 += a.duration_seconds;
                    entry.1 += 1;
                }
            }
        }
    }
    
    let mut media_list: Vec<_> = media_stats.into_iter().collect();
    media_list.sort_by(|a, b| b.1.0.cmp(&a.1.0));

    if !media_list.is_empty() {
        data.push_str("\n=== MEDIA PLAYBACK (Background) ===\n");
        for (track, (dur, count)) in media_list {
             data.push_str(&format!("  ♫ \"{}\" ({}s, {}x)\n", track, dur, count));
        }
    }

    // ── Background Window Context (what was open in background) ──
    // Collect all unique background window titles seen in this period
    let mut bg_windows: Vec<String> = filtered.iter()
        .flat_map(|a| a.metadata.as_ref())
        .flat_map(|m| m.background_windows.as_ref())
        .flat_map(|w| w.iter())
        .cloned()
        .collect();
    bg_windows.sort();
    bg_windows.dedup();
    
    if !bg_windows.is_empty() {
        data.push_str("\n=== BACKGROUND APPS ===\n");
        data.push_str("(Apps visible in background while user was doing other things)\n");
        // Filter out common noise
        let noise = ["Program Manager", "Settings", "Calculate", "Windows Input Experience"];
        for title in bg_windows.iter().take(30) {
            if !title.trim().is_empty() && !noise.contains(&title.as_str()) {
                data.push_str(&format!("  • {}\n", title));
            }
        }
    }

    data
}

fn category_name(id: i32) -> &'static str {
    match id {
        1 => "Development",
        2 => "Browser",
        3 => "Communication",
        4 => "Entertainment",
        5 => "Productivity",
        6 => "System",
        _ => "Other",
    }
}

// ─── Main query command ───

#[tauri::command]
pub async fn execute_query(
    app_handle: AppHandle,
    query: String,
) -> Result<QueryResult, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    // Parse the query and determine time range (local — simple date math)
    let (start_time, end_time, time_label) = parse_query_time_range(&query);
    
    // Determine if this is a broad query (multi-day) or a single-day query
    let time_span_hours = (end_time - start_time) / 3600;
    let is_broad_query = time_span_hours > 24;
    
    // Check for semantic hints to do efficient DB-level filtering
    let semantic_hints = extract_search_hints(&query);
    
    // Two-tier strategy:
    // - Single day + no specific filter → send ALL to AI (full context, ~500 max)
    // - Multi-day OR specific semantic filter → DB-level keyword search (efficient)
    let all_activities = if is_broad_query && !semantic_hints.keywords.is_empty() {
        // Broad query with semantic hints — use efficient DB search
        crate::database::queries::search_activities(
            &conn, start_time, end_time,
            &semantic_hints.keywords.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            &semantic_hints.category_ids,
            Some(500),
        ).map_err(|e| e.to_string())?
    } else if is_broad_query {
        // Broad query without hints — get recent + aggregate stats
        crate::database::queries::get_activities(&conn, start_time, end_time, Some(200))
            .map_err(|e| e.to_string())?
    } else {
        // Single day — send everything for full AI analysis
        crate::database::queries::get_activities(&conn, start_time, end_time, Some(500))
            .map_err(|e| e.to_string())?
    };
    
    let total_duration: i32 = all_activities.iter().map(|a| a.duration_seconds).sum();
    
    // Build structured data with activities for the AI to reason over
    let structured_data = build_structured_data(&all_activities, total_duration, &time_label);
    
    // For the display timeline, filter by app name or semantic hints
    let app_filter = extract_app_filter(&query);
    let display_activities = if let Some(ref app_name) = app_filter {
        let search_terms = expand_app_aliases(app_name);
        all_activities.iter()
            .filter(|a| {
                let app_lower = a.app_name.to_lowercase();
                let title_lower = a.window_title.to_lowercase();
                search_terms.iter().any(|term| app_lower.contains(term) || title_lower.contains(term))
            })
            .collect::<Vec<_>>()
    } else if !semantic_hints.keywords.is_empty() {
        // Use semantic hints for category-specific queries (songs, coding, etc.)
        all_activities.iter()
            .filter(|a| {
                // Match by category ID
                if semantic_hints.category_ids.contains(&a.category_id) {
                    return true;
                }
                // Match by title/app keywords
                let app_lower = a.app_name.to_lowercase();
                let title_lower = a.window_title.to_lowercase();
                semantic_hints.keywords.iter().any(|kw| {
                    app_lower.contains(kw.as_str()) || title_lower.contains(kw.as_str())
                })
            })
            .collect::<Vec<_>>()
    } else {
        all_activities.iter().collect::<Vec<_>>()
    };
    
    // Convert to query items (use local time for display)
    let local_tz = chrono::Local::now().timezone();
    let results: Vec<QueryItem> = display_activities.iter().map(|a| {
        let dt = chrono::DateTime::from_timestamp(a.start_time, 0)
            .unwrap_or_default()
            .with_timezone(&local_tz);
        let time_str = dt.format("%I:%M %p").to_string();
        let duration = format_duration(a.duration_seconds);
        
        QueryItem {
            timestamp: a.start_time,
            time_str,
            activity: format!("{} - {}", a.app_name, a.window_title),
            duration,
            details: a.metadata.as_ref().and_then(|m| {
                // Show media info if playing, or OCR snippet as fallback
                if let Some(ref media) = m.media_info {
                    if media.status == "Playing" {
                        return Some(format!("♫ {} - {}", media.title, media.artist));
                    }
                }
                m.screen_text.as_ref().and_then(|t| {
                    let trimmed = t.trim();
                    if trimmed.is_empty() {
                        None
                    } else if trimmed.len() > 80 {
                        let end = trimmed.char_indices().nth(80).map(|(i, _)| i).unwrap_or(trimmed.len());
                        Some(format!("{}...", &trimmed[..end]))
                    } else {
                        Some(trimmed.to_string())
                    }
                })
            }),
        }
    }).collect();
    
    // AI gets ALL data — it decides what's relevant based on the query
    let settings = load_settings(&app_handle).unwrap_or_default();
    let category_filter = extract_category_filter(&query);
    
    // Default to Agentic Search if AI is enabled
    let adjusted_summary = if settings.ai.enabled && !settings.ai.api_key.is_empty() {
        // Use the new Agentic Engine
        match crate::services::query_engine::run_agentic_search(&app_handle, &query, &settings).await {
             Ok(answer) => answer,
             Err(e) => {
                 eprintln!("Agentic search failed: {}", e);
                 // Fallback to old linear summary if agent fails
                  match ai_summarize_query(&query, &structured_data, &settings.ai.api_key, &settings.ai.model).await {
                        Ok(linear) => format!("[Agent failed, used linear fallback] {}", linear),
                        Err(_e2) => format!("[AI Error: {}] {}", e, build_fallback_summary(&time_label, &app_filter, &category_filter, &all_activities, total_duration))
                  }
             }
        }
    } else {
        build_fallback_summary(&time_label, &app_filter, &category_filter, &all_activities, total_duration)
    };
    
    let result = QueryResult {
        query: query.clone(),
        results,
        summary: adjusted_summary,
        timestamp: Utc::now().timestamp(),
    };
    
    // Cache the query result
    let _ = cache_query(&conn, &result);
    
    Ok(result)
}

// ─── Fallback summary (no AI) ───

fn build_fallback_summary(
    summary: &str,
    app_filter: &Option<String>,
    category_filter: &Option<i32>,
    filtered: &[crate::models::Activity],
    total_duration: i32,
) -> String {
    let cat_name = category_filter.map(|c| category_name(c));
    
    if let Some(ref app_name) = app_filter {
        if filtered.is_empty() {
            format!("No activity found for '{}' in the selected time range.", app_name)
        } else {
            format!("{} Found {} sessions of '{}' totaling {}.",
                summary, filtered.len(), app_name, format_duration(total_duration))
        }
    } else if let Some(cat) = cat_name {
        if filtered.is_empty() {
            format!("No {} activities found in the selected time range.", cat)
        } else {
            format!("{} Found {} {} sessions totaling {}.",
                summary, filtered.len(), cat, format_duration(total_duration))
        }
    } else {
        if filtered.is_empty() {
            "No activities found for the selected time range.".to_string()
        } else {
            format!("{} Found {} activities totaling {}.",
                summary, filtered.len(), format_duration(total_duration))
        }
    }
}

// ─── Query history & cache ───

#[tauri::command]
pub async fn get_query_history(
    app_handle: AppHandle,
    limit: Option<i32>,
) -> Result<Vec<QueryResult>, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    let sql_limit = limit.unwrap_or(20);
    
    let mut stmt = conn.prepare(
        "SELECT query_text, result, created_at FROM query_cache ORDER BY created_at DESC LIMIT ?1"
    ).map_err(|e| e.to_string())?;
    
    let history = stmt.query_map([sql_limit], |row| {
        let query_text: String = row.get(0)?;
        let result_blob: Option<Vec<u8>> = row.get(1)?;
        let created_at: i64 = row.get(2)?;
        
        let result: QueryResult = result_blob
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(QueryResult {
                query: query_text.clone(),
                results: vec![],
                summary: "Cached query".to_string(),
                timestamp: created_at,
            });
        
        Ok(result)
    }).map_err(|e| e.to_string())?
    .collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    
    Ok(history)
}

fn cache_query(conn: &rusqlite::Connection, result: &QueryResult) -> Result<(), String> {
    let result_blob = serde_json::to_vec(result).map_err(|e| e.to_string())?;
    
    conn.execute(
        "INSERT OR REPLACE INTO query_cache (query_hash, query_text, result, created_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            &crate::utils::hash_string(&result.query),
            &result.query,
            &result_blob,
            result.timestamp,
            result.timestamp + 3600,
        ],
    ).map_err(|e| e.to_string())?;
    
    Ok(())
}

// ─── App filter extraction ───

fn extract_app_filter(query: &str) -> Option<String> {
    let query_lower = query.to_lowercase();
    
    // Common app names to detect
    let known_apps = [
        "vs code", "vscode", "chrome", "firefox", "edge", "brave",
        "discord", "slack", "spotify", "teams", "outlook", "word",
        "excel", "powerpoint", "notepad", "terminal", "explorer",
        "figma", "photoshop", "illustrator", "blender", "notion",
        "obsidian", "whatsapp", "telegram", "zoom", "instagram",
        "twitter", "reddit", "github", "youtube", "netflix",
        "intentflow", "antigravity", "cursor",
    ];
    
    for app in known_apps {
        if query_lower.contains(app) {
            return Some(app.to_string());
        }
    }
    
    // Check for "use/used <app>" pattern
    let patterns = ["use ", "used ", "using ", "open ", "opened ", "on "];
    let time_words = [
        "today", "yesterday", "this", "last", "morning", "afternoon",
        "evening", "night", "week", "month", "hour", "ago", "recently",
        "earlier", "before", "so far", "now", "just", "my", "the",
        "much", "long", "time", "did", "was", "were", "been", "have",
        "has", "how", "what", "when", "where", "which", "that", "it",
        "monday", "tuesday", "wednesday", "thursday", "friday",
        "saturday", "sunday", "computer", "screen", "phone",
    ];
    for pattern in patterns {
        if let Some(pos) = query_lower.find(pattern) {
            let after = &query_lower[pos + pattern.len()..];
            let app_name = after.split(|c: char| c == '?' || c == '.' || c == ',' || c == '!' || c == ';')
                .next()
                .unwrap_or("")
                .trim();
            if !app_name.is_empty() && app_name.len() > 1 
                && !time_words.iter().any(|tw| app_name == *tw || app_name.starts_with(tw))
            {
                return Some(app_name.to_string());
            }
        }
    }
    
    None
}

// ─── Category title keywords (for mis-categorized old entries) ───

#[allow(dead_code)]
fn get_category_title_keywords(category_id: i32) -> Vec<&'static str> {
    match category_id {
        4 => vec![ // Entertainment
            "spotify", "youtube", "netflix", "twitch", "soundcloud",
            "apple music", "liked songs", "playlist", "music", "\u{2022}",
        ],
        1 => vec![ // Development
            "visual studio", ".ts ", ".py ", ".rs ", ".js ", "github", "gitlab",
        ],
        3 => vec![ // Communication
            "slack", "discord", "teams", "zoom", "telegram", "whatsapp",
        ],
        _ => vec![],
    }
}

// ─── App alias expansion ───

fn expand_app_aliases(app: &str) -> Vec<String> {
    let app_lower = app.to_lowercase();
    
    let alias_groups: Vec<Vec<&str>> = vec![
        vec!["vs code", "vscode", "visual studio code", "code", "antigravity"],
        vec!["chrome", "google chrome"],
        vec!["edge", "microsoft edge", "msedge"],
        vec!["brave", "brave browser"],
        vec!["firefox", "mozilla firefox"],
        vec!["teams", "microsoft teams"],
        vec!["word", "microsoft word"],
        vec!["excel", "microsoft excel"],
        vec!["notepad", "notepad++"],
        vec!["explorer", "file explorer", "windows explorer"],
        vec!["spotify", "liked songs"],
        vec!["youtube", "yt"],
        vec!["whatsapp", "whatsapp.root"],
        vec!["instagram", "insta"],
    ];
    
    for group in &alias_groups {
        if group.iter().any(|alias| *alias == app_lower) {
            return group.iter().map(|s| s.to_string()).collect();
        }
    }
    
    vec![app_lower]
}

// ─── Semantic search hints for DB-level filtering ───

struct SearchHints {
    keywords: Vec<String>,
    category_ids: Vec<i32>,
}

/// Extract semantic hints from the query for efficient DB-level filtering.
/// Used for broad multi-day queries where we can't send everything to AI.
fn extract_search_hints(query: &str) -> SearchHints {
    let q = query.to_lowercase();
    let mut keywords = Vec::new();
    let mut category_ids = Vec::new();
    
    // Music/songs queries
    if q.contains("song") || q.contains("music") || q.contains("listen")
        || q.contains("heard") || q.contains("hearing") || q.contains("played")
        || q.contains("streaming")
    {
        keywords.extend(["spotify", "liked songs", "\u{2022}", "soundcloud", "apple music"]
            .iter().map(|s| s.to_string()));
        category_ids.push(4);
    }
    
    // Video/entertainment queries
    if q.contains("watch") || q.contains("video") || q.contains("movie")
        || q.contains("entertainment") || q.contains("netflix") || q.contains("gaming")
    {
        keywords.extend(["youtube", "netflix", "twitch", "vlc", "media player"]
            .iter().map(|s| s.to_string()));
        category_ids.push(4);
    }
    
    // Coding queries
    if q.contains("cod") || q.contains("program") || q.contains("develop")
        || q.contains("debug")
    {
        keywords.extend(["visual studio", "code", "antigravity", "cursor", "terminal", "github"]
            .iter().map(|s| s.to_string()));
        category_ids.push(1);
    }
    
    // Communication queries
    if q.contains("chat") || q.contains("messag") || q.contains("communicat")
        || q.contains("email") || q.contains("meeting") || q.contains("call")
    {
        keywords.extend(["discord", "slack", "teams", "whatsapp", "telegram", "zoom"]
            .iter().map(|s| s.to_string()));
        category_ids.push(3);
    }
    
    // Browsing queries
    if q.contains("brows") || q.contains("surf") || q.contains("website")
        || q.contains("internet")
    {
        keywords.extend(["chrome", "brave", "firefox", "edge"]
            .iter().map(|s| s.to_string()));
        category_ids.push(2);
    }
    
    // Specific app mentions — add as keywords too
    if let Some(app) = extract_app_filter(query) {
        let aliases = expand_app_aliases(&app);
        keywords.extend(aliases);
    }
    
    // Deduplicate
    keywords.sort();
    keywords.dedup();
    category_ids.sort();
    category_ids.dedup();
    
    SearchHints { keywords, category_ids }
}

// ─── Category filter extraction ───

fn extract_category_filter(query: &str) -> Option<i32> {
    let q = query.to_lowercase();
    
    // Development (category 1)
    if q.contains("coding") || q.contains("programming") || q.contains("developing") ||
       q.contains("development") || q.contains("dev work") || q.contains("coded") {
        return Some(1);
    }
    
    // Browser (category 2)
    if q.contains("browsing") || q.contains("surfing") || q.contains("websites") ||
       q.contains("web pages") || q.contains("internet") {
        return Some(2);
    }
    
    // Communication (category 3)
    if q.contains("chatting") || q.contains("messaging") || q.contains("communicat") ||
       q.contains("emails") || q.contains("meetings") || q.contains("calls") {
        return Some(3);
    }
    
    // Entertainment (category 4)
    if q.contains("entertainment") || q.contains("watching") || q.contains("listening") ||
       q.contains("gaming") || q.contains("music") || q.contains("movies") ||
       q.contains("songs") || q.contains("hearing") || q.contains("heard") ||
       q.contains("listened") || q.contains("played") || q.contains("streaming") ||
       q.contains("video") || q.contains("musics") {
        return Some(4);
    }
    
    // Productivity (category 5)
    if q.contains("productive") || q.contains("productivity") || q.contains("notes") || 
       q.contains("prodcutiv") ||  // common typo
       q.contains("writing") || q.contains("documents") || q.contains("spreadsheet") {
        // "productivity" queries should NOT filter by category — they want overall analysis
        // Only filter if specifically asking about notes/documents
        if q.contains("notes") || q.contains("writing") || q.contains("documents") || q.contains("spreadsheet") {
            return Some(5);
        }
        // For "how was my productivity" — return None to show ALL activities
        return None;
    }
    
    None
}

// ─── Time range parsing ───

fn parse_query_time_range(query: &str) -> (i64, i64, String) {
    let now = chrono::Local::now();
    let query_lower = query.to_lowercase();
    let tz = now.timezone();
    
    // "yesterday" (with typo handling)
    if query_lower.contains("yesterday") || query_lower.contains("yesteray")
        || query_lower.contains("yeterday") || query_lower.contains("yestarday")
        || query_lower.contains("yesterda") || query_lower.contains("ysterday")
        || query_lower.contains("yesteday") || query_lower.contains("yesterdy")
    {
        let yesterday = now - chrono::Duration::days(1);
        let start = yesterday.date_naive().and_hms_opt(0, 0, 0).unwrap();
        let end = yesterday.date_naive().and_hms_opt(23, 59, 59).unwrap();
        return (
            start.and_local_timezone(tz).unwrap().timestamp(),
            end.and_local_timezone(tz).unwrap().timestamp(),
            "Yesterday's activity:".to_string(),
        );
    }
    
    // "last week" / "past week"
    if query_lower.contains("last week") || query_lower.contains("past week") {
        let start_date = now - chrono::Duration::days(7);
        let start = start_date.date_naive().and_hms_opt(0, 0, 0).unwrap();
        return (
            start.and_local_timezone(tz).unwrap().timestamp(),
            now.timestamp(),
            "Last 7 days activity:".to_string(),
        );
    }
    
    // "this week"
    if query_lower.contains("this week") {
        let weekday = now.date_naive().weekday().num_days_from_monday() as i64;
        let monday = now - chrono::Duration::days(weekday);
        let start = monday.date_naive().and_hms_opt(0, 0, 0).unwrap();
        return (
            start.and_local_timezone(tz).unwrap().timestamp(),
            now.timestamp(),
            "This week's activity:".to_string(),
        );
    }
    
    // "last month" / "past month"
    if query_lower.contains("last month") || query_lower.contains("past month") {
        let start_date = now - chrono::Duration::days(30);
        let start = start_date.date_naive().and_hms_opt(0, 0, 0).unwrap();
        return (
            start.and_local_timezone(tz).unwrap().timestamp(),
            now.timestamp(),
            "Last 30 days activity:".to_string(),
        );
    }
    
    // "last N hours" / "past N hours"
    if let Some(hours) = extract_n_hours(&query_lower) {
        let start = now - chrono::Duration::hours(hours);
        return (
            start.timestamp(),
            now.timestamp(),
            format!("Last {} hour{} activity:", hours, if hours == 1 { "" } else { "s" }),
        );
    }
    
    // "last hour"
    if query_lower.contains("last hour") {
        let start = now - chrono::Duration::hours(1);
        return (
            start.timestamp(),
            now.timestamp(),
            "Last hour's activity:".to_string(),
        );
    }
    
    // "N days ago"
    if let Some(days) = extract_days_ago(&query_lower) {
        let target = now - chrono::Duration::days(days);
        let start = target.date_naive().and_hms_opt(0, 0, 0).unwrap();
        let end = target.date_naive().and_hms_opt(23, 59, 59).unwrap();
        return (
            start.and_local_timezone(tz).unwrap().timestamp(),
            end.and_local_timezone(tz).unwrap().timestamp(),
            format!("{} days ago:", days),
        );
    }
    
    // Day names: "monday", "tuesday", etc. (finds the most recent one)
    if let Some((day_start, day_end, label)) = parse_day_name(&query_lower, &now) {
        return (day_start, day_end, label);
    }
    
    // "today" or "so far" or any unrecognized query (default to today)
    if query_lower.contains("today") || query_lower.contains("so far") {
        let start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
        return (
            start.and_local_timezone(tz).unwrap().timestamp(),
            now.timestamp(),
            "Today's activity:".to_string(),
        );
    }
    
    // Time-of-day queries
    if query_lower.contains("morning") {
        let start = now.date_naive().and_hms_opt(6, 0, 0).unwrap();
        let end = now.date_naive().and_hms_opt(12, 0, 0).unwrap();
        return (
            start.and_local_timezone(tz).unwrap().timestamp(),
            end.and_local_timezone(tz).unwrap().timestamp(),
            "This morning's activity:".to_string(),
        );
    }
    
    if query_lower.contains("afternoon") {
        let start = now.date_naive().and_hms_opt(12, 0, 0).unwrap();
        let end = now.date_naive().and_hms_opt(18, 0, 0).unwrap();
        return (
            start.and_local_timezone(tz).unwrap().timestamp(),
            end.and_local_timezone(tz).unwrap().timestamp(),
            "This afternoon's activity:".to_string(),
        );
    }
    
    if query_lower.contains("evening") || query_lower.contains("tonight") || query_lower.contains("night") {
        let start = now.date_naive().and_hms_opt(18, 0, 0).unwrap();
        let end = now.date_naive().and_hms_opt(23, 59, 59).unwrap();
        return (
            start.and_local_timezone(tz).unwrap().timestamp(),
            end.and_local_timezone(tz).unwrap().timestamp(),
            "This evening's activity:".to_string(),
        );
    }
    
    // Default: today (handles gibberish, vague, or any unmatched query)
    let start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
    (
        start.and_local_timezone(tz).unwrap().timestamp(),
        now.timestamp(),
        "Today's activity:".to_string(),
    )
}

// ─── Helper parsers ───

fn extract_days_ago(query: &str) -> Option<i64> {
    let re = regex::Regex::new(r"(\d+)\s+days?\s+ago").ok()?;
    re.captures(query)
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse::<i64>().ok())
}

fn extract_n_hours(query: &str) -> Option<i64> {
    let re = regex::Regex::new(r"(?:last|past)\s+(\d+)\s+hours?").ok()?;
    re.captures(query)
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse::<i64>().ok())
}

fn parse_day_name(query: &str, now: &chrono::DateTime<chrono::Local>) -> Option<(i64, i64, String)> {
    let days = [
        ("monday", chrono::Weekday::Mon),
        ("tuesday", chrono::Weekday::Tue),
        ("wednesday", chrono::Weekday::Wed),
        ("thursday", chrono::Weekday::Thu),
        ("friday", chrono::Weekday::Fri),
        ("saturday", chrono::Weekday::Sat),
        ("sunday", chrono::Weekday::Sun),
    ];
    
    for (name, weekday) in days {
        if query.contains(name) {
            let current_weekday = now.weekday();
            let mut days_back = (current_weekday.num_days_from_monday() as i64) 
                - (weekday.num_days_from_monday() as i64);
            if days_back <= 0 {
                days_back += 7; // go to last week's occurrence
            }
            
            let target = *now - chrono::Duration::days(days_back);
            let tz = now.timezone();
            let start = target.date_naive().and_hms_opt(0, 0, 0).unwrap();
            let end = target.date_naive().and_hms_opt(23, 59, 59).unwrap();
            
            let label = format!("Last {}'s activity:", 
                name.chars().next().unwrap().to_uppercase().collect::<String>() + &name[1..]);
            
            return Some((
                start.and_local_timezone(tz).unwrap().timestamp(),
                end.and_local_timezone(tz).unwrap().timestamp(),
                label,
            ));
        }
    }
    
    None
}

// ─── Duration formatting ───

fn format_duration(seconds: i32) -> String {
    if seconds < 60 {
        return format!("{}s", seconds);
    }
    
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    
    if hours > 0 {
        if minutes > 0 {
            format!("{}h {}m", hours, minutes)
        } else {
            format!("{}h", hours)
        }
    } else if secs > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}m", minutes)
    }
}
