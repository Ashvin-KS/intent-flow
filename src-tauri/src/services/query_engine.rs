use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::models::{Settings, ActivityMetadata};
use tauri::{Manager, Emitter};

// ─── Constants ───

const MAX_TURNS: usize = 7;
const MAX_TOOL_RETRY_LOOPS: usize = 3;

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
   - Args: hours (default 24), limit (default 40), category_id (optional)
   - Returns chronological activity events with app, title, category, duration, and time

3. `query_activities` - SQL queries on the `activities` table
   - Fields: app_name, window_title, start_time (unix timestamp), duration_seconds, category_id, metadata
   - metadata.media_info contains {title, artist, status} for music

4. `get_usage_stats` - Aggregated stats by app
   - Args: start_time_iso, end_time_iso

5. `search_ocr` - Search screen text content
   - Args: keyword, limit

6. `get_recent_ocr` - Browse recent OCR captures (including chats) without exact keyword
   - Args: hours (default 24), limit (default 20), app (optional), keyword (optional)
   - Returns recent OCR snippets with app and timestamp

7. `parallel_search` - Run multiple tool calls in parallel for broader coverage
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
8. For broad/ambiguous requests, prefer parallel_search with 2-3 tool calls
9. Use conversation history to resolve references like "it", "that", "the previous one", "what was it about".

## Response Format
Output JSON for tool calls: { "tool": "tool_name", "args": { ... }, "reasoning": "..." }
Output plain text for final answers (no JSON, no markdown, no **bold** markers).

Do NOT output markdown code blocks for tool calls. Output RAW JSON only.
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

// ─── Public API ───

pub async fn run_agentic_search(
    app_handle: &tauri::AppHandle,
    user_query: &str,
    settings: &Settings,
) -> Result<String, String> {
    // Delegate to the step-tracking version, just return the answer
    let result = run_agentic_search_with_steps(app_handle, user_query, settings).await?;
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
    run_agentic_search_with_steps_and_history(app_handle, user_query, settings, &[]).await
}

pub async fn run_agentic_search_with_steps_and_history(
    app_handle: &tauri::AppHandle,
    user_query: &str,
    settings: &Settings,
    prior_messages: &[ChatMessage],
) -> Result<AgentResult, String> {
    let api_key = &settings.ai.api_key;
    let model = &settings.ai.model;
    
    if api_key.is_empty() {
        return Err("AI is disabled or API key is missing".to_string());
    }

    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let mut steps: Vec<AgentStep> = Vec::new();
    let mut all_activities: Vec<Value> = Vec::new();
    
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
        content: format!("User query: \"{}\"\nCurrent Time: {}", user_query, chrono::Local::now().to_rfc3339()),
    });

    for turn in 0..MAX_TURNS {
        // 1. Call LLM with streaming callback
        // We accumulate the full content here, while also streaming it to the frontend
        let mut full_response = String::new();
        // Callback to handle streaming chunks
        let on_token = |chunk: &str| {
            // Simple heuristic: if it starts with {, it's likely a tool call (don't stream text/answer)
            // But we can't know for sure until we have enough chars.
            // For now, let's just stream everything. The frontend can decide to show/hide based on logic or user preference.
            // Actually, better: if it's the final answer (not JSON), we stream.
            // If it's a tool call (JSON), we might want to suppress it or show "Thinking..."
            
            // Send event to frontend
            let _ = app_handle.emit("chat://token", chunk);
        };

        call_llm_stream(model, api_key, &messages, &mut full_response, on_token).await?;

        // 2. Parse Response
        // Try to parse as ToolCall JSON first
        let parsed_response = if full_response.trim().starts_with('{') {
             match serde_json::from_str::<AgentResponse>(&full_response) {
                Ok(resp) => resp,
                Err(_) => {
                    // Maybe it was just a string starting with {? Unlikely for tool calls.
                    // Or malformed JSON.
                    // Fallback to treating as FinalAnswer
                    AgentResponse::FinalAnswer(full_response.clone())
                }
             }
        } else {
            // Plain text
            AgentResponse::FinalAnswer(full_response.clone())
        };

        // 3. Handle Action
        match parsed_response {
            AgentResponse::FinalAnswer(answer) => {
                // Done!
                let _ = app_handle.emit("chat://done", "final_answer");
                return Ok(AgentResult {
                    answer: normalize_final_answer(&answer),
                    steps,
                    activities_referenced: all_activities,
                });
            }
            AgentResponse::ToolCall { tool, args, reasoning } => {
                println!("[Agent] Turn {}: Calling {} ({:?})", turn + 1, tool, args);
                // Notify frontend of agent step (tool call) start?
                // For now, frontend just sees tokens.
                
                // Add assistant message to history
                messages.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: full_response.clone(),
                });

                // Execute tool with bounded retry loops and optional parallelization
                let (tool_output, tool_activities, attempts_used) = if tool == "parallel_search" {
                    let parallel_count = args
                        .get("calls")
                        .and_then(|v| v.as_array())
                        .map(|v| v.len())
                        .unwrap_or(0);
                    let _ = app_handle.emit(
                        "chat://token",
                        format!("\n[Agent] Running {} searches in parallel...\n", parallel_count),
                    );
                    let (out, activities) = execute_parallel_search(&db_path, &args)?;
                    (out, activities, 1usize)
                } else {
                    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
                    execute_tool_with_retries(&conn, &tool, &args, MAX_TOOL_RETRY_LOOPS)?
                };

                // Add activities from tool result to referenced activities
                all_activities.extend(transform_activities_for_frontend(&tool, &tool_activities));
                
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
                    tool_args: args.clone(),
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

    Ok(AgentResult {
        answer: "I reached the maximum number of steps without finding a definitive answer. Please try a more specific query.".to_string(),
        steps,
        activities_referenced: all_activities,
    })
}

