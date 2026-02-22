use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::models::{Settings, ActivityMetadata};
use tauri::{Manager, Emitter};
use chrono::{Datelike, Duration, Local, TimeZone};
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

9. `resolve_query_scope` - Widen the time range or request additional data sources
   - Args: suggested_scope (one of: "today", "yesterday", "last_3_days", "last_7_days", "last_30_days", "this_year", "all_time"), enable_sources (optional array of: "apps", "screen", "media", "browser", "files"), reason (string explaining why)
   - Use when user's query implies a different time range than what is currently selected (e.g. "few days back", "from the start", "not just today", "earlier")
   - Use when you need data sources that are not currently enabled
   - Returns a confirmation prompt to the user; after user confirms, the query re-runs with the new scope
   - ALWAYS use this tool when the user says things like "not just today", "days back", "from the start", "earlier", "before", "across days", "overall", "from few days", etc.

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
17. For person-identity queries (for example: "who is my crush"), never guess. Gather evidence using at least 2 distinct tools first; if evidence is weak or conflicting, ask a clarification question.
18. For any non-trivial factual query, fetch tool evidence before giving a final answer. If a final answer is attempted without evidence, call tools first.
19. Never claim the user texted/chatted someone unless there is explicit chat-app evidence (e.g., WhatsApp/Telegram/Instagram chat OCR/activity) in the current time scope.
20. For large-range summaries (like "this year" or "all time"), collect evidence in multiple compact aggregation steps (usage stats + grouped SQL rollups + focused slices) before writing the final answer.
21. For complex queries, especially those about people, relationships, or identifying someone (e.g., "who is my crush"), you MUST make a minimum of 5 distinct tool calls to gather comprehensive evidence across different apps, timeframes, and contexts before providing a final answer. Do not jump to conclusions based on limited recent data.
22. If the user asks a general question about habits, preferences, relationships, history, or asks "when", "how often", "first time", "ever" AND the current scope is narrow (like "Today" or "Last 7 Days"), you MUST call `resolve_query_scope` IMMEDIATELY as your first tool call to widen the scope to "last_30_days" or "all_time". Do NOT attempt to answer general or historical questions with just a few days of data. Also use this tool if the user's query implies a time range broader than the current scope (e.g., "few days back", "not just today", "earlier", "from the start", "before", "overall", "from the beginning", "across days", "the other day", "days ago", "recently" when scope is Today).
23. If you detect the user needs data from sources that are not currently enabled (e.g., asking about files but Files source is disabled, or asking about browser history but Browser source is disabled), call `resolve_query_scope` with the required enable_sources array so the user can enable them.

