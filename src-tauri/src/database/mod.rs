use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

pub mod schema;
pub mod queries;

pub fn init_database(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    
    // Enable WAL mode
    conn.pragma_update(None, "journal_mode", &"WAL")?;
    conn.pragma_update(None, "synchronous", &"NORMAL")?;
    conn.pragma_update(None, "foreign_keys", &"ON")?;
    
    // Create schema
    schema::create_tables(&conn)?;
    
    Ok(conn)
}
