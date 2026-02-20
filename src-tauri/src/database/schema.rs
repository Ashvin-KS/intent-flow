use anyhow::Result;
use rusqlite::Connection;

pub fn create_tables(conn: &Connection) -> Result<()> {
    // Categories table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS categories (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            icon TEXT NOT NULL,
            color TEXT NOT NULL,
            keywords TEXT,
            apps TEXT,
            created_at INTEGER NOT NULL
        )",
        [],
    )?;

    // Activities table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS activities (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            app_name TEXT NOT NULL,
            app_hash INTEGER NOT NULL,
            window_title TEXT,
            window_title_hash INTEGER,
            category_id INTEGER NOT NULL,
            start_time INTEGER NOT NULL,
            end_time INTEGER NOT NULL,
            duration_seconds INTEGER NOT NULL,
            metadata BLOB,
            FOREIGN KEY (category_id) REFERENCES categories(id)
        )",
        [],
    )?;

    // Create indexes for activities
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_activities_start_time ON activities(start_time)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_activities_app_hash ON activities(app_hash)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_activities_category_id ON activities(category_id)",
        [],
    )?;

    // Activity summaries
    conn.execute(
        "CREATE TABLE IF NOT EXISTS activity_summaries (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            date INTEGER NOT NULL,
            hour INTEGER,
            category_id INTEGER NOT NULL,
            total_duration INTEGER NOT NULL,
            event_count INTEGER NOT NULL,
            top_apps BLOB,
            top_titles BLOB,
            FOREIGN KEY (category_id) REFERENCES categories(id),
            UNIQUE(date, hour, category_id)
        )",
        [],
    )?;

    // Manual entries table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS manual_entries (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            entry_type TEXT NOT NULL,
            title TEXT NOT NULL,
            content TEXT,
            tags BLOB,
            status TEXT DEFAULT 'active',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            completed_at INTEGER
        )",
        [],
    )?;

    // Patterns table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS patterns (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            pattern_type TEXT NOT NULL,
            pattern_data BLOB NOT NULL,
            confidence REAL NOT NULL,
            last_observed INTEGER NOT NULL,
            occurrence_count INTEGER NOT NULL,
            is_active INTEGER DEFAULT 1
        )",
        [],
    )?;

    // Intent logs table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS intent_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_input TEXT NOT NULL,
            detected_intent TEXT NOT NULL,
            confidence REAL NOT NULL,
            actions_taken BLOB,
            timestamp INTEGER NOT NULL
        )",
        [],
    )?;

    // Workflows table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS workflows (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            uuid TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            description TEXT,
            icon TEXT,
            apps BLOB,
            files BLOB,
            urls BLOB,
            use_count INTEGER DEFAULT 0,
            last_used INTEGER,
            created_at INTEGER NOT NULL
        )",
        [],
    )?;

    // Workflow suggestions table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS workflow_suggestions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            workflow_id INTEGER NOT NULL,
            trigger_type TEXT NOT NULL,
            trigger_conditions BLOB NOT NULL,
            relevance_score REAL NOT NULL,
            suggested_count INTEGER DEFAULT 0,
            accepted_count INTEGER DEFAULT 0,
            last_suggested INTEGER,
            FOREIGN KEY (workflow_id) REFERENCES workflows(id)
        )",
        [],
    )?;

    // Query cache table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS query_cache (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            query_hash TEXT NOT NULL UNIQUE,
            query_text TEXT NOT NULL,
            result BLOB NOT NULL,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL
        )",
        [],
    )?;

    // Settings table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value BLOB NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    )?;

    // App registry table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS app_registry (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            app_name TEXT NOT NULL UNIQUE,
            app_hash INTEGER NOT NULL UNIQUE,
            display_name TEXT,
            icon_path TEXT,
            category_id INTEGER,
            first_seen INTEGER NOT NULL,
            last_seen INTEGER NOT NULL,
            usage_count INTEGER DEFAULT 0,
            total_duration INTEGER DEFAULT 0,
            FOREIGN KEY (category_id) REFERENCES categories(id)
        )",
        [],
    )?;

    // Chat sessions table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS chat_sessions (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    )?;

    // Chat messages table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS chat_messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            tool_calls TEXT,
            agent_steps TEXT,
            activities TEXT,
            created_at INTEGER NOT NULL,
            FOREIGN KEY (session_id) REFERENCES chat_sessions(id) ON DELETE CASCADE
        )",
        [],
    )?;

    // AI model usage (recently used models)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS ai_model_usage (
            model_id TEXT PRIMARY KEY,
            model_name TEXT NOT NULL,
            use_count INTEGER NOT NULL DEFAULT 0,
            last_used INTEGER NOT NULL
        )",
        [],
    )?;

    // Code file change events captured from monitored project roots.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS code_file_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT NOT NULL,
            project_root TEXT NOT NULL,
            entity_type TEXT NOT NULL DEFAULT 'file',
            change_type TEXT NOT NULL,
            content_preview TEXT,
            detected_at INTEGER NOT NULL
        )",
        [],
    )?;
    ensure_column_exists(
        conn,
        "code_file_events",
        "entity_type",
        "TEXT NOT NULL DEFAULT 'file'",
    )?;
    ensure_column_exists(conn, "code_file_events", "content_preview", "TEXT")?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_code_file_events_detected_at ON code_file_events(detected_at)",
        [],
    )?;

    // Daily dashboard snapshot generated from current-day context.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS dashboard_snapshots (
            date_key TEXT PRIMARY KEY,
            summary_json TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    )?;

    // Insert default categories if they don't exist
    insert_default_categories(conn)?;

    Ok(())
}

fn ensure_column_exists(conn: &Connection, table: &str, column: &str, definition: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
    let columns: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .collect();
    if columns.iter().any(|c| c == column) {
        return Ok(());
    }
    conn.execute(
        &format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, definition),
        [],
    )?;
    Ok(())
}

fn insert_default_categories(conn: &Connection) -> Result<()> {
    let categories = crate::models::category::get_default_categories();
    let now = chrono::Utc::now().timestamp();

    for category in categories {
        let keywords = serde_json::to_string(&category.keywords)?;
        let apps = serde_json::to_string(&category.apps)?;

        conn.execute(
            "INSERT OR IGNORE INTO categories (id, name, icon, color, keywords, apps, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            [
                &category.id.to_string(),
                &category.name,
                &category.icon,
                &category.color,
                &keywords,
                &apps,
                &now.to_string(),
            ],
        )?;
    }

    Ok(())
}