## Response Format
Output JSON for tool calls: { "tool": "tool_name", "args": { ... }, "reasoning": "..." }
Output detailed, crisp, and highly specific final answers. Use markdown (like bolding and bullet points) to make the answer easy to read.
For final answers, include:
- A direct answer first
- Evidence bullets with specific app/window/title + timestamp
- A short confidence statement
- If evidence is incomplete, explicitly say what is missing
Do not be overly brief for non-trivial queries.

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
    let resolved_scope = resolve_time_scope(time_scope);
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

    let needs_broad_scope = requires_broad_scope(user_query);
    let scope_warning = if needs_broad_scope && (resolved_scope.id == "today" || resolved_scope.id == "yesterday" || resolved_scope.id == "last_3_days" || resolved_scope.id == "last_7_days") {
        "\nCRITICAL: Your current search scope is narrow, but the user's query requires historical data, aggregation, or general knowledge about their habits/relationships. You MUST call `resolve_query_scope` immediately to widen the scope to 'last_30_days' or 'all_time' before doing anything else."
    } else {
        ""
    };

    messages.push(ChatMessage {
        role: "user".to_string(),
        content: format!(
            "User query: \"{}\"\nCurrent Time: {}\nSelected Time Scope: {} ({} to {})\nAlways keep retrieval strictly inside this scope unless the user asks to change it. If you need to search for people, names, or girls, use `search_ocr` with a high limit and try different keywords or no keywords at all to get all the data. If you need to search for chats, use `get_recent_ocr` with a high limit and try different apps like \"whatsapp\", \"instagram\", \"telegram\", etc.{}",
            user_query,
            chrono::Local::now().to_rfc3339(),
            resolved_scope.label,
            format_time_scope_ts(resolved_scope.start_ts),
            format_time_scope_ts(resolved_scope.end_ts),
            scope_warning
        ),
    });

    let use_long_range_pipeline = should_use_long_range_pipeline(user_query, &resolved_scope, &intent);
    if use_long_range_pipeline {
        let _ = app_handle.emit("chat://status", "Building long-range evidence (multi-step)...");
        if let Ok((pipeline_steps, pipeline_activities, digest)) =
            run_long_range_summary_pipeline(&db_path, &resolved_scope, &intent, user_query)
        {
            let start_turn = steps.len();
            for (idx, mut step) in pipeline_steps.into_iter().enumerate() {
                step.turn = start_turn + idx + 1;
                steps.push(step);
            }
            if !pipeline_activities.is_empty() {
                all_activities.extend(pipeline_activities);
                dedupe_activities(&mut all_activities);
                if !intent.wants_music {
                    all_activities.retain(|item| !is_media_activity_ref(item));
                }
            }
            messages.push(ChatMessage {
                role: "user".to_string(),
                content: format!(
                    "Pre-aggregated long-range evidence:\n{}\nUse this structured evidence first. Only call extra tools if there are clear gaps.",
                    truncate_for_token_limit(&digest, 3500)
                ),
            });
        }
    } else if intent.broad_summary {
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

    let must_validate_with_tools = requires_evidence_for_query(user_query);
    let mut final_without_evidence_attempts = 0usize;
    let mut forced_parallel_runs = 0usize;

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
            // Suppress any content containing "reasoning" key — this is always from tool-call JSON
            if sniff.contains("\"reasoning\"") {
                suppress_stream = true;
            }
            if contains_internal_tool_markup(&sniff) {
                suppress_stream = true;
            }

            if !suppress_stream {
                let cleaned = strip_internal_stream_markup(chunk);
                if !cleaned.trim().is_empty() {
                    let _ = app_handle.emit("chat://token", &cleaned);
                }
            }
        };

        call_llm_stream(model, &api_key, &messages, &mut full_response, on_token).await?;

        // 2. Parse Response
        let parsed_response = try_parse_tool_call_response(&full_response)
            .unwrap_or_else(|| AgentResponse::FinalAnswer(full_response.clone()));

        // 3. Handle Action
        match parsed_response {
            AgentResponse::FinalAnswer(answer) => {
                let cleaned_answer = strip_internal_stream_markup(&answer)
                    .replace("<think>", "")
                    .replace("</think>", "");
                let normalized = normalize_final_answer_hardened(&cleaned_answer);
                let normalized = scrub_unsupported_communication_claims(&normalized, user_query, &steps);
                if must_validate_with_tools && steps.is_empty() && forced_parallel_runs < 2 {
                    let forced_args = build_forced_validation_parallel_args(&resolved_scope, &intent, user_query);
                    let (out, activities) = execute_parallel_search(
                        &db_path,
                        &forced_args,
                        Some(&resolved_scope),
                        user_query,
                    )?;
                    forced_parallel_runs += 1;
                    if !activities.is_empty() {
                        all_activities.extend(activities);
                        dedupe_activities(&mut all_activities);
                    }
                    let truncated = truncate_for_token_limit(&out, 8000);
                    steps.push(AgentStep {
                        turn: turn + 1,
                        tool_name: "parallel_search".to_string(),
                        tool_args: forced_args,
                        tool_result: truncated.clone(),
                        reasoning: "Forced evidence validation before final answer".to_string(),
                    });
                    messages.push(ChatMessage {
                        role: "assistant".to_string(),
                        content: full_response.clone(),
                    });
                    messages.push(ChatMessage {
                        role: "user".to_string(),
                        content: format!(
                            "You attempted to answer without evidence. Use this forced evidence and continue with additional tool calls if needed:\n{}",
                            truncate_for_token_limit(&truncated, 3500)
                        ),
                    });
                    continue;
                }
                if contains_internal_tool_markup(&normalized) && turn + 1 < MAX_TURNS {
                    messages.push(ChatMessage {
                        role: "assistant".to_string(),
                        content: full_response.clone(),
                    });
                    messages.push(ChatMessage {
                        role: "user".to_string(),
                        content: "Your last response leaked internal tool-call markup. Re-emit either valid RAW JSON tool call {\"tool\":\"...\",\"args\":{...}} or a normal final answer. Never output internal markers like <|tool_call_begin|>.".to_string(),
                    });
                    continue;
                }
                
                let is_complex_query = user_query.to_lowercase().contains("who") || user_query.to_lowercase().contains("crush") || user_query.to_lowercase().contains("relationship");
                if is_complex_query && steps.len() < 5 && turn + 1 < MAX_TURNS {
                    messages.push(ChatMessage {
                        role: "assistant".to_string(),
                        content: full_response.clone(),
                    });
                    messages.push(ChatMessage {
                        role: "user".to_string(),
                        content: format!("You have only made {} tool calls. For this type of query, you MUST make at least 5 distinct tool calls to gather comprehensive evidence before answering. Please make another tool call.", steps.len()),
                    });
                    continue;
                }

                if !has_minimum_evidence_for_query(user_query, &steps) {
                    final_without_evidence_attempts += 1;
                    if must_validate_with_tools && final_without_evidence_attempts >= 2 && forced_parallel_runs < 2 {
                        let forced_args = build_forced_validation_parallel_args(&resolved_scope, &intent, user_query);
                        let (out, activities) = execute_parallel_search(
                            &db_path,
                            &forced_args,
                            Some(&resolved_scope),
                            user_query,
                        )?;
                        forced_parallel_runs += 1;
                        if !activities.is_empty() {
                            all_activities.extend(activities);
                            dedupe_activities(&mut all_activities);
                        }
                        let truncated = truncate_for_token_limit(&out, 8000);
                        steps.push(AgentStep {
                            turn: turn + 1,
                            tool_name: "parallel_search".to_string(),
                            tool_args: forced_args,
                            tool_result: truncated.clone(),
                            reasoning: "Forced cross-tool evidence after weak finalization attempt".to_string(),
                        });
                        messages.push(ChatMessage {
                            role: "assistant".to_string(),
                            content: full_response.clone(),
                        });
                        messages.push(ChatMessage {
                            role: "user".to_string(),
                            content: format!(
                                "Your answer was not sufficiently evidenced. Continue using this tool output and fetch more if needed:\n{}",
                                truncate_for_token_limit(&truncated, 3500)
                            ),
                        });
                        continue;
                    }
                    if turn + 1 < MAX_TURNS {
                        messages.push(ChatMessage {
                            role: "assistant".to_string(),
                            content: full_response.clone(),
                        });
                        messages.push(ChatMessage {
                            role: "user".to_string(),
                            content: "Do not finalize yet. First gather stronger evidence with multiple relevant tools (for example OCR + activity/chat/file-change tools), then answer only from that evidence. If evidence is still weak, say so explicitly and ask a clarifying question.".to_string(),
                        });
                        continue;
                    }
                    let _ = app_handle.emit("chat://done", "final_answer");
                    let action_marker = build_insufficient_evidence_action_marker(user_query, &resolved_scope);
                    return Ok(AgentResult {
                        answer: format!(
                            "I don't have enough cross-checked evidence to answer confidently. Try widening the time range (Last 7 Days or All Time) and enabling Browser History / Files & Documents, then ask me to retry.{}",
                            action_marker
                        ),
                        steps,
                        activities_referenced: all_activities,
                    });
                }
                // Done!
                let _ = app_handle.emit("chat://done", "final_answer");
                return Ok(AgentResult {
                    answer: normalized,
                    steps,
                    activities_referenced: all_activities,
                });
            }
            AgentResponse::ToolCall { tool, args, reasoning } => {
                // Handle resolve_query_scope as a special case — it returns a user-facing action prompt
                if tool == "resolve_query_scope" {
                    let suggested_scope = args["suggested_scope"].as_str().unwrap_or("last_7_days");
                    let reason = args["reason"].as_str().unwrap_or("Your query requires a wider search range.");
                    let enable_sources: Vec<String> = args.get("enable_sources")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                        .unwrap_or_default();

                    steps.push(AgentStep {
                        turn: turn + 1,
                        tool_name: "resolve_query_scope".to_string(),
                        tool_args: args.clone(),
                        tool_result: format!("Requesting scope change to {} with sources {:?}", suggested_scope, enable_sources),
                        reasoning: reasoning.as_deref().unwrap_or("").to_string(),
                    });

                    let payload = serde_json::json!({
                        "kind": "confirm_scope_or_sources",
                        "reason": reason,
                        "suggested_time_range": suggested_scope,
                        "enable_sources": enable_sources,
                        "retry_message": user_query
                    });

                    let _ = app_handle.emit("chat://done", "final_answer");
                    return Ok(AgentResult {
                        answer: format!(
                            "I can answer this more accurately after your confirmation.\n\n[[IF_ACTION:{}]]",
                            payload
                        ),
                        steps,
                        activities_referenced: all_activities,
                    });
                }

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
                if !intent.wants_music {
                    all_activities.retain(|item| !is_media_activity_ref(item));
                }
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
        || q.contains("this year")
        || q.contains("yearly")
        || q.contains("annual")
        || (wants_timeline && (wants_ocr || wants_files || wants_music));

    QueryIntent {
        wants_music,
        wants_ocr,
        wants_files,
        wants_timeline,
        broad_summary,
    }
}

fn requires_broad_scope(query: &str) -> bool {
    let q = query.to_lowercase();
    
    let time_indicators = [
        "first", "last time", "ever", "always", "never", "usually", "often", 
        "history", "past", "before", "earlier", "since", "overall", "all time",
        "months", "years", "weeks", "days ago", "long time", "recently"
    ];
    
    let general_questions = [
        "how many times", "how often", "when did i", "longest", "best", "worst", 
        "favorite", "most", "top", "frequent"
    ];
    
    let identity_questions = [
        "who is", "what is my", "guess", "crush", "relationship", "friend", "girlfriend", "boyfriend"
    ];

    time_indicators.iter().any(|&w| q.contains(w)) ||
    general_questions.iter().any(|&w| q.contains(w)) ||
    identity_questions.iter().any(|&w| q.contains(w))
}

fn query_has_time_hint(query: &str) -> bool {
    let q = query.to_lowercase();
    q.contains("today")
        || q.contains("yesterday")
        || q.contains("last ")
        || q.contains("past ")
        || q.contains("this year")
        || q.contains("this week")
        || q.contains("this month")
        || q.contains("right now")
        || q.contains("few mins")
        || q.contains("few days")
        || q.contains("days back")
        || q.contains("days ago")
        || q.contains("recently")
        || q.contains("earlier")
        || q.contains("before")
        || q.contains("the other day")
        || q.contains("not just today")
        || q.contains("from the start")
        || q.contains("from start")
        || q.contains("beginning")
        || q.contains("ever")
        || q.contains("overall")
        || q.contains("always")
        || q.contains("couple day")
        || q.contains("couple week")
        || q.contains("few weeks")
        || q.contains("month ago")
        || q.contains("week ago")
        || q.chars().any(|c| c.is_ascii_digit())
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

fn resolve_time_scope(explicit_scope: Option<&str>) -> TimeScope {
    let now = chrono::Utc::now().timestamp();
    let scope_id = explicit_scope
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_lowercase())
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
        "this_year" => {
            let local_now = Local::now();
            let year = local_now.year();
            let start_naive = chrono::NaiveDate::from_ymd_opt(year, 1, 1)
                .and_then(|d| d.and_hms_opt(0, 0, 0));
            let start_ts = start_naive
                .and_then(|dt| local_now.timezone().from_local_datetime(&dt).single())
                .map(|dt| dt.timestamp())
                .unwrap_or(now - 365 * 86400);
            TimeScope { id: scope_id, label: "This Year".to_string(), start_ts, end_ts: now }
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

fn build_forced_validation_parallel_args(scope: &TimeScope, intent: &QueryIntent, query: &str) -> Value {
    let mut calls = vec![
        serde_json::json!({
            "tool": "get_recent_activities",
            "args": { "limit": if scope.id == "all_time" { 120 } else { 80 }, "exclude_media_noise": !intent.wants_music }
        }),
        serde_json::json!({
            "tool": "get_recent_ocr",
            "args": { "limit": if scope.id == "all_time" { 120 } else { 80 } }
        }),
    ];

    if intent.wants_files || query.to_lowercase().contains("project") || query.to_lowercase().contains("file") || query.to_lowercase().contains("code") {
        calls.push(serde_json::json!({
            "tool": "get_recent_file_changes",
            "args": { "limit": if scope.id == "all_time" { 100 } else { 60 } }
        }));
    }

    if intent.wants_music || query.to_lowercase().contains("song") || query.to_lowercase().contains("music") {
        calls.push(serde_json::json!({
            "tool": "get_music_history",
            "args": { "limit": if scope.id == "all_time" { 80 } else { 50 } }
        }));
    }

    serde_json::json!({ "calls": calls })
}

fn should_use_long_range_pipeline(query: &str, scope: &TimeScope, intent: &QueryIntent) -> bool {
    let q = query.to_lowercase();
    let summary_like = q.contains("summary")
        || q.contains("overview")
        || q.contains("recap")
        || q.contains("what did i do")
        || q.contains("this year")
        || q.contains("yearly")
        || q.contains("annual");
    if !summary_like {
        return false;
    }
    let span_days = ((scope.end_ts - scope.start_ts).max(0)) / 86_400;
    scope.id == "this_year" || scope.id == "all_time" || span_days >= 90 || intent.broad_summary
}

fn run_long_range_summary_pipeline(
    db_path: &std::path::Path,
    scope: &TimeScope,
    intent: &QueryIntent,
    user_query: &str,
) -> Result<(Vec<AgentStep>, Vec<Value>, String), String> {
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    let mut steps: Vec<AgentStep> = Vec::new();
    let mut all_refs: Vec<Value> = Vec::new();
    let mut digest_parts: Vec<String> = Vec::new();

    // Step 1: Aggregate app usage for the whole range.
    execute_and_record_long_range_step(
        &conn,
        scope,
        user_query,
        "get_usage_stats",
        serde_json::json!({}),
        "Aggregate usage baseline for long-range summary",
        &mut steps,
        &mut all_refs,
        &mut digest_parts,
    )?;

    // Step 2: Monthly category rollup to avoid feeding raw per-event data.
    let monthly_category_sql = format!(
        "SELECT strftime('%Y-%m', datetime(start_time, 'unixepoch', 'localtime')) AS month, category_id, SUM(duration_seconds) AS total_seconds, COUNT(*) AS events \
         FROM activities WHERE start_time >= {} AND start_time <= {} \
         GROUP BY month, category_id \
         ORDER BY month DESC, total_seconds DESC LIMIT 600",
        scope.start_ts,
        scope.end_ts
    );
    execute_and_record_long_range_step(
        &conn,
        scope,
        user_query,
        "query_activities",
        serde_json::json!({ "query": monthly_category_sql }),
        "Monthly category aggregation for long-range compression",
        &mut steps,
        &mut all_refs,
        &mut digest_parts,
    )?;

    // Step 3: Top apps over the full range.
    let top_apps_sql = format!(
        "SELECT app_name, SUM(duration_seconds) AS total_seconds, COUNT(*) AS events \
         FROM activities WHERE start_time >= {} AND start_time <= {} \
         GROUP BY app_name \
         ORDER BY total_seconds DESC LIMIT 40",
        scope.start_ts,
        scope.end_ts
    );
    execute_and_record_long_range_step(
        &conn,
        scope,
        user_query,
        "query_activities",
        serde_json::json!({ "query": top_apps_sql }),
        "Top apps aggregation for the selected long-range window",
        &mut steps,
        &mut all_refs,
        &mut digest_parts,
    )?;

    // Step 4: Recent high-signal activity slice for concrete examples.
    execute_and_record_long_range_step(
        &conn,
        scope,
        user_query,
        "get_recent_activities",
        serde_json::json!({
            "limit": if scope.id == "all_time" { 300 } else { 220 },
            "exclude_media_noise": !intent.wants_music
        }),
        "Concrete activity slice for examples and chronology",
        &mut steps,
        &mut all_refs,
        &mut digest_parts,
    )?;

    let q = user_query.to_lowercase();
    let needs_files = intent.wants_files || q.contains("project") || q.contains("repo") || q.contains("code");
    if needs_files {
        execute_and_record_long_range_step(
            &conn,
            scope,
            user_query,
            "get_recent_file_changes",
            serde_json::json!({ "limit": if scope.id == "all_time" { 220 } else { 160 } }),
            "File-change slice for project/work summary",
            &mut steps,
            &mut all_refs,
            &mut digest_parts,
        )?;
    }

    let needs_chat = intent.wants_ocr || q.contains("chat") || q.contains("text") || q.contains("message");
    if needs_chat {
        execute_and_record_long_range_step(
            &conn,
            scope,
            user_query,
            "get_recent_ocr",
            serde_json::json!({ "limit": if scope.id == "all_time" { 220 } else { 160 } }),
            "OCR/chat slice for communication evidence",
            &mut steps,
            &mut all_refs,
            &mut digest_parts,
        )?;
    }

    if intent.wants_music || q.contains("music") || q.contains("song") {
        execute_and_record_long_range_step(
            &conn,
            scope,
            user_query,
            "get_music_history",
            serde_json::json!({ "limit": if scope.id == "all_time" { 140 } else { 100 } }),
            "Music slice for media trend evidence",
            &mut steps,
            &mut all_refs,
            &mut digest_parts,
        )?;
    }

    let digest = digest_parts.join("\n\n");
    Ok((steps, all_refs, digest))
}

fn execute_and_record_long_range_step(
    conn: &Connection,
    scope: &TimeScope,
    user_query: &str,
    tool: &str,
    raw_args: Value,
    reasoning: &str,
    steps: &mut Vec<AgentStep>,
    all_refs: &mut Vec<Value>,
    digest_parts: &mut Vec<String>,
) -> Result<(), String> {
    let enforced_args = enforce_tool_args_with_scope(tool, &raw_args, scope, user_query);
    let (tool_output, tool_activities, attempts_used) =
        execute_tool_with_retries(conn, tool, &enforced_args, MAX_TOOL_RETRY_LOOPS)?;
    let with_retry_note = if attempts_used > 1 {
        format!(
            "Auto-retried with broader search {} time(s).\n{}",
            attempts_used - 1,
            tool_output
        )
    } else {
        tool_output
    };
    let truncated = truncate_for_token_limit(&with_retry_note, 10000);
    steps.push(AgentStep {
        turn: 0,
        tool_name: tool.to_string(),
        tool_args: enforced_args.clone(),
        tool_result: truncated.clone(),
        reasoning: reasoning.to_string(),
    });
    let refs = transform_activities_for_frontend(tool, &tool_activities);
    all_refs.extend(refs);
    digest_parts.push(format!(
        "{} -> {}",
        tool,
        truncate_for_token_limit(&normalize_whitespace(&truncated), 900)
    ));
    Ok(())
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

fn is_media_activity_ref(item: &Value) -> bool {
    if item.get("media").and_then(|v| v.get("title")).is_some() {
        return true;
    }
    let app = item.get("app").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
    app.contains("spotify") || app.contains("youtube music") || app.contains("apple music")
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
        "resolve_query_scope" => {
            // This tool lets the LLM request a wider time scope or additional sources.
            // It returns a confirmation action marker that the frontend will show to the user.
            let suggested_scope = args["suggested_scope"].as_str().unwrap_or("last_7_days");
            let reason = args["reason"].as_str().unwrap_or("Your query requires a wider search range.");
            let enable_sources: Vec<String> = args.get("enable_sources")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();

            let payload = serde_json::json!({
                "kind": "confirm_scope_or_sources",
                "reason": reason,
                "suggested_time_range": suggested_scope,
                "enable_sources": enable_sources,
                "retry_message": "" // Will use the original query on retry
            });

            let output = format!(
                "Scope change requested: time range → {}, additional sources → [{}]. Reason: {}",
                suggested_scope,
                enable_sources.join(", "),
                reason
            );

            Ok((output, vec![payload]))
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
                     if app_name.to_lowercase().contains("intentflow") {
                         continue;
                     }
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
                    if app_name.to_lowercase().contains("intentflow") {
                        continue;
                    }
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

fn normalize_final_answer_hardened(answer: &str) -> String {
    let cleaned = normalize_final_answer(answer);
    let cleaned = strip_think_blocks(&cleaned);
    let cleaned = strip_internal_stream_markup(&cleaned);
    let cleaned = strip_reasoning_fragments(&cleaned);
    cleaned
        .lines()
        .filter(|line| !contains_internal_tool_markup(line))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Strip leaked `"reasoning": "..."` fragments and partial JSON tool-call debris from text.
fn strip_reasoning_fragments(text: &str) -> String {
    use regex::Regex;
    // Match lines that are just reasoning fragments like:
    //   , "reasoning": "some text here."}
    //   "reasoning": "text"}
    let re = Regex::new(r#"(?m)^\s*,?\s*"reasoning"\s*:\s*"[^}]*$"#).unwrap_or_else(|_| Regex::new(".").unwrap());
    let result = re.replace_all(text, "").to_string();
    
    // Also strip lines that look like orphan fragments: starting with , "key": "..."
    let re2 = Regex::new(r#"(?m)^\s*,\s*"\w+"\s*:\s*"[^"]*"\s*\}?\s*$"#).unwrap_or_else(|_| Regex::new(".").unwrap());
    let result = re2.replace_all(&result, "").to_string();
    
    // Strip any remaining lines that are just `}` with no context
    result
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            // Keep non-empty lines that aren't just JSON debris
            !trimmed.is_empty() || true // keep blank lines for formatting
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn try_parse_tool_call_response(full_response: &str) -> Option<AgentResponse> {
    let cleaned = strip_internal_stream_markup(full_response);
    // Also strip <think>...</think> blocks that may wrap the tool call
    let cleaned = strip_think_blocks(&cleaned);
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 1. Try direct parse
    if let Ok(resp) = serde_json::from_str::<AgentResponse>(trimmed) {
        if matches!(resp, AgentResponse::ToolCall { .. }) {
            return Some(resp);
        }
    }

    if trimmed.contains("\"tool\"") && trimmed.contains("\"args\"") {
        // 2. Try extracting a top-level JSON object
        let start = trimmed.find('{')?;
        let end = trimmed.rfind('}')?;
        if end > start {
            let candidate = &trimmed[start..=end];
            if let Ok(resp) = serde_json::from_str::<AgentResponse>(candidate) {
                if matches!(resp, AgentResponse::ToolCall { .. }) {
                    return Some(resp);
                }
            }

            // 3. Try stripping the "reasoning" field entirely (it often has unescaped quotes)
            if let Some(fixed) = try_fix_broken_reasoning_json(candidate) {
                if let Ok(resp) = serde_json::from_str::<AgentResponse>(&fixed) {
                    if matches!(resp, AgentResponse::ToolCall { .. }) {
                        return Some(resp);
                    }
                }
            }
        }
    }

    None
}

/// Strip <think>...</think> blocks (potentially unclosed) from a string.
fn strip_think_blocks(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut remaining = text;
    loop {
        let lower = remaining.to_lowercase();
        if let Some(open_pos) = lower.find("<think") {
            // Find end of opening tag
            let tag_end = remaining[open_pos..].find('>').map(|i| open_pos + i + 1).unwrap_or(remaining.len());
            result.push_str(&remaining[..open_pos]);
            if let Some(close_pos) = lower[tag_end..].find("</think>") {
                remaining = &remaining[tag_end + close_pos + 8..];
            } else {
                // Unclosed tag - strip everything after it
                break;
            }
        } else {
            result.push_str(remaining);
            break;
        }
    }
    result
}

/// Try to fix broken JSON where the "reasoning" field has unescaped quotes.
/// Strategy: find the "reasoning" key, find its value boundaries via brace/bracket depth,
/// and either strip the field or fix the quoting.
fn try_fix_broken_reasoning_json(json_str: &str) -> Option<String> {
    // Strategy 1: Remove the "reasoning" field entirely and re-parse
    // Find `"reasoning"` key position
    let reasoning_key = json_str.find("\"reasoning\"")?;
    let colon_pos = json_str[reasoning_key + 11..].find(':')? + reasoning_key + 11;
    
    // Find the start of the value (skip whitespace after colon)
    let value_start_region = &json_str[colon_pos + 1..];
    let value_offset = value_start_region.find(|c: char| !c.is_whitespace())?;
    let abs_value_start = colon_pos + 1 + value_offset;
    
    if json_str.as_bytes().get(abs_value_start)? != &b'"' {
        return None; // Not a string value
    }
    
    // The value starts with a quote. We need to find where it truly ends.
    // Since quotes inside are unescaped, find the last `"}` or `", ` before the final `}`.
    // The final `}` of the whole object is at the end.
    let last_brace = json_str.rfind('}')?;
    
    // Remove the reasoning field: everything from the comma (or opening) before "reasoning" to just before the closing brace
    // Find the comma before "reasoning"
    let before_reasoning = json_str[..reasoning_key].trim_end();
    let stripped_before = if before_reasoning.ends_with(',') {
        &before_reasoning[..before_reasoning.len() - 1]
    } else {
        before_reasoning
    };
    
    // Build JSON without reasoning field
    let fixed = format!("{}\n{}", stripped_before.trim_end(), &json_str[last_brace..]);
    Some(fixed)
}

fn requires_multi_tool_validation(query: &str) -> bool {
    let q = query.to_lowercase();
    q.contains("who")
        || q.contains("which")
        || q.contains("name")
        || q.contains("chat")
        || q.contains("message")
        || q.contains("project")
        || q.contains("code")
        || q.contains("file")
        || q.contains("did i")
        || q.contains("what did")
        || q.contains("evidence")
        || q.contains("confirm")
}

fn is_smalltalk_query(query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.len() <= 12 && (q == "hi" || q == "hello" || q == "hey" || q == "yo") {
        return true;
    }
    q.contains("how are you")
        || q.contains("thanks")
        || q.contains("thank you")
        || q.contains("good morning")
        || q.contains("good night")
}

fn requires_evidence_for_query(query: &str) -> bool {
    if is_smalltalk_query(query) {
        return false;
    }
    true
}

fn build_insufficient_evidence_action_marker(query: &str, scope: &TimeScope) -> String {
    let mut enable_sources: Vec<&str> = Vec::new();
    let q = query.to_lowercase();
    if q.contains("project") || q.contains("repo") || q.contains("code") || q.contains("file") {
        enable_sources.push("files");
    }
    if q.contains("browser") || q.contains("website") || q.contains("history") || q.contains("linkedin") {
        enable_sources.push("browser");
    }
    if q.contains("chat") || q.contains("text") || q.contains("message") {
        enable_sources.push("screen");
    }

    let suggested_scope = if q.contains("this year") && scope.id != "this_year" {
        "this_year"
    } else if scope.id != "all_time" {
        "all_time"
    } else if scope.id != "last_7_days" {
        "last_7_days"
    } else {
        ""
    };

    if suggested_scope.is_empty() && enable_sources.is_empty() {
        return String::new();
    }

    let payload = serde_json::json!({
        "kind": "confirm_scope_or_sources",
        "reason": "Evidence is insufficient in the current scope/source settings.",
        "suggested_time_range": if suggested_scope.is_empty() { Value::Null } else { Value::String(suggested_scope.to_string()) },
        "enable_sources": enable_sources,
        "retry_message": query
    });
    format!("\n\n[[IF_ACTION:{}]]", payload)
}

fn has_minimum_evidence_for_query(query: &str, steps: &[AgentStep]) -> bool {
    if !requires_evidence_for_query(query) {
        return true;
    }
    let evidence_steps = collect_evidence_tool_names(steps);
    if evidence_steps.is_empty() {
        return false;
    }
    if requires_multi_tool_validation(query) {
        if evidence_steps.len() < 2 {
            return false;
        }
        if is_project_query(query) && !has_project_evidence(steps) {
            return false;
        }
        if is_identity_or_romance_query(query) {
            return has_explicit_chat_evidence(steps) || has_non_chat_strong_identity_evidence(steps);
        }
        return true;
    }
    if is_project_query(query) {
        return has_project_evidence(steps);
    }
    evidence_steps.len() >= 1
}

fn is_project_query(query: &str) -> bool {
    let q = query.to_lowercase();
    q.contains("project")
        || q.contains("projects")
        || q.contains("code")
        || q.contains("repo")
        || q.contains("worked on")
        || q.contains("work i did")
}

fn has_project_evidence(steps: &[AgentStep]) -> bool {
    for step in steps {
        let name = step.tool_name.as_str();
        if name == "get_recent_file_changes" {
            return step_has_material_evidence(step);
        }
        let out = step.tool_result.to_lowercase();
        if out.contains(".ts")
            || out.contains(".tsx")
            || out.contains(".js")
            || out.contains(".rs")
            || out.contains(".py")
            || out.contains("github")
            || out.contains("repo")
            || out.contains("pull request")
        {
            return true;
        }
    }
    false
}

fn is_identity_or_romance_query(query: &str) -> bool {
    let q = query.to_lowercase();
    q.contains("crush")
        || q.contains("girl")
        || q.contains("boy")
        || q.contains("name")
        || q.contains("whom do i")
        || q.contains("who do i")
        || q.contains("love")
}

fn has_explicit_chat_evidence(steps: &[AgentStep]) -> bool {
    for step in steps {
        let out = step.tool_result.to_lowercase();
        if out.contains("whatsapp")
            || out.contains("telegram")
            || out.contains("instagram")
            || out.contains("chat")
            || out.contains("message")
        {
            return true;
        }
    }
    false
}

fn has_non_chat_strong_identity_evidence(steps: &[AgentStep]) -> bool {
    let mut support_hits = 0usize;
    for step in steps {
        if !step_has_material_evidence(step) {
            continue;
        }
        let out = step.tool_result.to_lowercase();
        if out.contains("linkedin")
            || out.contains("profile")
            || out.contains("call log")
            || out.contains("contact")
            || out.contains("frequent")
        {
            support_hits += 1;
        }
    }
    support_hits >= 2
}

fn scrub_unsupported_communication_claims(answer: &str, query: &str, steps: &[AgentStep]) -> String {
    if !is_identity_or_romance_query(query) || has_explicit_chat_evidence(steps) {
        return answer.to_string();
    }

    let lower = answer.to_lowercase();
    let likely_chat_claim = lower.contains("texted")
        || lower.contains("chatted")
        || lower.contains("whatsapp")
        || lower.contains("messaged");
    if !likely_chat_claim {
        return answer.to_string();
    }

    format!(
        "{}\n\nNote: I don't have explicit chat-app evidence in this time range, so I cannot claim texting/chats.",
        answer
    )
}

fn collect_evidence_tool_names(steps: &[AgentStep]) -> std::collections::HashSet<String> {
    let mut distinct = std::collections::HashSet::new();
    for step in steps {
        if !step_has_material_evidence(step) {
            continue;
        }
        match step.tool_name.as_str() {
            "get_recent_ocr" | "search_ocr" | "get_recent_activities" | "query_activities" | "get_recent_file_changes" | "get_music_history" | "get_usage_stats" => {
                distinct.insert(step.tool_name.clone());
            }
            "parallel_search" => {
                if let Some(calls) = step.tool_args.get("calls").and_then(|v| v.as_array()) {
                    for call in calls {
                        if let Some(tool) = call.get("tool").and_then(|v| v.as_str()) {
                            distinct.insert(tool.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    distinct
}

fn step_has_material_evidence(step: &AgentStep) -> bool {
    let out = step.tool_result.trim();
    if out.is_empty() || out == "[]" || out == "{}" {
        return false;
    }
    let lower = out.to_lowercase();
    !(lower.contains("no results")
        || lower.contains("0 result")
        || lower.contains("no matching")
        || lower.contains("\"results\":[]")
        || lower.contains("\"items\":[]"))
}

fn contains_internal_tool_markup(text: &str) -> bool {
    let lower = text.to_lowercase();
    text.contains("<|tool_")
        || lower.contains("tool_calls_section_begin")
        || lower.contains("tool_call_begin")
        || lower.contains("tool_call_argument_begin")
        || lower.contains("tool_calls_section_end")
}

fn strip_internal_stream_markup(text: &str) -> String {
    text
        .replace("<|tool_calls_section_begin|>", "")
        .replace("<|tool_calls_section_end|>", "")
        .replace("<|tool_call_begin|>", "")
        .replace("<|tool_call_end|>", "")
        .replace("<|tool_call_argument_begin|>", "")
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
        "User query: {query}\nTime scope: {label} ({start} to {end})\nEvidence items: {count}\nTool evidence:\n{evidence}\n\nReturn a detailed, crisp, and highly specific final answer. Start with a direct answer, then provide evidence bullets with exact times, app names, and window titles. Break down activities chronologically or by major tasks. Do not just give a high-level summary of time spent. Include a short confidence statement and explicitly list missing evidence when uncertain. Do not call tools.",
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
            content: "You are a precise assistant. Produce one final answer from provided evidence only. Be detailed, specific, and not overly brief. Include direct answer, evidence bullets, and confidence. No tool JSON.".to_string(),
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
    let cleaned = strip_internal_stream_markup(&out)
        .replace("<think>", "")
        .replace("</think>", "");
    let normalized = normalize_final_answer_hardened(&cleaned);
    Ok(scrub_unsupported_communication_claims(&normalized, user_query, steps))
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
        max_tokens: 1600,
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