// ─── Tool Execution ───

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

    match tool {
        "get_music_history" | "get_recent_activities" | "get_recent_ocr" => {
            let new_limit = std::cmp::min(limit + 20, 250);
            let new_hours = std::cmp::min(hours * 2, 168);
            obj.insert("limit".to_string(), Value::Number(serde_json::Number::from(new_limit)));
            obj.insert("hours".to_string(), Value::Number(serde_json::Number::from(new_hours)));
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
        let tool_args = call.get("args").cloned().unwrap_or_else(|| serde_json::json!({}));
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
            
            let now = chrono::Utc::now().timestamp();
            let lookback = now - (hours * 3600);
            
            // Query a broad slice of recent activity and filter by media_info in Rust.
            // Music can be present while the active app is not an entertainment app.
            let mut stmt = conn.prepare(
                "SELECT app_name, window_title, start_time, duration_seconds, metadata, category_id
                 FROM activities 
                 WHERE start_time > ?1 AND metadata IS NOT NULL
                 ORDER BY start_time DESC 
                 LIMIT ?2"
            ).map_err(|e| format!("SQL Error: {}", e))?;
            
            let rows = stmt.query_map(rusqlite::params![lookback, scan_limit], |row| {
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
                let mut f = format!("Here are the songs you've listened to in the last {} hours:\n\n", hours);
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
            let limit = args["limit"].as_u64().unwrap_or(40) as i32;
            let hours = args["hours"].as_u64().unwrap_or(24) as i64;
            let category_filter = args["category_id"].as_i64();

            let now = chrono::Utc::now().timestamp();
            let lookback = now - (hours * 3600);

            let (sql, params): (&str, Vec<rusqlite::types::Value>) = if let Some(cat) = category_filter {
                (
                    "SELECT app_name, window_title, start_time, duration_seconds, category_id, metadata
                     FROM activities
                     WHERE start_time > ?1 AND category_id = ?2
                     ORDER BY start_time DESC
                     LIMIT ?3",
                    vec![
                        rusqlite::types::Value::Integer(lookback),
                        rusqlite::types::Value::Integer(cat),
                        rusqlite::types::Value::Integer(limit as i64),
                    ],
                )
            } else {
                (
                    "SELECT app_name, window_title, start_time, duration_seconds, category_id, metadata
                     FROM activities
                     WHERE start_time > ?1
                     ORDER BY start_time DESC
                     LIMIT ?2",
                    vec![
                        rusqlite::types::Value::Integer(lookback),
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

            let events: Vec<Value> = rows.filter_map(|r| r.ok()).collect();
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
                    "Here are your recent activity events from the last {} hours:\n\n",
                    hours
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
            let limit = args["limit"].as_u64().unwrap_or(20) as usize;
            
            // Search in metadata blobs (inefficient but works for now without FTS5)
            // Ideally we'd have a separate text table.
            let mut stmt = conn.prepare(
                "SELECT start_time, app_name, window_title, duration_seconds, category_id, metadata FROM activities 
                 WHERE start_time > ?1
                 ORDER BY start_time DESC LIMIT 1000"
            ).map_err(|e| e.to_string())?;
            
            // Default lookback 24h
            let now = chrono::Utc::now().timestamp();
            let lookback = now - 86400; // 24h
            
            let mut matches: Vec<Value> = Vec::new();
            
            let rows = stmt.query_map([lookback], |row| {
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
            let limit = args["limit"].as_u64().unwrap_or(20) as usize;
            let hours = args["hours"].as_u64().unwrap_or(24) as i64;
            let app_filter = args["app"].as_str().map(|s| s.to_lowercase());
            let keyword = args["keyword"].as_str().map(|s| s.to_lowercase());

            let now = chrono::Utc::now().timestamp();
            let lookback = now - (hours * 3600);
            let scan_limit = std::cmp::max((limit as i64) * 20, 1000);

            let mut stmt = conn.prepare(
                "SELECT start_time, app_name, window_title, duration_seconds, category_id, metadata
                 FROM activities
                 WHERE start_time > ?1 AND metadata IS NOT NULL
                 ORDER BY start_time DESC
                 LIMIT ?2"
            ).map_err(|e| e.to_string())?;

            let rows = stmt.query_map(rusqlite::params![lookback, scan_limit], |row| {
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

                                let short = normalize_whitespace(&normalized_text.chars().take(220).collect::<String>());
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
                let mut out = format!("Recent OCR snippets (last {} hours):\n\n", hours);
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
        .replace("**", "")
        .replace('•', "-")
        .replace("â€“", "-")
        .replace("â€”", "-")
        .trim()
        .to_string()
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
        let start_char_idx = text[..idx].chars().count().saturating_sub(50);
        let start = text.char_indices().nth(start_char_idx).map(|(i, _)| i).unwrap_or(0);
        
        // Find end byte safely
        let end_char_idx = start_char_idx + 100 + keyword.len(); // approximate
        let end = text.char_indices().nth(end_char_idx).map(|(i, _)| i).unwrap_or(text.len());

        format!("...{}...", &text[start..end])
    } else {
        text.chars().take(100).collect()
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

// Streaming LLM Call
async fn call_llm_stream<F>(
    model: &str, 
    api_key: &str, 
    messages: &[ChatMessage], 
    output_buffer: &mut String,
    mut on_token: F
) -> Result<(), String> 
where F: FnMut(&str) {
    let client = reqwest::Client::new();
    
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
                        if let Some(ref content) = choice.delta.content {
                            output_buffer.push_str(content);
                            on_token(content);
                        }
                    }
                }
            }
        }
        
        buffer = last_part;
    }

    Ok(())
}

// Kept for backward compat if needed, but we don't really use it now
async fn call_llm(model: &str, api_key: &str, messages: &[ChatMessage]) -> Result<String, String> {
    let mut out = String::new();
    call_llm_stream(model, api_key, messages, &mut out, |_| {}).await?;
    Ok(out)
}


