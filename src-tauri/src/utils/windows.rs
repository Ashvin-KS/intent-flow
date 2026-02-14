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
