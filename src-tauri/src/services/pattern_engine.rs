use tauri::AppHandle;
use std::time::Duration;
use rusqlite::Connection;
use serde::{Serialize, Deserialize};
use tauri::Manager;

// Only run pattern analysis every 30 minutes
const ANALYSIS_INTERVAL_SECS: u64 = 30 * 60; 

pub fn start_pattern_engine(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        println!("[PatternEngine] ⏳ Waiting 60s before first analysis run...");
        tokio::time::sleep(Duration::from_secs(60)).await;
        
        println!("[PatternEngine] ✅ Service started (runs every 30m)");
        
        loop {
            println!("[PatternEngine] 🧠 Running pattern analysis...");
            if let Err(e) = run_analysis(&app_handle) {
                println!("[PatternEngine] ❌ Analysis failed: {}", e);
            }
            
            tokio::time::sleep(Duration::from_secs(ANALYSIS_INTERVAL_SECS)).await;
        }
    });
}

fn run_analysis(app_handle: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app_handle.path().app_data_dir()?;
    let db_path = data_dir.join("intentflow.db");
    let conn = Connection::open(db_path)?;
    
    // 1. Time-of-day Patterns
    // "User often opens VS Code between 9am-10am"
    analyze_time_patterns(&conn)?;
    
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct TimePatternData {
    app_name: String,
    hour: u32,
    day_type: String,
}

fn analyze_time_patterns(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    let now = chrono::Utc::now().timestamp();
    let two_weeks_ago = now - (14 * 24 * 3600);
    
    let mut stmt = conn.prepare(r#"
        SELECT 
            app_name, 
            strftime('%H', datetime(start_time, 'unixepoch', 'localtime')) as hour,
            count(*) as frequency
        FROM activities 
        WHERE start_time > ?1
        GROUP BY app_name, hour
        HAVING frequency > 5
        ORDER BY frequency DESC
    "#)?;
    
    let rows = stmt.query_map([two_weeks_ago], |row: &rusqlite::Row| {
        let app: String = row.get(0)?;
        let hour_str: String = row.get(1)?;
        let count: i32 = row.get(2)?;
        let hour: u32 = hour_str.parse().unwrap_or(0);
        Ok((app, hour, count))
    })?;
    
    for row in rows {
        let (app, hour, count) = row?;
        
        let pattern_data = TimePatternData {
            app_name: app.clone(),
            hour,
            day_type: "daily".to_string(),
        };
        let json_data = serde_json::to_vec(&pattern_data)?;
        
        let confidence = (count as f32 / 10.0).min(0.95);
        
        store_pattern(conn, "time_of_day", &json_data, confidence)?;
    }
    
    Ok(())
}

fn store_pattern(conn: &Connection, p_type: &str, data: &[u8], confidence: f32) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute(
        "INSERT INTO patterns (pattern_type, pattern_data, confidence, last_observed, occurrence_count, is_active)
         VALUES (?1, ?2, ?3, ?4, ?5, 1)",
        rusqlite::params![
            p_type,
            data,
            confidence,
            chrono::Utc::now().timestamp(),
            1,
        ],
    )?;
    
    Ok(())
}
