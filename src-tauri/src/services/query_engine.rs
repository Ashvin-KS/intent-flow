use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::models::{Settings, ActivityMetadata};
use tauri::{Manager, Emitter};
use chrono::{Duration, Local, TimeZone};
use std::time::Duration as StdDuration;

// ─── Constants ───

const MAX_TURNS: usize = 20;
const MAX_TOOL_RETRY_LOOPS: usize = 3;
const LLM_TIMEOUT_SECS: u64 = 60;

// ─── Types ───

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
    stream: bool, // Enable streaming
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatRecvMessage,
}

#[derive(Deserialize)]
struct ChatRecvMessage {
    content: Option<String>,
    #[allow(dead_code)]
    reasoning_content: Option<String>,
}

// For streaming
#[derive(Deserialize)]
struct ChatStreamResponse {
    choices: Vec<ChatStreamChoice>,
}

#[derive(Deserialize)]
struct ChatStreamChoice {
    delta: ChatStreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChatStreamDelta {
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
}

// ─── Agent Logic ───

// We define the agent tools and instructions here
const AGENT_SYSTEM_PROMPT: &str = r#"You are IntentFlow's AI activity analyst — a smart, conversational assistant embedded inside the desktop app.
You have access to the user's activity history (apps, windows, duration, time) and OCR screen text.

## Your Tools
1. `get_music_history` - For finding songs/music
   - Args: hours (default 24), limit (default 50)
   - Returns formatted list of songs with title, artist, app, and time

2. `get_recent_activities` - For events/tasks/recent activity timeline
   - Args: hours (default 24), limit (default 100), category_id (optional)
   - Returns chronological activity events with app, title, category, duration, and time

3. `query_activities` - SQL queries on the `activities` table
   - Fields: app_name, window_title, start_time (unix timestamp), duration_seconds, category_id, metadata
   - metadata.media_info contains {title, artist, status} for music

4. `get_usage_stats` - Aggregated stats by app
   - Args: start_time_iso, end_time_iso

5. `search_ocr` - Search screen text content
   - Args: keyword, limit (default 100)

6. `get_recent_ocr` - Browse recent OCR captures (including chats) without exact keyword
   - Args: hours (default 24), limit (default 100), app (optional), keyword (optional)
   - Returns recent OCR snippets with app and timestamp

7. `get_recent_file_changes` - Recent code/document file changes from monitored project roots
   - Args: hours (default 24), limit (default 40), change_type (optional: created|modified|deleted)
   - Returns recent file change events with project root and timestamp

8. `parallel_search` - Run multiple tool calls in parallel for broader coverage
   - Args: calls = [{tool: "...", args: {...}}, ...]
   - Use for complex queries that need combining activity + OCR + music evidence quickly

## Category IDs
- 1 = Development | 2 = Browser | 3 = Communication | 4 = Entertainment | 5 = Productivity | 6 = System | 7 = Other

## CRITICAL RULES
1. For music/song queries → Use get_music_history tool
2. For "what did I do", "events", "timeline", "recent activity" queries → Use get_recent_activities first
3. For time spent / top apps / summary queries → Use get_usage_stats or query_activities with SUM
4. For "what did I text", "WhatsApp chat", "what did I chat" queries → Use get_recent_ocr with app="whatsapp" first, then search_ocr if needed
5. For "show OCR data" queries → Use get_recent_ocr without keyword
6. NEVER give up after one query if results are empty - try different approaches
7. If a tool returns empty results, try a broader query or different keywords
8. For coding progress or project-change questions, use get_recent_file_changes.
9. For broad/ambiguous requests, prefer parallel_search with 2-3 tool calls
10. Use conversation history to resolve references like "it", "that", "the previous one", "what was it about".
11. Never claim facts without tool evidence from the requested time scope.
12. If evidence is weak or contradictory, ask a clarifying date/day question instead of guessing.
13. For "what am I hearing right now", rely only on very recent records marked as Playing.
14. For underspecified queries (missing source/app or time intent), ask a short clarifying question before searching.
15. If you are asked about people, names, or girls, use `search_ocr` with a high limit and try different keywords or no keywords at all to get all the data.
16. If you are asked about chats, use `get_recent_ocr` with a high limit and try different apps like "whatsapp", "instagram", "telegram", etc.

## Response Format
Output JSON for tool calls: { "tool": "tool_name", "args": { ... }, "reasoning": "..." }
Output detailed, crisp, and highly specific final answers. Use markdown (like bolding and bullet points) to make the answer easy to read.

Do NOT output markdown code blocks for tool calls. Output RAW JSON only.

## Thinking Quality Rules
- If reasoning content is emitted, keep it user-facing and concise (max 1 short sentence).
- Never include internal planning language such as "the user is asking", "I should", "let me", "likely", or tool-selection analysis.
- Never echo raw tool-call JSON inside thinking text.
- Bad example: "The user says now? Likely they want..."
- Good example: "Checking your recent music activity now."
"#;

#[derive(Deserialize, Serialize, Debug)]
#[serde(untagged)]
enum AgentResponse {
    ToolCall {
        tool: String,
        args: Value,
        #[allow(dead_code)]
        reasoning: Option<String>,
    },
    // If it's not a tool call, we treat it as a final answer string
    FinalAnswer(String),
}

#[derive(Clone, Debug)]
struct TimeScope {
    id: String,
    label: String,
    start_ts: i64,
    end_ts: i64,
}

#[derive(Clone, Debug, Default)]
struct QueryIntent {
    wants_music: bool,
    wants_ocr: bool,
    wants_files: bool,
    wants_timeline: bool,
    broad_summary: bool,
}

// ─── Public API ───

pub async fn run_agentic_search(
    app_handle: &tauri::AppHandle,
    user_query: &str,
    settings: &Settings,
) -> Result<String, String> {
    // Delegate to the step-tracking version, just return the answer
    let result = run_agentic_search_with_steps_and_scope(app_handle, user_query, settings, None).await?;
    Ok(result.answer)
}

// ─── Structured Agent Result (for Chat UI) ───

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentStep {
    pub turn: usize,
    pub tool_name: String,
    pub tool_args: Value,
    pub tool_result: String,
    pub reasoning: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentResult {
    pub answer: String,
    pub steps: Vec<AgentStep>,
    pub activities_referenced: Vec<Value>,
}

pub async fn run_agentic_search_with_steps(
    app_handle: &tauri::AppHandle,
    user_query: &str,
    settings: &Settings,
) -> Result<AgentResult, String> {
    run_agentic_search_with_steps_and_scope(app_handle, user_query, settings, None).await
}

pub async fn run_agentic_search_with_steps_and_scope(
    app_handle: &tauri::AppHandle,
    user_query: &str,
    settings: &Settings,
    time_scope: Option<&str>,
) -> Result<AgentResult, String> {
    run_agentic_search_with_steps_and_history_and_scope(
        app_handle,
        user_query,
        settings,
        &[],
        time_scope,
    ).await
}

pub async fn run_agentic_search_with_steps_and_history(
    app_handle: &tauri::AppHandle,
    user_query: &str,
    settings: &Settings,
    prior_messages: &[ChatMessage],
) -> Result<AgentResult, String> {
    run_agentic_search_with_steps_and_history_and_scope(
        app_handle,
        user_query,
        settings,
        prior_messages,
        None,
    ).await
}

pub async fn run_agentic_search_with_steps_and_history_and_scope(
    app_handle: &tauri::AppHandle,
    user_query: &str,
    settings: &Settings,
    prior_messages: &[ChatMessage],
    time_scope: Option<&str>,
) -> Result<AgentResult, String> {
    let api_key = crate::utils::config::resolve_api_key(&settings.ai.api_key);
    let model = &settings.ai.model;
    
    if api_key.is_empty() {
        return Err("AI is disabled or API key is missing".to_string());
    }

    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let mut steps: Vec<AgentStep> = Vec::new();
    let mut all_activities: Vec<Value> = Vec::new();
    let resolved_scope = resolve_time_scope(time_scope, user_query);
    let intent = detect_query_intent(user_query);
    
    // Initial messages
    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: AGENT_SYSTEM_PROMPT.to_string(),
    }];

    // Include recent chat history so follow-up questions keep context.
    for msg in prior_messages.iter().rev().take(12).rev() {
        if msg.content.trim().is_empty() {
            continue;
        }
        let role = if msg.role.eq_ignore_ascii_case("assistant") {
            "assistant"
        } else {
            "user"
        };
        messages.push(ChatMessage {
            role: role.to_string(),
            content: truncate_for_token_limit(&msg.content, 1200),
        });
    }

