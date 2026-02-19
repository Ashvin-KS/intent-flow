use anyhow::Result;
use rusqlite::Connection;
use tauri::Manager;
use crate::models::{Activity, ActivityStats, AppStat, CategoryStat};

pub fn get_activities(
    conn: &Connection,
    start_time: i64,
    end_time: i64,
    limit: Option<i32>,
) -> Result<Vec<Activity>> {
    let limit_clause = limit.map(|l| format!("LIMIT {}", l)).unwrap_or_default();
    
    let mut stmt = conn.prepare(
        &format!(
            "SELECT id, app_name, app_hash, window_title, window_title_hash, 
                    category_id, start_time, end_time, duration_seconds, metadata
             FROM activities 
             WHERE end_time >= ?1 AND start_time <= ?2 
             ORDER BY start_time DESC 
             {}",
            limit_clause
        )
    )?;

    let activities = stmt.query_map([start_time, end_time], |row| {
        Ok(Activity {
            id: row.get(0)?,
            app_name: row.get(1)?,
            app_hash: row.get::<_, i64>(2)? as u64,
            window_title: row.get::<_, String>(3).unwrap_or_default(),
            window_title_hash: row.get::<_, i64>(4).unwrap_or(0) as u64,
            category_id: row.get(5)?,
            start_time: row.get(6)?,
            end_time: row.get(7)?,
            duration_seconds: row.get(8)?,
            metadata: row.get::<_, Option<Vec<u8>>>(9)?.map(|b| {
                serde_json::from_slice(&b).unwrap_or_default()
            }),
        })
    })?.collect::<Result<Vec<_>, _>>()?;

    Ok(activities)
}

/// Search activities by title/app keywords at the SQL level.
/// Efficient for broad queries spanning many days (e.g., "all songs I've heard").
pub fn search_activities(
    conn: &Connection,
    start_time: i64,
    end_time: i64,
    title_keywords: &[&str],
    category_ids: &[i32],
    limit: Option<i32>,
) -> Result<Vec<Activity>> {
    let limit_clause = limit.map(|l| format!("LIMIT {}", l)).unwrap_or_default();
    
    // Build WHERE conditions for keywords (OR-ed together)
    let mut conditions = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    
    // Time range is always param 1 and 2
    params.push(Box::new(start_time));
    params.push(Box::new(end_time));
    
    let mut keyword_conditions = Vec::new();
    let mut param_idx = 3;
    
    for kw in title_keywords {
        keyword_conditions.push(format!(
            "(LOWER(window_title) LIKE ?{} OR LOWER(app_name) LIKE ?{})",
            param_idx, param_idx
        ));
        params.push(Box::new(format!("%{}%", kw.to_lowercase())));
        param_idx += 1;
    }
    
    if !category_ids.is_empty() {
        let cat_placeholders: Vec<String> = category_ids.iter().enumerate().map(|(i, _)| {
            format!("?{}", param_idx + i)
        }).collect();
        keyword_conditions.push(format!("category_id IN ({})", cat_placeholders.join(",")));
        for cat in category_ids {
            params.push(Box::new(*cat));
        }
    }
    
    if !keyword_conditions.is_empty() {
        conditions.push(format!("({})", keyword_conditions.join(" OR ")));
    }
    
    let extra_where = if conditions.is_empty() {
        String::new()
    } else {
        format!("AND {}", conditions.join(" AND "))
    };
    
    let sql = format!(
        "SELECT id, app_name, app_hash, window_title, window_title_hash, 
                category_id, start_time, end_time, duration_seconds, metadata
         FROM activities 
         WHERE end_time >= ?1 AND start_time <= ?2 
         {} 
         ORDER BY start_time DESC 
         {}",
        extra_where, limit_clause
    );
    
    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    
    let activities = stmt.query_map(param_refs.as_slice(), |row| {
        Ok(Activity {
            id: row.get(0)?,
            app_name: row.get(1)?,
            app_hash: row.get::<_, i64>(2)? as u64,
            window_title: row.get::<_, String>(3).unwrap_or_default(),
            window_title_hash: row.get::<_, i64>(4).unwrap_or(0) as u64,
            category_id: row.get(5)?,
            start_time: row.get(6)?,
            end_time: row.get(7)?,
            duration_seconds: row.get(8)?,
            metadata: row.get::<_, Option<Vec<u8>>>(9)?.map(|b| {
                serde_json::from_slice(&b).unwrap_or_default()
            }),
        })
    })?.collect::<Result<Vec<_>, _>>()?;

    Ok(activities)
}

