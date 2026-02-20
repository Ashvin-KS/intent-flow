use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager};
use walkdir::WalkDir;

const SCAN_INTERVAL_SECS: u64 = 2;
const MAX_SCAN_DEPTH: usize = 8;
const RECENT_CREATE_WINDOW_MS: i64 = 180_000;
const MAX_SNAPSHOT_CHARS: usize = 4000;
const MAX_PREVIEW_CHARS: usize = 500;

pub fn start_file_monitor(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let roots = discover_roots();
        if roots.is_empty() {
            println!("[FileMonitor] No valid roots found. Set INTENTFLOW_CODE_ROOTS to enable monitoring.");
            return;
        }
        println!("[FileMonitor] Active monitoring started (interval={}s, depth={})", SCAN_INTERVAL_SECS, MAX_SCAN_DEPTH);
        for root in &roots {
            println!("[FileMonitor] Watching root: {}", root.to_string_lossy());
        }

        let mut known_mtimes: HashMap<String, i64> = HashMap::new();
        let mut known_dirs: HashSet<String> = HashSet::new();
        let mut known_content: HashMap<String, String> = HashMap::new();
        let mut initialized_roots: HashSet<String> = HashSet::new();

        loop {
            for root in &roots {
                scan_root(
                    &app_handle,
                    root,
                    &mut known_mtimes,
                    &mut known_dirs,
                    &mut known_content,
                    &mut initialized_roots,
                );
            }
            tokio::time::sleep(Duration::from_secs(SCAN_INTERVAL_SECS)).await;
        }
    });
}

fn discover_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();

    if let Ok(raw) = std::env::var("INTENTFLOW_CODE_ROOTS") {
        for part in raw.split(';').flat_map(|p| p.split(',')) {
            let trimmed = part.trim();
            if !trimmed.is_empty() {
                roots.push(PathBuf::from(trimmed));
            }
        }
    }

    if roots.is_empty() {
        if let Ok(profile) = std::env::var("USERPROFILE") {
            roots.push(PathBuf::from(&profile).join("Developer").join("Code"));
            roots.push(PathBuf::from(profile).join("Documents").join("GitHub"));
        }
        if let Ok(cwd) = std::env::current_dir() {
            roots.push(cwd);
        }
    }

    let mut unique: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for p in roots.into_iter().filter(|p| p.exists() && p.is_dir()) {
        let key = p.to_string_lossy().to_string();
        if seen.insert(key) {
            unique.push(p);
        }
    }
    unique
}

fn scan_root(
    app_handle: &AppHandle,
    root: &Path,
    known_mtimes: &mut HashMap<String, i64>,
    known_dirs: &mut HashSet<String>,
    known_content: &mut HashMap<String, String>,
    initialized_roots: &mut HashSet<String>,
) {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let now = chrono::Utc::now().timestamp();
    let mut seen_in_scan: HashSet<String> = HashSet::new();
    let mut seen_dirs_in_scan: HashSet<String> = HashSet::new();
    let root_string = root.to_string_lossy().to_string();
    let initialized = initialized_roots.contains(&root_string);

    for entry in WalkDir::new(root)
        .max_depth(MAX_SCAN_DEPTH)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if entry.file_type().is_dir() {
            let dir_str = path.to_string_lossy().to_string();
            seen_dirs_in_scan.insert(dir_str.clone());
            if initialized && !known_dirs.contains(&dir_str) {
                let _ = insert_event(
                    app_handle,
                    &dir_str,
                    &root_string,
                    "folder",
                    "created",
                    None,
                    now,
                );
            }
            continue;
        }
        if !entry.file_type().is_file() || !is_code_file(path) {
            continue;
        }

        let Some(mtime) = modified_unix_millis(path) else { continue };
        let path_str = path.to_string_lossy().to_string();
        seen_in_scan.insert(path_str.clone());

        match known_mtimes.get(&path_str).copied() {
            None => {
                known_mtimes.insert(path_str.clone(), mtime);
                if let Some(content) = read_file_snapshot(path) {
                    known_content.insert(path_str.clone(), content);
                }
                if now_ms - mtime <= RECENT_CREATE_WINDOW_MS {
                    let created_preview = read_file_snapshot(path)
                        .map(|content| format!("Initial content:\n{}", truncate_chars(&content, MAX_PREVIEW_CHARS)));
                    let _ = insert_event(
                        app_handle,
                        &path_str,
                        &root_string,
                        "file",
                        "created",
                        created_preview.as_deref(),
                        now,
                    );
                }
            }
            Some(prev) if mtime > prev => {
                known_mtimes.insert(path_str.clone(), mtime);
                let previous = known_content.get(&path_str).cloned();
                let current = read_file_snapshot(path);
                let preview = build_change_preview(previous.as_deref(), current.as_deref());
                if let Some(content) = current {
                    known_content.insert(path_str.clone(), content);
                }
                let _ = insert_event(
                    app_handle,
                    &path_str,
                    &root_string,
                    "file",
                    "modified",
                    preview.as_deref(),
                    now,
                );
            }
            _ => {}
        }
    }

    let deleted_dirs: Vec<String> = known_dirs
        .iter()
        .filter(|p| p.starts_with(&root_string) && !seen_dirs_in_scan.contains(*p))
        .cloned()
        .collect();
    for path in deleted_dirs {
        known_dirs.remove(&path);
        if initialized {
            let _ = insert_event(
                app_handle,
                &path,
                &root_string,
                "folder",
                "deleted",
                None,
                now,
            );
        }
    }

    known_dirs.extend(seen_dirs_in_scan.into_iter());

    let deleted: Vec<String> = known_mtimes
        .keys()
        .filter(|p| p.starts_with(&root_string) && !seen_in_scan.contains(*p))
        .cloned()
        .collect();
    for path in deleted {
        known_mtimes.remove(&path);
        known_content.remove(&path);
        let _ = insert_event(
            app_handle,
            &path,
            &root_string,
            "file",
            "deleted",
            None,
            now,
        );
    }

    initialized_roots.insert(root_string);
}