    messages.push(ChatMessage {
        role: "user".to_string(),
        content: format!(
            "User query: \"{}\"\nCurrent Time: {}\nSelected Time Scope: {} ({} to {})\nAlways keep retrieval strictly inside this scope unless the user asks to change it. If you need to search for people, names, or girls, use `search_ocr` with a high limit and try different keywords or no keywords at all to get all the data. If you need to search for chats, use `get_recent_ocr` with a high limit and try different apps like \"whatsapp\", \"instagram\", \"telegram\", etc.",
            user_query,
            chrono::Local::now().to_rfc3339(),
            resolved_scope.label,
            format_time_scope_ts(resolved_scope.start_ts),
            format_time_scope_ts(resolved_scope.end_ts)
        ),
    });

    if intent.broad_summary {
        let prefetch_args = build_prefetch_parallel_args(&resolved_scope, &intent);
        if let Ok((prefetch_output, prefetch_activities)) =
            execute_parallel_search(&db_path, &prefetch_args, Some(&resolved_scope), user_query)
        {
            if !prefetch_activities.is_empty() {
                all_activities.extend(prefetch_activities);
            }
            steps.push(AgentStep {
                turn: 0,
                tool_name: "parallel_search".to_string(),
                tool_args: prefetch_args,
                tool_result: truncate_for_token_limit(&prefetch_output, 4000),
                reasoning: "Prefetch evidence for broad multi-source summary".to_string(),
            });
            messages.push(ChatMessage {
                role: "user".to_string(),
                content: format!(
                    "Prefetched evidence before tool-planning:\n{}",
                    truncate_for_token_limit(&prefetch_output, 3500)
                ),
            });
        }
    }

    for turn in 0..MAX_TURNS {
        let _ = app_handle.emit("chat://status", format!("Thinking (step {}/{})", turn + 1, MAX_TURNS));
        // 1. Call LLM with streaming callback
        // We accumulate the full content here, while also streaming it to the frontend
        let mut full_response = String::new();
        let mut decision_made = false;
        let mut suppress_stream = false;
        let mut sniff = String::new();
        // Callback to handle streaming chunks
        let on_token = |chunk: &str| {
            sniff.push_str(chunk);
            if !decision_made && sniff.trim_start().len() >= 6 {
                decision_made = true;
            }
            if sniff.contains("\"tool\"") && sniff.contains("\"args\"") {
                suppress_stream = true;
            }

            if !suppress_stream {
                let _ = app_handle.emit("chat://token", chunk);
            }
        };

        call_llm_stream(model, &api_key, &messages, &mut full_response, on_token).await?;

        // 2. Parse Response
        let parsed_response = try_parse_tool_call_response(&full_response)
            .unwrap_or_else(|| AgentResponse::FinalAnswer(full_response.clone()));

        // 3. Handle Action
        match parsed_response {
            AgentResponse::FinalAnswer(answer) => {
                // Done!
                let _ = app_handle.emit("chat://done", "final_answer");
                let normalized = normalize_final_answer(&answer);
                return Ok(AgentResult {
                    answer: normalized,
                    steps,
                    activities_referenced: all_activities,
                });
            }
            AgentResponse::ToolCall { tool, args, reasoning } => {
                let enforced_args = enforce_tool_args_with_scope(&tool, &args, &resolved_scope, user_query);
                println!("[Agent] Turn {}: Calling {} ({:?})", turn + 1, tool, enforced_args);
                let _ = app_handle.emit("chat://status", format!("Running {}", tool));
                // Notify frontend of agent step (tool call) start?
                // For now, frontend just sees tokens.
                
                // Add assistant message to history
                messages.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: full_response.clone(),
                });

                // Execute tool with bounded retry loops and optional parallelization
                let (tool_output, tool_activities, attempts_used) = if tool == "parallel_search" {
                    let parallel_count = enforced_args
                        .get("calls")
                        .and_then(|v| v.as_array())
                        .map(|v| v.len())
                        .unwrap_or(0);
                    let _ = app_handle.emit(
                        "chat://token",
                        format!("\n[Agent] Running {} searches in parallel...\n", parallel_count),
                    );
                    let (out, activities) = execute_parallel_search(
                        &db_path,
                        &enforced_args,
                        Some(&resolved_scope),
                        user_query,
                    )?;
                    (out, activities, 1usize)
                } else {
                    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
                    execute_tool_with_retries(&conn, &tool, &enforced_args, MAX_TOOL_RETRY_LOOPS)?
                };

                // Add activities from tool result to referenced activities
                all_activities.extend(transform_activities_for_frontend(&tool, &tool_activities));
                dedupe_activities(&mut all_activities);
                let _ = app_handle.emit(
                    "chat://status",
                    format!("{} completed ({} referenced items)", tool, tool_activities.len())
                );
                
                // Truncate output if too long to save tokens
                let with_retry_note = if attempts_used > 1 {
                    format!(
                        "Auto-retried with broader search {} time(s).\n{}",
                        attempts_used - 1,
                        tool_output
                    )
                } else {
                    tool_output
                };
                let truncated_output = truncate_for_token_limit(&with_retry_note, 10000);
                
                // Record step
                steps.push(AgentStep {
                    turn: turn + 1,
                    tool_name: tool.clone(),
                    tool_args: enforced_args.clone(),
                    tool_result: truncated_output.clone(),
                    reasoning: reasoning.as_deref().unwrap_or("").to_string(),
                });

                // Add tool output to history
                messages.push(ChatMessage {
                    role: "user".to_string(),
                    content: format!("Tool Output (JSON): {}", truncated_output),
                });
            }
        }
    }

    let _ = app_handle.emit("chat://status", "Finalizing answer from gathered evidence...");
    let answer = synthesize_answer_from_evidence(
        app_handle,
        model,
        &api_key,
        user_query,
        &resolved_scope,
        &steps,
        &all_activities,
    ).await.unwrap_or_else(|_| "I checked your activity and found partial evidence, but not enough for a fully confident answer. Ask with a specific date/app and I will give exact details.".to_string());
    Ok(AgentResult { answer, steps, activities_referenced: all_activities })
}

// ─── Tool Execution ───

fn detect_query_intent(query: &str) -> QueryIntent {
    let q = query.to_lowercase();
    let wants_music = q.contains("song")
        || q.contains("music")
        || q.contains("spotify")
        || q.contains("hearing")
        || q.contains("listen");
    let wants_ocr = q.contains("ocr")
        || q.contains("whatsapp")
        || q.contains("chat")
        || q.contains("text")
        || q.contains("instagram");
    let wants_files = q.contains("file")
        || q.contains("code")
        || q.contains("project")
        || q.contains("document")
        || q.contains("change");
    let wants_timeline = q.contains("timeline")
        || q.contains("what did i do")
        || q.contains("activity")
        || q.contains("summary")
        || q.contains("overview");
    let broad_summary = q.contains("full summary")
        || q.contains("don't leave anything")
        || q.contains("dont leave anything")
        || q.contains("everything")
        || q.contains("today")
        || q.contains("yesterday")
        || (wants_timeline && (wants_ocr || wants_files || wants_music));

    QueryIntent {
        wants_music,
        wants_ocr,
        wants_files,
        wants_timeline,
        broad_summary,
    }
}

fn query_has_time_hint(query: &str) -> bool {
    let q = query.to_lowercase();
    q.contains("today")
        || q.contains("yesterday")
        || q.contains("last ")
        || q.contains("past ")
        || q.contains("this week")
        || q.contains("this month")
        || q.contains("right now")
        || q.contains("few mins")
        || q.chars().any(|c| c.is_ascii_digit())
}

fn parse_scope_id_from_query(query: &str) -> Option<&'static str> {
    let q = query.to_lowercase();
    if q.contains("yesterday")
        || q.contains("yesteray")
        || q.contains("yeterday")
        || q.contains("yestarday")
    {
        return Some("yesterday");
    }
    if q.contains("last 3 day") || q.contains("past 3 day") {
        return Some("last_3_days");
    }
    if q.contains("last 7 day") || q.contains("past week") || q.contains("last week") {
        return Some("last_7_days");
    }
    if q.contains("last 30 day") || q.contains("past month") || q.contains("last month") {
        return Some("last_30_days");
    }
    if q.contains("all time") || q.contains("ever") || q.contains("across all days") {
        return Some("all_time");
    }
    if q.contains("today") || q.contains("so far") {
        return Some("today");
    }
    None
}

