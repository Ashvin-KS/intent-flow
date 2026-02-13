use anyhow::Result;
use rusqlite::Connection;
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
             WHERE start_time >= ?1 AND end_time <= ?2 
             ORDER BY start_time DESC 
             {}",
            limit_clause
        )
    )?;

    let activities = stmt.query_map([start_time, end_time], |row| {
        Ok(Activity {
            id: row.get(0)?,
            app_name: row.get(1)?,
            app_hash: row.get(2)?,
            window_title: row.get(3)?,
            window_title_hash: row.get(4)?,
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
         WHERE start_time >= ?1 AND end_time <= ?2",
        [start_time, end_time],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    // Get top apps
    let mut stmt = conn.prepare(
        "SELECT app_name, SUM(duration_seconds) as duration, COUNT(*) as count
         FROM activities 
         WHERE start_time >= ?1 AND end_time <= ?2
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
         WHERE a.start_time >= ?1 AND a.end_time <= ?2
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

pub fn insert_activity(conn: &Connection, activity: &crate::models::ActivityEvent) -> Result<i64> {
    let _metadata = serde_json::to_vec(&activity.metadata)?;
    
    conn.execute(
        "INSERT INTO activities 
         (app_name, app_hash, window_title, window_title_hash, category_id, 
          start_time, end_time, duration_seconds, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        [
            &activity.app_name,
            &activity.app_hash.to_string(),
            &activity.window_title,
            &activity.window_title_hash.to_string(),
            &activity.category_id.to_string(),
            &activity.start_time.to_string(),
            &activity.end_time.to_string(),
            &activity.duration_seconds.to_string(),
        ],
    )?;

    Ok(conn.last_insert_rowid())
}
