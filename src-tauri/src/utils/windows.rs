use winapi::shared::minwindef::{BOOL, LPARAM};
use winapi::shared::windef::HWND;
use winapi::um::winuser::{
    EnumWindows, GetWindowTextW, GetWindowTextLengthW, IsWindowVisible, 
};


/// Get a list of titles for all currently visible windows
pub fn get_open_windows() -> Vec<String> {


    let mut titles = Vec::new();

    unsafe {
        extern "system" fn enum_window_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
            unsafe {
                let titles = &mut *(lparam as *mut Vec<String>);
                
                if IsWindowVisible(hwnd) != 0 {
                    let len = GetWindowTextLengthW(hwnd);
                    if len > 0 {
                        let mut buf = vec![0u16; (len + 1) as usize];
                        let copied = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
                        if copied > 0 {
                            let title = String::from_utf16_lossy(&buf[..copied as usize]);
                            if !title.trim().is_empty() && title != "Program Manager" {
                                titles.push(title);
                            }
                        }
                    }
                }
            }
            1 // Continue enumeration
        }

        EnumWindows(Some(enum_window_callback), &mut titles as *mut _ as LPARAM);
    }
    
    // Sort and deduplicate
    titles.sort();
    titles.dedup();
    
    titles
}

pub fn get_media_info() -> Option<crate::models::activity::MediaInfo> {
    use windows::Media::Control::{GlobalSystemMediaTransportControlsSessionManager, GlobalSystemMediaTransportControlsSessionPlaybackStatus};
    
    // We use .get() which blocks. This function should be called inside spawn_blocking.
    
    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync().ok()?.get().ok()?;
    let session = manager.GetCurrentSession().ok()?;
    
    let info = session.GetPlaybackInfo().ok()?;
    let status = info.PlaybackStatus().ok()?;

    // Only care if playing or paused (ignore closed/stopped)
    let status_str = match status {
        GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing => "Playing",
        GlobalSystemMediaTransportControlsSessionPlaybackStatus::Paused => "Paused",
        _ => return None,
    };

    let props = session.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let title = props.Title().ok()?.to_string();
    let artist = props.Artist().ok()?.to_string();

    if title.is_empty() {
        return None;
    }

    Some(crate::models::activity::MediaInfo {
        title,
        artist,
        status: status_str.to_string(),
    })
}

/// Set Windows Focus Assist (soft-toggle via registry for "Do Not Disturb")
pub fn set_focus_assist(enabled: bool) -> Result<(), String> {
    use std::process::Command;
    
    let path = r#"HKCU\Software\Microsoft\Windows\CurrentVersion\PushNotifications"#;
    let value = if enabled { "0" } else { "1" }; // 0 = DND enabled (toasts disabled), 1 = toasts enabled
    
    // Use reg.exe to change the value
    Command::new("reg")
        .args(["add", path, "/v", "ToastEnabled", "/t", "REG_DWORD", "/d", value, "/f"])
        .spawn()
        .map_err(|e| format!("Failed to toggle Focus Assist registry: {}", e))?;
    
    Ok(())
}

/// Helper to show a system notification from the backend
pub fn show_system_notification(app_handle: &tauri::AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    
    app_handle.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .unwrap_or_else(|e| log::error!("Failed to show notification: {}", e));
}

/// Get a list of recently accessed files from Windows Recent items
pub fn get_recent_files(limit: usize) -> Vec<String> {
    use std::path::PathBuf;
    
    let mut recent_path = PathBuf::from(std::env::var("APPDATA").unwrap_or_default());
    recent_path.push(r#"Microsoft\Windows\Recent"#);
    
    if !recent_path.exists() {
        return Vec::new();
    }
    
    // Skip these system/noise files
    let skip_names: &[&str] = &[
        "desktop.ini",
        "zone.identifier",
        "thumbs.db",
    ];

    let mut files_with_time = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&recent_path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(name) = entry.file_name().into_string() {
                        let lower = name.to_lowercase();

                        // Skip system files and hidden noise
                        if skip_names.iter().any(|s| lower.contains(s)) {
                            continue;
                        }

                        // Remove .lnk extension for display
                        let display_name = name.strip_suffix(".lnk").unwrap_or(&name).to_string();
                        
                        // Skip empty or very short names
                        if display_name.trim().len() < 2 {
                            continue;
                        }

                        files_with_time.push((display_name, modified));
                    }
                }
            }
        }
    }
    
    // Sort by modification time descending (newest first)
    files_with_time.sort_by(|a, b| b.1.cmp(&a.1));
    
    let mut files: Vec<String> = files_with_time.into_iter().map(|(n, _)| n).collect();

    // Deduplicate while preserving order
    let mut seen = std::collections::HashSet::new();
    files.retain(|f| seen.insert(f.clone()));

    files.truncate(limit);
    files
}
