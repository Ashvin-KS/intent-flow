use tauri::{AppHandle, Manager};
use crate::models::{QueryResult, QueryItem};
use chrono::{TimeZone, Utc};

#[tauri::command]
pub async fn execute_query(
    app_handle: AppHandle,
    query: String,
) -> Result<QueryResult, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    // Parse the query and determine time range
    let (start_time, end_time, summary) = parse_query_time_range(&query);
    
    // Get activities for the time range
    let activities = crate::database::queries::get_activities(&conn, start_time, end_time, Some(50))
        .map_err(|e| e.to_string())?;
    
    // Convert to query items
    let results: Vec<QueryItem> = activities.iter().map(|a| {
        let dt = Utc.timestamp_opt(a.start_time, 0).unwrap();
        let time_str = dt.format("%I:%M %p").to_string();
        let duration = format_duration(a.duration_seconds);
        
        QueryItem {
            timestamp: a.start_time,
            time_str,
            activity: format!("{} - {}", a.app_name, a.window_title),
            duration,
            details: None,
        }
    }).collect();
    
    Ok(QueryResult {
        query: query.clone(),
        results,
        summary,
        timestamp: Utc::now().timestamp(),
    })
}

#[tauri::command]
pub async fn get_query_history(
    _app_handle: AppHandle,
    _limit: Option<i32>,
) -> Result<Vec<QueryResult>, String> {
    // Return query history from cache
    Ok(vec![]) // Placeholder
}

fn parse_query_time_range(query: &str) -> (i64, i64, String) {
    let now = Utc::now();
    let query_lower = query.to_lowercase();
    
    if query_lower.contains("yesterday") {
        let yesterday = now - chrono::Duration::days(1);
        let start = yesterday.date_naive().and_hms_opt(0, 0, 0).unwrap();
        let end = yesterday.date_naive().and_hms_opt(23, 59, 59).unwrap();
        (
            start.and_utc().timestamp(),
            end.and_utc().timestamp(),
            "Here's what you did yesterday:".to_string(),
        )
    } else if query_lower.contains("today") {
        let start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
        let end = now;
        (
            start.and_utc().timestamp(),
            end.timestamp(),
            "Here's what you've done today:".to_string(),
        )
    } else if query_lower.contains("morning") {
        let start = now.date_naive().and_hms_opt(6, 0, 0).unwrap();
        let end = now.date_naive().and_hms_opt(12, 0, 0).unwrap();
        (
            start.and_utc().timestamp(),
            end.and_utc().timestamp(),
            "Here's what you did this morning:".to_string(),
        )
    } else if query_lower.contains("afternoon") {
        let start = now.date_naive().and_hms_opt(12, 0, 0).unwrap();
        let end = now.date_naive().and_hms_opt(18, 0, 0).unwrap();
        (
            start.and_utc().timestamp(),
            end.and_utc().timestamp(),
            "Here's what you did this afternoon:".to_string(),
        )
    } else {
        // Default to today
        let start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
        let end = now;
        (
            start.and_utc().timestamp(),
            end.timestamp(),
            "Here's what I found:".to_string(),
        )
    }
}

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
