use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use std::path::PathBuf;
use tauri::AppHandle;

// ─── Shared state ───
// The screen capture service writes OCR text here,
// and the activity tracker reads it when storing activities.

static CAPTURE_ENABLED: AtomicBool = AtomicBool::new(true);

fn screen_text_store() -> &'static Mutex<String> {
    static STORE: OnceLock<Mutex<String>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(String::new()))
}

/// Get the latest OCR-extracted screen text.
/// Called by activity_tracker when storing activities.
pub fn get_latest_screen_text() -> Option<String> {
    let text = screen_text_store().lock().ok()?.clone();
    if text.is_empty() { None } else { Some(text) }
}

/// Start the periodic screen capture + OCR service.
/// Runs every ~10 seconds on a background task, non-blocking.
pub fn start_screen_capture(_app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Wait a bit on startup before first capture
        println!("[OCR] ⏳ Screen capture service waiting 15s before first capture...");
        tokio::time::sleep(Duration::from_secs(15)).await;
        
        println!("[OCR] ✅ Screen capture + OCR service started (every 10s)");
        
        let mut capture_count: u32 = 0;
        
        loop {
            if CAPTURE_ENABLED.load(Ordering::Relaxed) {
                capture_count += 1;
                let count = capture_count;
                
                // Run capture + OCR in a blocking task so it doesn't block the async runtime
                tokio::task::spawn_blocking(move || {
                    println!("\n[OCR] ── Capture #{} ──────────────────────", count);
                    let start = Instant::now();
                    
                    match capture_and_ocr() {
                        Ok(text) => {
                            let elapsed = start.elapsed();
                            let char_count = text.len();
                            let word_count = text.split_whitespace().count();
                            
                            println!("[OCR] ✅ OCR completed in {:.1}s", elapsed.as_secs_f64());
                            println!("[OCR] 📊 Extracted: {} chars, {} words", char_count, word_count);
                            
                            if !text.trim().is_empty() {
                                // Show first 300 chars as preview
                                let preview = if text.len() > 300 {
                                    format!("{}...", &text[..300])
                                } else {
                                    text.clone()
                                };
                                println!("[OCR] 📝 Text preview:\n{}", preview);
                                
                                // Truncate to 2000 chars max to avoid bloating the DB
                                let truncated = if text.len() > 2000 {
                                    text[..2000].to_string()
                                } else {
                                    text
                                };
                                if let Ok(mut store) = screen_text_store().lock() {
                                    *store = truncated;
                                    println!("[OCR] 💾 Stored in shared state");
                                }
                            } else {
                                println!("[OCR] ⚠️ OCR returned empty text");
                            }
                        }
                        Err(e) => {
                            println!("[OCR] ❌ Error: {}", e);
                        }
                    }
                }).await.unwrap_or_else(|e| {
                    println!("[OCR] ❌ spawn_blocking task failed: {:?}", e);
                });
            } else {
                println!("[OCR] ⏸️ Capture disabled, skipping");
            }
            
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });
}

pub fn set_capture_enabled(enabled: bool) {
    CAPTURE_ENABLED.store(enabled, Ordering::Relaxed);
    println!("[OCR] Capture enabled: {}", enabled);
}

// ─── Screenshot capture ───

fn capture_and_ocr() -> Result<String, String> {
    // 1. Capture the primary monitor
    println!("[OCR] 📸 Getting monitor list...");
    let monitors = xcap::Monitor::all().map_err(|e| format!("Monitor list: {}", e))?;
    println!("[OCR] Found {} monitor(s)", monitors.len());
    
    let primary = monitors.into_iter()
        .find(|m| m.is_primary())
        .or_else(|| xcap::Monitor::all().ok()?.into_iter().next())
        .ok_or("No monitor found")?;
    
    println!("[OCR] 📸 Capturing primary monitor ({}x{})...", primary.width(), primary.height());
    let capture_start = Instant::now();
    let screenshot = primary.capture_image().map_err(|e| format!("Capture: {}", e))?;
    println!("[OCR] 📸 Screenshot captured in {:.1}ms ({}x{} px)", 
        capture_start.elapsed().as_millis(), screenshot.width(), screenshot.height());
    
    // 2. Save to a temp file (Windows OCR needs a file path)
    let temp_path = std::env::temp_dir().join("intentflow_ocr_temp.png");
    println!("[OCR] 💾 Saving to: {}", temp_path.display());
    let save_start = Instant::now();
    screenshot.save(&temp_path).map_err(|e| format!("Save: {}", e))?;
    println!("[OCR] 💾 Saved in {:.1}ms", save_start.elapsed().as_millis());
    
    // 3. Run Windows OCR on the saved image
    println!("[OCR] 🔍 Running Windows OCR...");
    let ocr_start = Instant::now();
    let text = run_windows_ocr(&temp_path)?;
    println!("[OCR] 🔍 OCR finished in {:.1}ms", ocr_start.elapsed().as_millis());
    
    // 4. Clean up temp file
    let _ = std::fs::remove_file(&temp_path);
    
    Ok(text)
}

// ─── Windows OCR via windows crate ───

fn run_windows_ocr(image_path: &PathBuf) -> Result<String, String> {
    use windows::Graphics::Imaging::BitmapDecoder;
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::{FileAccessMode, StorageFile};
    
    // Convert path to HSTRING
    let path_str = image_path.to_string_lossy().to_string();
    let hpath = windows::core::HSTRING::from(&path_str);
    
    // Open the image file
    println!("[OCR]   → Opening file...");
    let file = StorageFile::GetFileFromPathAsync(&hpath)
        .map_err(|e| format!("GetFile: {}", e))?
        .get()
        .map_err(|e| format!("GetFile await: {}", e))?;
    
    // Open a read stream
    println!("[OCR]   → Opening stream...");
    let stream = file.OpenAsync(FileAccessMode::Read)
        .map_err(|e| format!("OpenStream: {}", e))?
        .get()
        .map_err(|e| format!("OpenStream await: {}", e))?;
    
    // Decode the image to a SoftwareBitmap
    println!("[OCR]   → Decoding bitmap...");
    let decoder = BitmapDecoder::CreateAsync(&stream)
        .map_err(|e| format!("Decoder: {}", e))?
        .get()
        .map_err(|e| format!("Decoder await: {}", e))?;
    
    let bitmap = decoder.GetSoftwareBitmapAsync()
        .map_err(|e| format!("Bitmap: {}", e))?
        .get()
        .map_err(|e| format!("Bitmap await: {}", e))?;
    
    // Create OCR engine (uses system language)
    println!("[OCR]   → Creating OCR engine...");
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .map_err(|e| format!("OcrEngine: {}", e))?;
    
    // Run OCR recognition
    println!("[OCR]   → Recognizing text...");
    let result = engine.RecognizeAsync(&bitmap)
        .map_err(|e| format!("Recognize: {}", e))?
        .get()
        .map_err(|e| format!("Recognize await: {}", e))?;
    
    // Extract recognized text
    let text = result.Text()
        .map_err(|e| format!("Text: {}", e))?
        .to_string();
    
    println!("[OCR]   → Got {} chars of text", text.len());
    
    Ok(text)
}