pub fn get_activity_stats(
    conn: &Connection,
    start_time: i64,
    end_time: i64,
) -> Result<ActivityStats> {
    // Get total duration and event count
    let total: (i64, i32) = conn.query_row(
        "SELECT COALESCE(SUM(duration_seconds), 0), COUNT(*) 
         FROM activities 
         WHERE end_time >= ?1 AND start_time <= ?2",
        [start_time, end_time],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    // Get top apps
    let mut stmt = conn.prepare(
        "SELECT app_name, SUM(duration_seconds) as duration, COUNT(*) as count
         FROM activities 
         WHERE end_time >= ?1 AND start_time <= ?2
         GROUP BY app_name
         ORDER BY duration DESC
         LIMIT 10"
    )?;

    let top_apps: Vec<AppStat> = stmt.query_map([start_time, end_time], |row| {
        let duration: i64 = row.get(1)?;
        let percentage = if total.0 > 0 {
            (duration as f32 / total.0 as f32) * 100.0
        } else {
            0.0
        };
        Ok(AppStat {
            app_name: row.get(0)?,
            duration,
            count: row.get(2)?,
            percentage,
        })
    })?.collect::<Result<Vec<_>, _>>()?;

    // Get top categories
    let mut stmt = conn.prepare(
        "SELECT c.id, c.name, SUM(a.duration_seconds) as duration, COUNT(*) as count
         FROM activities a
         JOIN categories c ON a.category_id = c.id
         WHERE a.end_time >= ?1 AND a.start_time <= ?2
         GROUP BY c.id
         ORDER BY duration DESC
         LIMIT 10"
    )?;

    let top_categories: Vec<CategoryStat> = stmt.query_map([start_time, end_time], |row| {
        let duration: i64 = row.get(2)?;
        let percentage = if total.0 > 0 {
            (duration as f32 / total.0 as f32) * 100.0
        } else {
            0.0
        };
        Ok(CategoryStat {
            category_id: row.get(0)?,
            category_name: row.get(1)?,
            duration,
            count: row.get(3)?,
            percentage,
        })
    })?.collect::<Result<Vec<_>, _>>()?;

    Ok(ActivityStats {
        total_duration: total.0,
        total_events: total.1,
        top_apps,
        top_categories,
    })
}

#[allow(dead_code)]
pub fn insert_activity(conn: &Connection, activity: &crate::models::ActivityEvent) -> Result<i64> {
    let metadata_blob = serde_json::to_vec(&activity.metadata)?;
    
    conn.execute(
        "INSERT INTO activities 
         (app_name, app_hash, window_title, window_title_hash, category_id, 
          start_time, end_time, duration_seconds, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            &activity.app_name,
            activity.app_hash as i64,
            &activity.window_title,
            activity.window_title_hash as i64,
            activity.category_id,
            activity.start_time,
            activity.end_time,
            activity.duration_seconds,
            &metadata_blob,
        ],
    )?;

    Ok(conn.last_insert_rowid())
}

pub fn store_clipboard_item(app_handle: &tauri::AppHandle, content: &str, hash: i64) -> Result<()> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let db_path = data_dir.join("intentflow.db");
    let conn = Connection::open(&db_path)?;
    
    let now = chrono::Utc::now().timestamp();
    let one_hour_ago = now - 3600;

    // Deduplication within the last hour
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clipboard_history WHERE content_hash = ?1 AND timestamp > ?2",
        rusqlite::params![hash, one_hour_ago],
        |row| row.get(0),
    )?;

    if count > 0 {
        return Ok(());
    }
    
    conn.execute(
        "INSERT INTO clipboard_history (content, content_hash, timestamp)
         VALUES (?1, ?2, ?3)",
        rusqlite::params![content, hash, now],
    )?;
    
    Ok(())
}

pub fn get_clipboard_history(conn: &Connection, limit: usize) -> Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT content, timestamp FROM clipboard_history ORDER BY timestamp DESC LIMIT ?1"
    )?;
    
    let history = stmt.query_map([limit], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?.collect::<Result<Vec<_>, _>>()?;
    
    Ok(history)
}