fn insert_event(
    app_handle: &AppHandle,
    path: &str,
    project_root: &str,
    entity_type: &str,
    change_type: &str,
    content_preview: Option<&str>,
    detected_at: i64,
) -> Result<(), String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("intentflow.db");
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO code_file_events (path, project_root, entity_type, change_type, content_preview, detected_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![path, project_root, entity_type, change_type, content_preview, detected_at],
    )
    .map_err(|e| e.to_string())?;
    println!(
        "[FileMonitor] {} {} | {}{}",
        entity_type,
        change_type,
        path,
        content_preview.map(|p| format!(" | {}", p.replace('\n', " "))).unwrap_or_default()
    );
    Ok(())
}

fn is_code_file(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    matches!(
        ext.as_deref(),
        Some("rs")
            | Some("ts")
            | Some("tsx")
            | Some("js")
            | Some("jsx")
            | Some("py")
            | Some("go")
            | Some("java")
            | Some("cpp")
            | Some("c")
            | Some("h")
            | Some("hpp")
            | Some("cs")
            | Some("json")
            | Some("toml")
            | Some("yaml")
            | Some("yml")
            | Some("md")
            | Some("sql")
    )
}

fn modified_unix_millis(path: &Path) -> Option<i64> {
    let modified: SystemTime = path.metadata().ok()?.modified().ok()?;
    let duration = modified.duration_since(UNIX_EPOCH).ok()?;
    Some(duration.as_millis() as i64)
}

fn read_file_snapshot(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    Some(truncate_chars(&content, MAX_SNAPSHOT_CHARS))
}

fn build_change_preview(previous: Option<&str>, current: Option<&str>) -> Option<String> {
    let prev = previous.unwrap_or("");
    let curr = current.unwrap_or("");
    if prev == curr {
        return None;
    }

    let prev_lines: Vec<&str> = prev.lines().collect();
    let curr_lines: Vec<&str> = curr.lines().collect();
    let max = prev_lines.len().max(curr_lines.len());

    for idx in 0..max {
        let old_line = prev_lines.get(idx).copied().unwrap_or("");
        let new_line = curr_lines.get(idx).copied().unwrap_or("");
        if old_line != new_line {
            let summary = format!(
                "Line {} changed:\n- {}\n+ {}",
                idx + 1,
                truncate_chars(old_line, MAX_PREVIEW_CHARS / 2),
                truncate_chars(new_line, MAX_PREVIEW_CHARS / 2),
            );
            return Some(truncate_chars(&summary, MAX_PREVIEW_CHARS));
        }
    }

    Some("Content changed".to_string())
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut count = 0usize;
    for ch in input.chars() {
        if count >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
        count += 1;
    }
    out
}