fn local_day_bounds(days_ago: i64) -> Option<(i64, i64)> {
    let now = Local::now();
    let target_date = now.date_naive() - Duration::days(days_ago);
    let start_naive = target_date.and_hms_opt(0, 0, 0)?;
    let end_naive = target_date.and_hms_opt(23, 59, 59)?;
    let tz = now.timezone();
    let start = tz.from_local_datetime(&start_naive).single()?.timestamp();
    let end = tz.from_local_datetime(&end_naive).single()?.timestamp();
    Some((start, end))
}

fn resolve_time_scope(explicit_scope: Option<&str>, query: &str) -> TimeScope {
    let now = chrono::Utc::now().timestamp();
    let scope_id = explicit_scope
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_lowercase())
        .or_else(|| parse_scope_id_from_query(query).map(|s| s.to_string()))
        .unwrap_or_else(|| "today".to_string());

    match scope_id.as_str() {
        "yesterday" => {
            let (start_ts, end_ts) = local_day_bounds(1).unwrap_or((now - 86400, now));
            TimeScope { id: scope_id, label: "Yesterday".to_string(), start_ts, end_ts }
        }
        "last_3_days" => {
            let start_ts = local_day_bounds(2).map(|(s, _)| s).unwrap_or(now - 3 * 86400);
            TimeScope { id: scope_id, label: "Last 3 Days".to_string(), start_ts, end_ts: now }
        }
        "last_7_days" => {
            let start_ts = local_day_bounds(6).map(|(s, _)| s).unwrap_or(now - 7 * 86400);
            TimeScope { id: scope_id, label: "Last 7 Days".to_string(), start_ts, end_ts: now }
        }
        "last_30_days" => {
            let start_ts = local_day_bounds(29).map(|(s, _)| s).unwrap_or(now - 30 * 86400);
            TimeScope { id: scope_id, label: "Last 30 Days".to_string(), start_ts, end_ts: now }
        }
        "all_time" => TimeScope {
            id: scope_id,
            label: "All Time".to_string(),
            start_ts: 0,
            end_ts: now,
        },
        _ => {
            let start_ts = local_day_bounds(0).map(|(s, _)| s).unwrap_or(now - 86400);
            TimeScope { id: "today".to_string(), label: "Today".to_string(), start_ts, end_ts: now }
        }
    }
}

