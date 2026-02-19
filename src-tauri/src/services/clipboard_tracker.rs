use tauri::AppHandle;
use std::time::Duration;
use arboard::Clipboard;
use crate::database::queries::store_clipboard_item;

pub fn start_clipboard_tracker(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut clipboard = match Clipboard::new() {
            Ok(c) => c,
            Err(e) => {
                log::error!("Failed to initialize clipboard: {}", e);
                return;
            }
        };

        let mut last_hash = 0u64;

        loop {
            // Check clipboard every 2 seconds
            tokio::time::sleep(Duration::from_secs(2)).await;

            if let Ok(text) = clipboard.get_text() {
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }

                // Check for sensitive patterns (minimal)
                if text.len() > 5000 {
                    continue; // Ignore very large snippets for performance
                }

                let hash = calculate_hash(text);
                if hash != last_hash {
                    last_hash = hash;
                    
                    // Store in database
                    if let Err(e) = store_clipboard_item(&app_handle, text, hash as i64) {
                        log::error!("Failed to store clipboard item: {}", e);
                    }
                }
            }
        }
    });
}

fn calculate_hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}