fn format_time_scope_ts(ts: i64) -> String {
    if ts <= 0 {
        return "beginning".to_string();
    }
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.with_timezone(&Local).format("%b %d, %Y %I:%M %p").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn enforce_tool_args_with_scope(tool: &str, args: &Value, scope: &TimeScope, user_query: &str) -> Value {
    if tool == "parallel_search" {
        let mut next = args.clone();
        let root = match next.as_object_mut() {
            Some(v) => v,
            None => return args.clone(),
        };
        if let Some(calls) = root.get_mut("calls").and_then(|v| v.as_array_mut()) {
            for call in calls {
                let Some(call_obj) = call.as_object_mut() else { continue; };
                let call_tool = call_obj
                    .get("tool")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let base_args = call_obj.get("args").cloned().unwrap_or_else(|| serde_json::json!({}));
                call_obj.insert(
                    "args".to_string(),
                    enforce_tool_args_with_scope(&call_tool, &base_args, scope, user_query),
                );
            }
        }
        return next;
    }

    let mut next = args.clone();
    let Some(obj) = next.as_object_mut() else {
        return args.clone();
    };

    obj.insert("start_ts".to_string(), serde_json::json!(scope.start_ts));
    obj.insert("end_ts".to_string(), serde_json::json!(scope.end_ts));
    obj.insert("scope_label".to_string(), serde_json::json!(scope.label));

    let span_seconds = (scope.end_ts - scope.start_ts).max(0);
    let span_hours = ((span_seconds + 3599) / 3600).max(1);
    obj.insert("hours".to_string(), serde_json::json!(span_hours));

    if tool == "get_recent_activities" && !detect_query_intent(user_query).wants_music {
        obj.insert("exclude_media_noise".to_string(), Value::Bool(true));
    }

    if tool == "get_usage_stats" {
        let start_iso = chrono::DateTime::from_timestamp(scope.start_ts, 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());
        let end_iso = chrono::DateTime::from_timestamp(scope.end_ts, 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
        obj.insert("start_time_iso".to_string(), Value::String(start_iso));
        obj.insert("end_time_iso".to_string(), Value::String(end_iso));
    }

    next
}

fn resolve_window_from_args(args: &Value, default_hours: i64) -> (i64, i64) {
    let now = chrono::Utc::now().timestamp();
    let hours = args["hours"].as_u64().unwrap_or(default_hours as u64) as i64;
    let mut start_ts = args.get("start_ts").and_then(|v| v.as_i64()).unwrap_or(now - hours * 3600);
    let mut end_ts = args.get("end_ts").and_then(|v| v.as_i64()).unwrap_or(now);

    if end_ts <= 0 {
        end_ts = now;
    }
    if start_ts < 0 {
        start_ts = 0;
    }
    if start_ts > end_ts {
        std::mem::swap(&mut start_ts, &mut end_ts);
    }
    (start_ts, end_ts)
}

fn build_prefetch_parallel_args(scope: &TimeScope, intent: &QueryIntent) -> Value {
    let mut calls = vec![serde_json::json!({
        "tool": "get_recent_activities",
        "args": {
            "limit": if scope.id == "all_time" { 120 } else { 80 },
            "exclude_media_noise": !intent.wants_music
        }
    })];

    if intent.wants_ocr || intent.broad_summary {
        calls.push(serde_json::json!({
            "tool": "get_recent_ocr",
            "args": { "limit": if scope.id == "all_time" { 80 } else { 50 } }
        }));
    }

    if intent.wants_files || intent.wants_timeline || intent.broad_summary {
        calls.push(serde_json::json!({
            "tool": "get_recent_file_changes",
            "args": { "limit": if scope.id == "all_time" { 80 } else { 40 } }
        }));
    }

    if intent.wants_music {
        calls.push(serde_json::json!({
            "tool": "get_music_history",
            "args": { "limit": if scope.id == "all_time" { 80 } else { 40 } }
        }));
    }

    serde_json::json!({ "calls": calls })
}

fn dedupe_activities(activities: &mut Vec<Value>) {
    let mut seen = std::collections::HashSet::new();
    activities.retain(|item| {
        let app = item.get("app").and_then(|v| v.as_str()).unwrap_or_default();
        let title = item.get("title").and_then(|v| v.as_str()).unwrap_or_default();
        let time = item.get("time").and_then(|v| v.as_i64()).unwrap_or_default();
        let key = format!("{}|{}|{}", app, title, time);
        seen.insert(key)
    });
}

fn is_media_noise_event(event: &Value) -> bool {
    let Some(media) = event.get("media_info").and_then(|v| v.as_object()) else {
        return false;
    };
    let title = media.get("title").and_then(|v| v.as_str()).unwrap_or("").trim();
    if title.is_empty() {
        return false;
    }
    let app_name = event.get("app_name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
    // Keep direct player rows, filter incidental "now playing" reflections from other windows.
    !(app_name.contains("spotify") || app_name.contains("youtube") || app_name.contains("music"))
}

fn is_low_signal_result(tool: &str, output: &str, activities: &[Value]) -> bool {
    if !activities.is_empty() {
        return false;
    }
    let text = output.trim().to_lowercase();
    if text == "[]" {
        return true;
    }
    match tool {
        "get_music_history" => text.contains("no music activity found"),
        "get_recent_activities" => text.contains("no activity events found"),
        "get_recent_file_changes" => text.contains("no file changes found"),
        "search_ocr" | "get_recent_ocr" => text.contains("no ocr") || text.contains("no matches"),
        "query_activities" => text.contains("[]") || text.contains("no rows"),
        _ => false,
    }
}

fn broaden_tool_args(tool: &str, args: &Value, attempt: usize) -> Value {
    let mut next = args.clone();
    let obj = match next.as_object_mut() {
        Some(v) => v,
        None => return args.clone(),
    };

    let limit = obj.get("limit").and_then(|v| v.as_u64()).unwrap_or(20);
    let hours = obj.get("hours").and_then(|v| v.as_u64()).unwrap_or(24);
    let has_fixed_window = obj.get("start_ts").and_then(|v| v.as_i64()).is_some()
        && obj.get("end_ts").and_then(|v| v.as_i64()).is_some();

    match tool {
        "get_music_history" | "get_recent_activities" | "get_recent_ocr" | "get_recent_file_changes" => {
            let new_limit = std::cmp::min(limit + 20, 250);
            obj.insert("limit".to_string(), Value::Number(serde_json::Number::from(new_limit)));
            if !has_fixed_window {
                let new_hours = std::cmp::min(hours * 2, 168);
                obj.insert("hours".to_string(), Value::Number(serde_json::Number::from(new_hours)));
            }
        }
        "search_ocr" => {
            let new_limit = std::cmp::min(limit + 20, 200);
            obj.insert("limit".to_string(), Value::Number(serde_json::Number::from(new_limit)));
            if attempt == 1 {
                if let Some(keyword) = obj.get("keyword").and_then(|v| v.as_str()) {
                    if keyword.contains(' ') {
                        if let Some(first) = keyword.split_whitespace().next() {
                            obj.insert("keyword".to_string(), Value::String(first.to_string()));
                        }
                    }
                }
            }
        }
        _ => {}
    }
    next
}

fn execute_tool_with_retries(
    conn: &Connection,
    tool: &str,
    args: &Value,
    max_loops: usize,
) -> Result<(String, Vec<Value>, usize), String> {
    let loops = std::cmp::max(max_loops, 1);
    let mut current_args = args.clone();

    for attempt in 1..=loops {
        let (output, activities) = execute_tool(conn, tool, &current_args)?;
        if attempt == loops || !is_low_signal_result(tool, &output, &activities) {
            return Ok((output, activities, attempt));
        }
        current_args = broaden_tool_args(tool, &current_args, attempt);
    }

    Err("Tool execution failed after retries".to_string())
}

fn execute_parallel_search(
    db_path: &std::path::Path,
    args: &Value,
    scope: Option<&TimeScope>,
    user_query: &str,
) -> Result<(String, Vec<Value>), String> {
    let calls = args
        .get("calls")
        .and_then(|v| v.as_array())
        .ok_or("parallel_search requires args.calls array")?;
    if calls.is_empty() {
        return Err("parallel_search requires at least one tool call".to_string());
    }

    let mut handles = Vec::new();
    for call in calls {
        let tool = call
            .get("tool")
            .and_then(|v| v.as_str())
            .ok_or("Each parallel call needs a tool field")?
            .to_string();
        if tool == "parallel_search" {
            return Err("Nested parallel_search is not allowed".to_string());
        }
        let raw_tool_args = call.get("args").cloned().unwrap_or_else(|| serde_json::json!({}));
        let tool_args = if let Some(active_scope) = scope {
            enforce_tool_args_with_scope(&tool, &raw_tool_args, active_scope, user_query)
        } else {
            raw_tool_args
        };
        let db_path = db_path.to_path_buf();

        handles.push(std::thread::spawn(move || -> Result<(String, String, Vec<Value>, usize), String> {
            let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
            let (output, activities, attempts) =
                execute_tool_with_retries(&conn, &tool, &tool_args, MAX_TOOL_RETRY_LOOPS)?;
            Ok((tool, output, activities, attempts))
        }));
    }

    let mut combined_output = format!("Parallel search executed {} tool calls:\n", calls.len());
    let mut combined_activities: Vec<Value> = Vec::new();

    for handle in handles {
        let (tool, output, activities, attempts) = handle
            .join()
            .map_err(|_| "Parallel search worker panicked".to_string())??;
        combined_output.push_str(&format!(
            "- {} (attempts: {})\n",
            tool, attempts
        ));
        combined_output.push_str(&format!(
            "  {}\n",
            truncate_for_token_limit(&normalize_whitespace(&output), 500)
        ));
        combined_activities.extend(transform_activities_for_frontend(&tool, &activities));
    }

    Ok((combined_output, combined_activities))
}

fn execute_tool(conn: &Connection, tool: &str, args: &Value) -> Result<(String, Vec<Value>), String> {
    match tool {
        // Dedicated music history tool - finds songs from Spotify, YouTube, etc.
        "get_music_history" => {
            let limit = args["limit"].as_u64().unwrap_or(100) as i32;
            let hours = args["hours"].as_u64().unwrap_or(24) as i64;
            let scan_limit = std::cmp::max(limit.saturating_mul(20), 500);
            let (start_ts, end_ts) = resolve_window_from_args(args, hours);
            let scope_label = args["scope_label"].as_str().unwrap_or("the selected time range");
            
            // Query a broad slice of recent activity and filter by media_info in Rust.
            // Music can be present while the active app is not an entertainment app.
            let mut stmt = conn.prepare(
                "SELECT app_name, window_title, start_time, duration_seconds, metadata, category_id
                 FROM activities 
                 WHERE start_time >= ?1 AND start_time <= ?2 AND metadata IS NOT NULL
                 ORDER BY start_time DESC 
                 LIMIT ?3"
            ).map_err(|e| format!("SQL Error: {}", e))?;
            
            let rows = stmt.query_map(rusqlite::params![start_ts, end_ts, scan_limit], |row| {
                let app_name: String = row.get(0)?;
                let window_title: String = row.get(1)?;
                let start_time: i64 = row.get(2)?;
                let duration_seconds: i32 = row.get(3)?;
                let metadata_blob: Option<Vec<u8>> = row.get(4)?;
                let category_id: i32 = row.get(5)?;
                
                // Parse metadata to extract media_info
                let media_info = if let Some(blob) = &metadata_blob {
                    if let Ok(meta) = serde_json::from_slice::<ActivityMetadata>(blob) {
                        meta.media_info
                    } else {
                        None
                    }
                } else {
                    None
                };
                
                Ok(serde_json::json!({
                    "app_name": app_name,
                    "window_title": window_title,
                    "start_time": start_time,
                    "duration_seconds": duration_seconds,
                    "media_info": media_info,
                    "category_id": category_id
                }))
            }).map_err(|e| e.to_string())?;
            
            let mut results: Vec<Value> = Vec::new();
            let mut seen_songs: std::collections::HashSet<String> = std::collections::HashSet::new();
            
            for r in rows {
                if let Ok(val) = r {
                    let app_name = val.get("app_name").and_then(|a| a.as_str()).unwrap_or("");
                    
                    // Check if it's Spotify by checking raw bytes (handles encoding issues)
                    // Spotify app name can be "Spotify\u00008\u0016\u0001FileV" with embedded nulls
                    let is_spotify = app_name.as_bytes().windows(7).any(|w| w == b"Spotify") ||
                                     app_name.starts_with("Spotify");
                    
                    // Get media info to check if it's actual music
                    let media = val.get("media_info").and_then(|m| m.as_object());
                    let title = media.as_ref().and_then(|m| m.get("title"))
                        .and_then(|t| t.as_str()).unwrap_or("");
                    let artist = media.as_ref().and_then(|m| m.get("artist"))
                        .and_then(|a| a.as_str()).unwrap_or("");
                    
                    // Keywords that indicate video content, not music
                    let video_keywords = [
                        "tutorial", "course", "how to", "guide", "qwiklab", 
                        "google cloud", "aws cloud", "certification", "#gsp", 
                        "feb 2026", "validate data", "finding data", "interacting with",
                        "vault policies", "google sheets", "data in google"
                    ];
                    let title_lower = title.to_lowercase();
                    let is_video = video_keywords.iter().any(|kw| title_lower.contains(kw));
                    
                    // Check if it looks like a song (has artist and title, not too long)
                    let is_song = !title.is_empty() && !artist.is_empty() && title.len() < 100;
                    
                    // Create a unique key for deduplication
                    let song_key = format!("{}-{}", title, artist);
                    
                    // Include if:
                    // 1. It's Spotify with media info, OR
                    // 2. It has media info that looks like a song (not a video)
                    // And we haven't seen this song before (dedupe)
                    let should_include = (is_spotify && media.is_some()) || 
                                         (media.is_some() && is_song && !is_video);
                    
                    if should_include && !seen_songs.contains(&song_key) {
                        seen_songs.insert(song_key);
                        results.push(val);
                        if results.len() as i32 >= limit {
                            break;
                        }
                    }
                }
            }
            
            // Create activity references for frontend (transform to expected format)
            let activity_refs: Vec<Value> = results.iter().map(|track| {
                let media = track.get("media_info").and_then(|m| m.as_object());
                let category_id = track.get("category_id").and_then(|v| v.as_i64()).unwrap_or(4);
                let category_name = category_name_from_id(category_id);
                // Normalize app name for display (handle Spotify encoding issues)
                let app_raw = track.get("app_name").and_then(|a| a.as_str()).unwrap_or("");
                let is_spotify = app_raw.as_bytes().windows(7).any(|w| w == b"Spotify") || app_raw.starts_with("Spotify");
                let app_display = if is_spotify {
                    "Spotify"
                } else if app_raw.to_lowercase().contains("youtube") {
                    "YouTube"
                } else {
                    app_raw
                };
                serde_json::json!({
                    "app": app_display,
                    "title": track.get("window_title").and_then(|t| t.as_str()).unwrap_or(""),
                    "time": track.get("start_time").and_then(|t| t.as_i64()).unwrap_or(0),
                    "duration_seconds": track.get("duration_seconds").and_then(|d| d.as_i64()).unwrap_or(0),
                    "category": category_name,
                    "media": media.cloned()
                })
            }).collect();
            
                                    // Format for chat display in plain text (no markdown markers)
            let formatted = if results.is_empty() {
                "No music activity found in the specified time range.".to_string()
            } else {
                let mut f = format!("Here are the songs you've listened to in {}:\n\n", scope_label);
                for (i, track) in results.iter().enumerate() {
                    let media = track.get("media_info").and_then(|m| m.as_object());
                    let app_raw = track.get("app_name").and_then(|a| a.as_str()).unwrap_or("");
                    // Normalize Spotify app name (handle encoding issues)
                    let is_spotify = app_raw.as_bytes().windows(7).any(|w| w == b"Spotify") || app_raw.starts_with("Spotify");
                    let app = if is_spotify {
                        "Spotify"
                    } else if app_raw.to_lowercase().contains("youtube") {
                        "YouTube"
                    } else {
                        app_raw
                    };
                    let time = track.get("start_time").and_then(|t| t.as_i64()).unwrap_or(0);
                    // Convert Unix timestamp to local time
                    let dt = chrono::DateTime::from_timestamp(time, 0)
                        .map(|dt| dt.with_timezone(&chrono::Local).format("%I:%M %p").to_string())
                        .unwrap_or_default();

                    if let Some(m) = media {
                        let title = m.get("title").and_then(|t| t.as_str()).unwrap_or("Unknown");
                        let artist = m.get("artist").and_then(|a| a.as_str()).unwrap_or("Unknown");
                        let status = m.get("status").and_then(|s| s.as_str()).unwrap_or("");
                        f.push_str(&format!(
                            "{}. {} - {}\n   {} | {} | {}\n",
                            i + 1,
                            title,
                            artist,
                            app,
                            status,
                            dt
                        ));
                    } else {
                        f.push_str(&format!(
                            "{}. [Unknown track]\n   {} | {}\n",
                            i + 1,
                            app,
                            dt
                        ));
                    }
                }
                f
            };

            Ok((formatted, activity_refs))
        },
        "get_recent_activities" => {
            let limit = args["limit"].as_u64().unwrap_or(100) as i32;
            let hours = args["hours"].as_u64().unwrap_or(24) as i64;
            let category_filter = args["category_id"].as_i64();
            let exclude_media_noise = args["exclude_media_noise"].as_bool().unwrap_or(false);
            let (start_ts, end_ts) = resolve_window_from_args(args, hours);
            let scope_label = args["scope_label"].as_str().unwrap_or("the selected time range");

            let (sql, params): (&str, Vec<rusqlite::types::Value>) = if let Some(cat) = category_filter {
                (
                    "SELECT app_name, window_title, start_time, duration_seconds, category_id, metadata
                     FROM activities
                     WHERE start_time >= ?1 AND start_time <= ?2 AND category_id = ?3
                     ORDER BY start_time DESC
                     LIMIT ?4",
                    vec![
                        rusqlite::types::Value::Integer(start_ts),
                        rusqlite::types::Value::Integer(end_ts),
                        rusqlite::types::Value::Integer(cat),
                        rusqlite::types::Value::Integer(limit as i64),
                    ],
                )
            } else {
                (
                    "SELECT app_name, window_title, start_time, duration_seconds, category_id, metadata
                     FROM activities
                     WHERE start_time >= ?1 AND start_time <= ?2
                     ORDER BY start_time DESC
                     LIMIT ?3",
                    vec![
                        rusqlite::types::Value::Integer(start_ts),
                        rusqlite::types::Value::Integer(end_ts),
                        rusqlite::types::Value::Integer(limit as i64),
                    ],
                )
            };

            let mut stmt = conn.prepare(sql).map_err(|e| format!("SQL Error: {}", e))?;
            let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
                let metadata_blob: Option<Vec<u8>> = row.get(5)?;
                let media_info = metadata_blob
                    .as_ref()
                    .and_then(|blob| serde_json::from_slice::<ActivityMetadata>(blob).ok())
                    .and_then(|m| m.media_info);

                Ok(serde_json::json!({
                    "app_name": row.get::<_, String>(0)?,
                    "window_title": row.get::<_, String>(1)?,
                    "start_time": row.get::<_, i64>(2)?,
                    "duration_seconds": row.get::<_, i32>(3)?,
                    "category_id": row.get::<_, i32>(4)?,
                    "media_info": media_info
                }))
            }).map_err(|e| e.to_string())?;

            let mut events: Vec<Value> = rows.filter_map(|r| r.ok()).collect();
            if exclude_media_noise {
                events.retain(|event| !is_media_noise_event(event));
            }
            let activity_refs: Vec<Value> = events
                .iter()
                .map(|event| {
                    let app = event.get("app_name").and_then(|v| v.as_str()).unwrap_or("");
                    let title = event.get("window_title").and_then(|v| v.as_str()).unwrap_or("");
                    let time = event.get("start_time").and_then(|v| v.as_i64()).unwrap_or(0);
                    let duration = event.get("duration_seconds").and_then(|v| v.as_i64()).unwrap_or(0);
                    let category_id = event.get("category_id").and_then(|v| v.as_i64()).unwrap_or(7);
                    let media = event.get("media_info").cloned();
                    serde_json::json!({
                        "app": app,
                        "title": title,
                        "time": time,
                        "duration_seconds": duration,
                        "category": category_name_from_id(category_id),
                        "media": media
                    })
                })
                .collect();

            let formatted = if events.is_empty() {
                "No activity events found in the selected time range.".to_string()
            } else {
                let mut out = format!(
                    "Here are your recent activity events from {}:\n\n",
                    scope_label
                );
                for (i, event) in events.iter().enumerate() {
                    let app = event.get("app_name").and_then(|v| v.as_str()).unwrap_or("Unknown");
                    let title = event.get("window_title").and_then(|v| v.as_str()).unwrap_or("");
                    let start_time = event.get("start_time").and_then(|v| v.as_i64()).unwrap_or(0);
                    let duration = event.get("duration_seconds").and_then(|v| v.as_i64()).unwrap_or(0);
                    let category_id = event.get("category_id").and_then(|v| v.as_i64()).unwrap_or(7);
                    let dt = chrono::DateTime::from_timestamp(start_time, 0)
                        .map(|dt| dt.with_timezone(&chrono::Local).format("%I:%M %p").to_string())
                        .unwrap_or_else(|| "Unknown time".to_string());
                    out.push_str(&format!(
                        "{}. {} | {} | {} | {}\n   {}\n",
                        i + 1,
                        app,
                        category_name_from_id(category_id),
                        dt,
                        format_duration(duration),
                        if title.is_empty() { "(No window title)".to_string() } else { title.to_string() }
                    ));
                }
                out
            };

            Ok((formatted, activity_refs))
        },
        "get_recent_file_changes" => {
            let limit = args["limit"].as_u64().unwrap_or(40) as i64;
            let hours = args["hours"].as_u64().unwrap_or(24) as i64;
            let change_type = args["change_type"].as_str();
            let (start_ts, end_ts) = resolve_window_from_args(args, hours);
            let scope_label = args["scope_label"].as_str().unwrap_or("the selected time range");
            println!(
                "[Timeline][FileChanges] Query start: start_ts={}, end_ts={}, limit={}, change_type={}",
                start_ts,
                end_ts,
                limit,
                change_type.unwrap_or("any")
            );

            let (sql, params): (&str, Vec<rusqlite::types::Value>) = if let Some(kind) = change_type {
                (
                    "SELECT path, project_root, entity_type, change_type, content_preview, detected_at
                     FROM code_file_events
                     WHERE detected_at >= ?1 AND detected_at <= ?2 AND change_type = ?3
                     ORDER BY detected_at DESC
                     LIMIT ?4",
                    vec![
                        rusqlite::types::Value::Integer(start_ts),
                        rusqlite::types::Value::Integer(end_ts),
                        rusqlite::types::Value::Text(kind.to_string()),
                        rusqlite::types::Value::Integer(limit),
                    ],
                )
            } else {
                (
                    "SELECT path, project_root, entity_type, change_type, content_preview, detected_at
                     FROM code_file_events
                     WHERE detected_at >= ?1 AND detected_at <= ?2
                     ORDER BY detected_at DESC
                     LIMIT ?3",
                    vec![
                        rusqlite::types::Value::Integer(start_ts),
                        rusqlite::types::Value::Integer(end_ts),
                        rusqlite::types::Value::Integer(limit),
                    ],
                )
            };

            let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(rusqlite::params_from_iter(params.iter()), |row| {
                    Ok(serde_json::json!({
                        "path": row.get::<_, String>(0)?,
                        "project_root": row.get::<_, String>(1)?,
                        "entity_type": row.get::<_, String>(2)?,
                        "change_type": row.get::<_, String>(3)?,
                        "content_preview": row.get::<_, Option<String>>(4)?,
                        "detected_at": row.get::<_, i64>(5)?,
                    }))
                })
                .map_err(|e| e.to_string())?;

            let changes: Vec<Value> = rows.filter_map(|r| r.ok()).collect();
            println!(
                "[Timeline][FileChanges] Retrieved {} rows (start_ts={}, end_ts={})",
                changes.len(),
                start_ts,
                end_ts
            );
            for item in &changes {
                let path = item.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let change = item.get("change_type").and_then(|v| v.as_str()).unwrap_or("");
                let entity_type = item.get("entity_type").and_then(|v| v.as_str()).unwrap_or("file");
                let preview = item.get("content_preview").and_then(|v| v.as_str());
                let detected = item.get("detected_at").and_then(|v| v.as_i64()).unwrap_or(0);
                let dt = chrono::DateTime::from_timestamp(detected, 0)
                    .map(|dt| dt.with_timezone(&chrono::Local).format("%Y-%m-%d %I:%M:%S %p").to_string())
                    .unwrap_or_else(|| "Unknown time".to_string());
                println!(
                    "[Timeline][FileChanges] {} | {} {} | {}{}",
                    dt,
                    entity_type,
                    change,
                    path,
                    preview.map(|p| format!(" | {}", p.replace('\n', " "))).unwrap_or_default()
                );
            }
            let formatted = if changes.is_empty() {
                "No file changes found in the selected time range.".to_string()
            } else {
                let mut out = format!("Recent file changes ({}):\n\n", scope_label);
                for (idx, item) in changes.iter().enumerate() {
                    let path = item.get("path").and_then(|v| v.as_str()).unwrap_or("");
                    let project_root = item.get("project_root").and_then(|v| v.as_str()).unwrap_or("");
                    let entity_type = item.get("entity_type").and_then(|v| v.as_str()).unwrap_or("file");
                    let change = item.get("change_type").and_then(|v| v.as_str()).unwrap_or("");
                    let preview = item.get("content_preview").and_then(|v| v.as_str()).unwrap_or("");
                    let detected = item.get("detected_at").and_then(|v| v.as_i64()).unwrap_or(0);
                    let dt = chrono::DateTime::from_timestamp(detected, 0)
                        .map(|dt| dt.with_timezone(&chrono::Local).format("%I:%M %p").to_string())
                        .unwrap_or_else(|| "Unknown time".to_string());
                    out.push_str(&format!(
                        "{}. [{} {}] {} ({})\n   {}\n",
                        idx + 1,
                        entity_type,
                        change,
                        path,
                        dt,
                        project_root
                    ));
                    if !preview.is_empty() {
                        out.push_str(&format!("   Change: {}\n", preview.replace('\n', " ")));
                    }
                }
                out
            };

            Ok((formatted, changes))
        }
        "query_activities" => {
            let sql = args["query"].as_str().or_else(|| args["sql"].as_str())
                .ok_or("Missing 'query' argument")?;
            
            // Basic sanitization (read-only)
            let upper = sql.to_uppercase();
            if upper.contains("DELETE") || upper.contains("UPDATE") || upper.contains("DROP") || upper.contains("INSERT") {
                return Err("Only SELECT queries are allowed.".to_string());
            }

            let mut stmt = conn.prepare(sql).map_err(|e| format!("SQL Error: {}", e))?;
            
            // Map columns to JSON
            let col_count = stmt.column_count();
            let col_names: Vec<String> = stmt.column_names().into_iter().map(|s| s.to_string()).collect();
            
            let rows = stmt.query_map([], |row| {
                let mut map = serde_json::Map::new();
                for i in 0..col_count {
                    let val = match row.get_ref(i)? {
                        rusqlite::types::ValueRef::Null => Value::Null,
                        rusqlite::types::ValueRef::Integer(n) => Value::Number(n.into()),
                        rusqlite::types::ValueRef::Real(n) => serde_json::Number::from_f64(n).map(Value::Number).unwrap_or(Value::Null),
                        rusqlite::types::ValueRef::Text(s) => Value::String(String::from_utf8_lossy(s).to_string()),
                        rusqlite::types::ValueRef::Blob(b) => {
                            // Try to parse metadata blob as JSON
                             if let Ok(meta) = serde_json::from_slice::<ActivityMetadata>(b) {
                                serde_json::json!(meta)
                             } else {
                                Value::String(format!("<blob {} bytes>", b.len()))
                             }
                        }
                    };
                    map.insert(col_names[i].clone(), val);
                }
                Ok(Value::Object(map))
            }).map_err(|e| e.to_string())?;

            let results: Vec<Value> = rows.filter_map(|r| r.ok()).collect();
            Ok((serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string()), results))
        },
        "search_ocr" => {
            let keyword = args["keyword"].as_str().ok_or("Missing keyword")?;
            let limit = args["limit"].as_u64().unwrap_or(100) as usize;
            let hours = args["hours"].as_u64().unwrap_or(24) as i64;
            let (start_ts, end_ts) = resolve_window_from_args(args, hours);
            
            // Search in metadata blobs (inefficient but works for now without FTS5)
            // Ideally we'd have a separate text table.
            let mut stmt = conn.prepare(
                "SELECT start_time, app_name, window_title, duration_seconds, category_id, metadata FROM activities 
                 WHERE start_time >= ?1 AND start_time <= ?2
                 AND LOWER(CAST(metadata AS TEXT)) LIKE ?3
                 ORDER BY start_time DESC LIMIT 20000"
            ).map_err(|e| e.to_string())?;
            
            let mut matches: Vec<Value> = Vec::new();
            let mut seen_snippets = std::collections::HashSet::new();
            let kw_param = format!("%{}%", keyword.to_lowercase());
            
            let rows = stmt.query_map(rusqlite::params![start_ts, end_ts, kw_param], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i32>(3)?,
                    row.get::<_, i32>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?
                ))
            }).map_err(|e| e.to_string())?;
            
            for r in rows {
                if let Ok((start_time, app_name, window_title, duration_seconds, category_id, meta_blob)) = r {
                     if let Some(blob) = meta_blob {
                        if let Ok(meta) = serde_json::from_slice::<ActivityMetadata>(&blob) {
                            if let Some(text) = meta.screen_text {
                                let cleaned = sanitize_ocr_for_query(&text);
                                if cleaned.is_empty() {
                                    continue;
                                }
                                if cleaned.to_lowercase().contains(&keyword.to_lowercase()) {
                                    let snippet = truncate_snippet(&cleaned, &keyword.to_lowercase());
                                    let short = normalize_whitespace(&snippet.chars().take(500).collect::<String>());
                                    if !seen_snippets.insert(short.clone()) {
                                        continue;
                                    }
                                    matches.push(serde_json::json!({
                                        "app_name": app_name,
                                        "window_title": window_title,
                                        "start_time": start_time,
                                        "duration_seconds": duration_seconds,
                                        "category_id": category_id,
                                        "metadata": {
                                            "screen_text": cleaned,
                                            "ocr_snippet": snippet
                                        }
                                    }));
                                    if matches.len() >= limit { break; }
                                }
                            }
                        }
                     }
                }
            }
            let formatted = if matches.is_empty() {
                format!("No OCR results found for '{}'.", keyword)
            } else {
                let mut out = format!("Found {} OCR matches for '{}':\n\n", matches.len(), keyword);
                for (i, item) in matches.iter().enumerate() {
                    let app = item.get("app_name").and_then(|v| v.as_str()).unwrap_or("Unknown");
                    let start_time = item.get("start_time").and_then(|v| v.as_i64()).unwrap_or(0);
                    let dt = chrono::DateTime::from_timestamp(start_time, 0)
                        .map(|dt| dt.with_timezone(&chrono::Local).format("%I:%M %p").to_string())
                        .unwrap_or_else(|| "Unknown time".to_string());
                    let snippet = item
                        .get("metadata")
                        .and_then(|m| m.get("ocr_snippet"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    out.push_str(&format!("{}. {} at {}\n   {}\n", i + 1, app, dt, snippet));
                }
                out
            };
            Ok((formatted, matches))
        },
        "get_recent_ocr" => {
            let limit = args["limit"].as_u64().unwrap_or(100) as usize;
            let hours = args["hours"].as_u64().unwrap_or(24) as i64;
            let app_filter = args["app"].as_str().map(|s| s.to_lowercase());
            let keyword = args["keyword"].as_str().map(|s| s.to_lowercase());
            let (start_ts, end_ts) = resolve_window_from_args(args, hours);
            let scope_label = args["scope_label"].as_str().unwrap_or("the selected time range");
            let scan_limit = std::cmp::max((limit as i64) * 50, 10000);

            let mut stmt = conn.prepare(
                "SELECT start_time, app_name, window_title, duration_seconds, category_id, metadata
                 FROM activities
                 WHERE start_time >= ?1 AND start_time <= ?2 AND metadata IS NOT NULL
                 AND (?4 IS NULL OR LOWER(app_name) LIKE ?4)
                 AND (?5 IS NULL OR LOWER(CAST(metadata AS TEXT)) LIKE ?5)
                 ORDER BY start_time DESC
                 LIMIT ?3"
            ).map_err(|e| e.to_string())?;

            let app_param = app_filter.as_ref().map(|a| format!("%{}%", a));
            let kw_param = keyword.as_ref().map(|k| format!("%{}%", k));

            let rows = stmt.query_map(rusqlite::params![start_ts, end_ts, scan_limit, app_param, kw_param], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i32>(3)?,
                    row.get::<_, i32>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?
                ))
            }).map_err(|e| e.to_string())?;

            let mut seen_snippets = std::collections::HashSet::new();
            let mut results: Vec<Value> = Vec::new();
            for row in rows {
                if let Ok((start_time, app_name, window_title, duration_seconds, category_id, metadata_blob)) = row {
                    if let Some(blob) = metadata_blob {
                        if let Ok(meta) = serde_json::from_slice::<ActivityMetadata>(&blob) {
                            if let Some(text) = meta.screen_text {
                                let normalized_text = sanitize_ocr_for_query(&text);
                                if normalized_text.is_empty() {
                                    continue;
                                }

                                if let Some(ref app_q) = app_filter {
                                    if !app_name.to_lowercase().contains(app_q) {
                                        continue;
                                    }
                                }
                                if let Some(ref kw) = keyword {
                                    if !normalized_text.to_lowercase().contains(kw) {
                                        continue;
                                    }
                                }

                                let short = normalize_whitespace(&normalized_text.chars().take(500).collect::<String>());
                                if !seen_snippets.insert(short.clone()) {
                                    continue;
                                }

                                results.push(serde_json::json!({
                                    "app_name": app_name,
                                    "window_title": window_title,
                                    "start_time": start_time,
                                    "duration_seconds": duration_seconds,
                                    "category_id": category_id,
                                    "metadata": {
                                        "screen_text": normalized_text,
                                        "ocr_snippet": short
                                    }
                                }));

                                if results.len() >= limit {
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            let activity_refs: Vec<Value> = results.iter().map(|item| {
                let app = item.get("app_name").and_then(|v| v.as_str()).unwrap_or("");
                let title = item.get("window_title").and_then(|v| v.as_str()).unwrap_or("");
                let time = item.get("start_time").and_then(|v| v.as_i64()).unwrap_or(0);
                let duration = item.get("duration_seconds").and_then(|v| v.as_i64()).unwrap_or(0);
                let category_id = item.get("category_id").and_then(|v| v.as_i64()).unwrap_or(7);
                serde_json::json!({
                    "app": app,
                    "title": title,
                    "time": time,
                    "duration_seconds": duration,
                    "category": category_name_from_id(category_id),
                    "media": Value::Null
                })
            }).collect();

            let formatted = if results.is_empty() {
                "No OCR snippets found in the selected time range.".to_string()
            } else {
                let mut out = format!("Recent OCR snippets ({}):\n\n", scope_label);
                for (i, item) in results.iter().enumerate() {
                    let app = item.get("app_name").and_then(|v| v.as_str()).unwrap_or("Unknown");
                    let start_time = item.get("start_time").and_then(|v| v.as_i64()).unwrap_or(0);
                    let dt = chrono::DateTime::from_timestamp(start_time, 0)
                        .map(|dt| dt.with_timezone(&chrono::Local).format("%I:%M %p").to_string())
                        .unwrap_or_else(|| "Unknown time".to_string());
                    let snippet = item
                        .get("metadata")
                        .and_then(|m| m.get("ocr_snippet"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    out.push_str(&format!("{}. {} at {}\n   {}\n", i + 1, app, dt, snippet));
                }
                out
            };

            Ok((formatted, activity_refs))
        },
        "get_usage_stats" => {
             let start = args["start_time_iso"].as_str().unwrap_or("");
            let end = args["end_time_iso"].as_str().unwrap_or("");
            
            let s_ts = parse_iso_to_unix(start).unwrap_or(0);
            let e_ts = parse_iso_to_unix(end).unwrap_or(chrono::Utc::now().timestamp());
            
            let mut stmt = conn.prepare(
                "SELECT app_name, SUM(duration_seconds) as total_dur, COUNT(*) as cnt
                 FROM activities 
                 WHERE start_time >= ?1 AND start_time <= ?2 
                 GROUP BY app_name
                 ORDER BY total_dur DESC LIMIT 20"
            ).map_err(|e| e.to_string())?;
            
            let rows = stmt.query_map(rusqlite::params![s_ts, e_ts], |row: &rusqlite::Row| {
                Ok(serde_json::json!({
                    "app": row.get::<_, String>(0)?,
                    "total_seconds": row.get::<_, i64>(1)?,
                    "count": row.get::<_, i32>(2)?
                }))
            }).map_err(|e| e.to_string())?;
            
            let results: Vec<Value> = rows.filter_map(|r: Result<Value, rusqlite::Error>| r.ok()).collect();
            Ok((serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string()), results))
        },
        "query_history" => {
             // Alias for old query_activities call?
              Err("Use query_activities instead".to_string()) 
        },
        _ => Err(format!("Unknown tool: {}", tool))
    }
}

// ─── Helpers ───

fn category_name_from_id(category_id: i64) -> &'static str {
    match category_id {
        1 => "Development",
        2 => "Browser",
        3 => "Communication",
        4 => "Entertainment",
        5 => "Productivity",
        6 => "System",
        _ => "Other",
    }
}

fn transform_activities_for_frontend(tool: &str, tool_activities: &[Value]) -> Vec<Value> {
    if tool == "get_music_history"
        || tool == "get_recent_activities"
        || tool == "get_recent_ocr"
        || tool == "parallel_search"
    {
        return tool_activities.to_vec();
    }

    if tool == "get_recent_file_changes" {
        return tool_activities
            .iter()
            .map(|item| {
                let path = item.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let entity_type = item.get("entity_type").and_then(|v| v.as_str()).unwrap_or("file");
                let change_type = item.get("change_type").and_then(|v| v.as_str()).unwrap_or("changed");
                let content_preview = item.get("content_preview").and_then(|v| v.as_str()).unwrap_or("");
                let title = if content_preview.is_empty() {
                    format!("[{} {}] {}", entity_type, change_type, path)
                } else {
                    format!(
                        "[{} {}] {} | {}",
                        entity_type,
                        change_type,
                        path,
                        content_preview.replace('\n', " ")
                    )
                };
                serde_json::json!({
                    "app": "File Monitor",
                    "title": title,
                    "time": item.get("detected_at").and_then(|v| v.as_i64()).unwrap_or(0),
                    "duration_seconds": 0,
                    "category": "Development",
                    "media": Value::Null
                })
            })
            .collect();
    }

    if tool == "query_activities" || tool == "search_ocr" {
        let mut transformed = Vec::new();
        for act in tool_activities {
            let media = act.get("metadata").and_then(|m| m.get("media_info")).cloned();
            let category_id = act.get("category_id").and_then(|v| v.as_i64()).unwrap_or(0);
            transformed.push(serde_json::json!({
                "app": act.get("app_name").and_then(|v| v.as_str()).unwrap_or(""),
                "title": act.get("window_title").and_then(|v| v.as_str()).unwrap_or(""),
                "time": act.get("start_time").and_then(|v| v.as_i64()).unwrap_or(0),
                "duration_seconds": act.get("duration_seconds").and_then(|v| v.as_i64()).unwrap_or(0),
                "category": category_name_from_id(category_id),
                "media": media,
            }));
        }
        return transformed;
    }

    Vec::new()
}

fn format_duration(total_seconds: i64) -> String {
    if total_seconds <= 0 {
        return "0s".to_string();
    }
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

fn normalize_final_answer(answer: &str) -> String {
    answer
        .replace("â€“", "-")
        .replace("â€”", "-")
        .trim()
        .to_string()
}

fn try_parse_tool_call_response(full_response: &str) -> Option<AgentResponse> {
    let trimmed = full_response.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(resp) = serde_json::from_str::<AgentResponse>(trimmed) {
        if matches!(resp, AgentResponse::ToolCall { .. }) {
            return Some(resp);
        }
    }

    if trimmed.contains("\"tool\"") && trimmed.contains("\"args\"") {
        let start = trimmed.find('{')?;
        let end = trimmed.rfind('}')?;
        if end > start {
            let candidate = &trimmed[start..=end];
            if let Ok(resp) = serde_json::from_str::<AgentResponse>(candidate) {
                if matches!(resp, AgentResponse::ToolCall { .. }) {
                    return Some(resp);
                }
            }
        }
    }

    None
}

fn parse_iso_to_unix(iso: &str) -> Option<i64> {
    if iso.is_empty() { return None; }
    chrono::DateTime::parse_from_rfc3339(iso).ok().map(|dt| dt.timestamp())
        .or_else(|| {
             chrono::NaiveDateTime::parse_from_str(iso, "%Y-%m-%dT%H:%M:%S")
                .ok()
                .and_then(|dt| dt.and_local_timezone(chrono::Local).single())
                .map(|dt| dt.timestamp())
        })
}

fn truncate_for_token_limit(text: &str, limit_chars: usize) -> String {
    if text.len() <= limit_chars {
        text.to_string()
    } else {
        // Safe char boundary truncation
        let end = text.char_indices().nth(limit_chars).map(|(i, _)| i).unwrap_or(text.len());
        format!("{}... [truncated]", &text[..end])
    }
}

fn truncate_snippet(text: &str, keyword: &str) -> String {
    if let Some(idx) = text.to_lowercase().find(keyword) {
        // Safe char boundary calculation
        let start_char_idx = text[..idx].chars().count().saturating_sub(150);
        let start = text.char_indices().nth(start_char_idx).map(|(i, _)| i).unwrap_or(0);
        
        // Find end byte safely
        let end_char_idx = start_char_idx + 300 + keyword.len(); // approximate
        let end = text.char_indices().nth(end_char_idx).map(|(i, _)| i).unwrap_or(text.len());

        format!("...{}...", &text[start..end])
    } else {
        text.chars().take(300).collect()
    }
}

fn normalize_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn sanitize_ocr_for_query(text: &str) -> String {
    let compact = normalize_whitespace(text);
    if compact.is_empty() {
        return String::new();
    }

    let filtered: String = compact
        .chars()
        .filter(|c| {
            c.is_alphanumeric()
                || c.is_whitespace()
                || ",.;:!?()[]{}'\"/@#&+-_|".contains(*c)
        })
        .collect();
    let cleaned = normalize_whitespace(&filtered);
    if cleaned.len() < 3 {
        return String::new();
    }
    if looks_like_gibberish(&cleaned) {
        return String::new();
    }
    cleaned
}

fn looks_like_gibberish(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return true;
    }
    let total = chars.len() as f64;
    let letters = chars.iter().filter(|c| c.is_alphabetic()).count() as f64;
    let digits = chars.iter().filter(|c| c.is_ascii_digit()).count() as f64;
    let symbols = chars
        .iter()
        .filter(|c| !c.is_alphanumeric() && !c.is_whitespace())
        .count() as f64;
    let vowels = chars
        .iter()
        .filter(|c| "aeiouAEIOU".contains(**c))
        .count() as f64;

    let symbol_ratio = symbols / total;
    let alpha_ratio = letters / total;
    let digit_ratio = digits / total;
    let vowel_ratio = if letters > 0.0 { vowels / letters } else { 0.0 };

    symbol_ratio > 0.35 || alpha_ratio < 0.18 || (letters > 10.0 && vowel_ratio < 0.06) || digit_ratio > 0.7
}

async fn synthesize_answer_from_evidence(
    app_handle: &tauri::AppHandle,
    model: &str,
    api_key: &str,
    user_query: &str,
    scope: &TimeScope,
    steps: &[AgentStep],
    activities: &[Value],
) -> Result<String, String> {
    let mut evidence_lines: Vec<String> = Vec::new();
    for (i, step) in steps.iter().take(8).enumerate() {
        evidence_lines.push(format!(
            "{}. {} -> {}",
            i + 1,
            step.tool_name,
            truncate_for_token_limit(&step.tool_result, 8000)
        ));
    }

    let summary_prompt = format!(
        "User query: {query}\nTime scope: {label} ({start} to {end})\nEvidence items: {count}\nTool evidence:\n{evidence}\n\nReturn a detailed, crisp, and highly specific final answer. Break down the activities chronologically or by major tasks. Mention specific window titles, exact times, and specific apps/websites visited. Do not just give a high-level summary of time spent. Provide a rich narrative of what the user was actually doing. Do not call tools. If evidence is weak, clearly state uncertainty.",
        query = user_query,
        label = scope.label,
        start = format_time_scope_ts(scope.start_ts),
        end = format_time_scope_ts(scope.end_ts),
        count = activities.len(),
        evidence = evidence_lines.join("\n\n"),
    );

    let mut out = String::new();
    let on_token = |chunk: &str| {
        let _ = app_handle.emit("chat://token", chunk);
    };
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: "You are a precise assistant. Produce one final answer from provided evidence only. Provide specific details, window titles, and times. No tool JSON.".to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: summary_prompt,
        },
    ];
    call_llm_stream(model, api_key, &messages, &mut out, on_token).await?;
    if matches!(try_parse_tool_call_response(&out), Some(AgentResponse::ToolCall { .. })) {
        return Ok("I gathered evidence but could not produce a stable final summary. Please ask with a specific app/date and I’ll answer exactly.".to_string());
    }
    Ok(normalize_final_answer(&out))
}

// Streaming LLM Call
async fn call_llm_stream<F>(
    model: &str, 
    api_key: &str, 
    messages: &[ChatMessage], 
    output_buffer: &mut String,
    mut on_token: F
) -> Result<(), String> 
where F: FnMut(&str) {
    let client = reqwest::Client::builder()
        .timeout(StdDuration::from_secs(LLM_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("Failed to init HTTP client: {}", e))?;
    
    let request = ChatRequest {
        model: model.to_string(),
        messages: messages.to_vec(),
        temperature: 0.0,
        max_tokens: 1024,
        stream: true,
    };

    let mut response = client
        .post("https://integrate.api.nvidia.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Net err: {}", e))?;

    if !response.status().is_success() {
        // Read full body error
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("API Error {}: {}", status, text));
    }

    // Process stream line by line
    let mut buffer = String::new();
    let mut reasoning_open = false;
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        let chunk_str = String::from_utf8_lossy(&chunk);
        buffer.push_str(&chunk_str);
        
        // Split by lines
        let lines: Vec<&str> = buffer.split('\n').collect();
        // Keep the last part if it doesn't end with \n
        let last_part = if chunk_str.ends_with('\n') {
            String::new()
        } else {
            lines.last().unwrap_or(&"").to_string()
        };
        
        // Process complete lines
        for line in lines {
            let line = line.trim();
            if line.starts_with("data: ") {
                let data = &line[6..];
                if data == "[DONE]" { break; }
                
                if let Ok(stream_resp) = serde_json::from_str::<ChatStreamResponse>(data) {
                    if let Some(choice) = stream_resp.choices.first() {
                        if let Some(ref reasoning) = choice.delta.reasoning_content {
                            if !reasoning.is_empty() {
                                if !reasoning_open {
                                    output_buffer.push_str("<think>");
                                    on_token("<think>");
                                    reasoning_open = true;
                                }
                                output_buffer.push_str(reasoning);
                                on_token(reasoning);
                            }
                        }
                        if let Some(ref content) = choice.delta.content {
                            if reasoning_open {
                                output_buffer.push_str("</think>");
                                on_token("</think>");
                                reasoning_open = false;
                            }
                            output_buffer.push_str(content);
                            on_token(content);
                        }
                    }
                }
            }
        }
        
        buffer = last_part;
    }

    if reasoning_open {
        output_buffer.push_str("</think>");
        on_token("</think>");
    }

    Ok(())
}

// Kept for backward compat if needed, but we don't really use it now
async fn call_llm(model: &str, api_key: &str, messages: &[ChatMessage]) -> Result<String, String> {
    let mut out = String::new();
    call_llm_stream(model, api_key, messages, &mut out, |_| {}).await?;
    Ok(out)
}


