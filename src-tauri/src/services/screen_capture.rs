use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use std::path::PathBuf;
use tauri::AppHandle;
use image::{DynamicImage, GrayImage, ImageBuffer, Rgba, RgbaImage};

// ─── Shared state ───
// The screen capture service writes OCR text here,
// and the activity tracker reads it when storing activities.

static CAPTURE_ENABLED: AtomicBool = AtomicBool::new(true);
const MAX_OCR_CHARS: usize = 2000;
const MIN_OCR_QUALITY_SCORE: f64 = 0.28;

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
        let mut last_image: Option<RgbaImage> = None;
        
        loop {
            if CAPTURE_ENABLED.load(Ordering::Relaxed) {
                capture_count += 1;
                let count = capture_count;
                
                // clone last_image for the blocking task
                let prev_img = last_image.clone();
                
                // Run capture + OCR in a blocking task so it doesn't block the async runtime
                let result = tokio::task::spawn_blocking(move || {
                    capture_and_ocr_pipeline(count, prev_img)
                }).await;

                match result {
                    Ok(Ok((text, new_img))) => {
                        // Update last image for diffing next time
                        if let Some(img) = new_img {
                             last_image = Some(img);
                        }
                        
                        // Store text if we got some
                        if let Some(extracted_text) = text {
                            if !extracted_text.trim().is_empty() {
                                // Truncate to avoid bloating metadata payloads.
                                let truncated = truncate_at_char_boundary(&extracted_text, MAX_OCR_CHARS);
                                if let Ok(mut store) = screen_text_store().lock() {
                                    *store = truncated;
                                }
                            }
                        }
                    },
                    Ok(Err(e)) => {
                        println!("[OCR] ❌ Pipeline error: {}", e);
                    },
                    Err(e) => {
                        println!("[OCR] ❌ spawn_blocking task failed: {:?}", e);
                    }
                }
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

// ─── Capture Pipeline ───

fn capture_and_ocr_pipeline(count: u32, prev_image: Option<RgbaImage>) -> Result<(Option<String>, Option<RgbaImage>), String> {
    println!("\n[OCR] ── Capture #{} ──────────────────────", count);
    let start = Instant::now();

    // Try to get specific active window first
    let active_window_info = active_win_pos_rs::get_active_window().ok();
    
    // Capture screenshot - use xcap types directly, convert later
    let screenshot_bytes: Vec<u8>;
    let screenshot_width: u32;
    let screenshot_height: u32;
    
    if let Some(ref info) = active_window_info {
        let windows = xcap::Window::all().map_err(|e| format!("Xcap window list: {}", e))?;
        
        let target_window = windows.into_iter().find(|w| {
            w.title().contains(&info.title) || w.app_name().contains(&info.app_name)
        });

        if let Some(window) = target_window {
            let img = window.capture_image().map_err(|e| format!("Window capture: {}", e))?;
            screenshot_width = img.width();
            screenshot_height = img.height();
            screenshot_bytes = img.into_raw();
        } else {
            let img = capture_primary_monitor_raw()?;
            screenshot_width = img.0;
            screenshot_height = img.1;
            screenshot_bytes = img.2;
        }
    } else {
        let img = capture_primary_monitor_raw()?;
        screenshot_width = img.0;
        screenshot_height = img.1;
        screenshot_bytes = img.2;
    };
    
    // Convert to our image crate's RgbaImage
    let screenshot: RgbaImage = RgbaImage::from_raw(screenshot_width, screenshot_height, screenshot_bytes)
        .ok_or("Failed to create RgbaImage from screenshot")?;
    
    // Resize if too huge (to speed up OCR and diffing)
    let (w, h) = screenshot.dimensions();
    let (new_w, new_h) = if w > 1920 || h > 1080 {
        let scale = (1920.0 / w as f64).min(1080.0 / h as f64);
        ((w as f64 * scale) as u32, (h as f64 * scale) as u32)
    } else {
        (w, h)
    };

    let processed_image: RgbaImage = if new_w != w || new_h != h {
         image::imageops::resize(&screenshot, new_w, new_h, image::imageops::FilterType::Triangle)
    } else {
        screenshot
    };

    // 2. Diffing
    if let Some(ref prev) = prev_image {
        if is_visually_similar(prev, &processed_image) {
            println!("[OCR] ⏭️ Screen unchanged, skipping OCR");
            return Ok((None, Some(processed_image)));
        }
    }

    // 3. OCR via temp file (Windows OCR works most reliably with StorageFile)
    println!("[OCR] 🔍 Running Windows OCR...");
    let ocr_start = Instant::now();
    
    let text = run_ocr_with_variants(&processed_image)?;
    
    let elapsed = start.elapsed();
    println!("[OCR] ✅ OCR completed in {:.1}s (OCR part: {}ms). Found {} chars.", 
        elapsed.as_secs_f64(), ocr_start.elapsed().as_millis(), text.len());

    Ok((Some(text), Some(processed_image)))
}

/// Capture primary monitor and return (width, height, raw_bytes)
fn capture_primary_monitor_raw() -> Result<(u32, u32, Vec<u8>), String> {
    let monitors = xcap::Monitor::all().map_err(|e| format!("Monitor list: {}", e))?;
    let primary = monitors.into_iter()
        .find(|m| m.is_primary())
        .or_else(|| xcap::Monitor::all().ok()?.into_iter().next())
        .ok_or("No monitor found")?;
    
    let img = primary.capture_image().map_err(|e| format!("Monitor capture: {}", e))?;
    let w = img.width();
    let h = img.height();
    Ok((w, h, img.into_raw()))
}

fn is_visually_similar(img1: &RgbaImage, img2: &RgbaImage) -> bool {
    if img1.dimensions() != img2.dimensions() {
        return false;
    }
    
    let (w, h) = img1.dimensions();
    let sample_step = 20;
    let mut diff_accum: u64 = 0;
    let mut count: u64 = 0;
    
    for y in (0..h).step_by(sample_step) {
        for x in (0..w).step_by(sample_step) {
            let p1 = img1.get_pixel(x, y);
            let p2 = img2.get_pixel(x, y);
            
            let r_diff = (p1[0] as i32 - p2[0] as i32).abs();
            let g_diff = (p1[1] as i32 - p2[1] as i32).abs();
            let b_diff = (p1[2] as i32 - p2[2] as i32).abs();
            
            diff_accum += (r_diff + g_diff + b_diff) as u64;
            count += 1;
        }
    }
    
    if count == 0 { return true; }
    
    let avg_diff = diff_accum as f64 / count as f64;
    avg_diff < 15.0
}

fn run_ocr_with_variants(img: &RgbaImage) -> Result<String, String> {
    // Two OCR variants: original + high-contrast binarized image.
    let variants: [(&str, RgbaImage); 2] = [
        ("original", img.clone()),
        ("contrast", preprocess_for_text(img)),
    ];

    let mut best_text = String::new();
    let mut best_score = f64::MIN;

    for (name, variant) in variants {
        let temp_path = std::env::temp_dir().join(format!("intentflow_ocr_{}.png", name));
        variant.save(&temp_path).map_err(|e| format!("Save temp {}: {}", name, e))?;
        let raw_text = run_windows_ocr(&temp_path)?;
        let _ = std::fs::remove_file(&temp_path);

        let cleaned = clean_ocr_text(&raw_text);
        let score = score_ocr_text(&cleaned);
        println!(
            "[OCR] variant={} raw_chars={} cleaned_chars={} score={:.3}",
            name,
            raw_text.len(),
            cleaned.len(),
            score
        );

        if score > best_score {
            best_score = score;
            best_text = cleaned;
        }
    }

    if best_score < MIN_OCR_QUALITY_SCORE {
        println!(
            "[OCR] ⚠️ Low-quality OCR skipped (score={:.3}, threshold={:.3})",
            best_score, MIN_OCR_QUALITY_SCORE
        );
        return Ok(String::new());
    }

    Ok(best_text)
}

fn preprocess_for_text(img: &RgbaImage) -> RgbaImage {
    // Convert to grayscale then apply a simple adaptive threshold.
    let gray: GrayImage = DynamicImage::ImageRgba8(img.clone()).grayscale().to_luma8();
    let (w, h) = gray.dimensions();
    let mean = {
        let mut sum: u64 = 0;
        for p in gray.pixels() {
            sum += p[0] as u64;
        }
        (sum / (w as u64 * h as u64)).clamp(70, 190) as u8
    };
    let threshold = mean.saturating_sub(8);

    ImageBuffer::from_fn(w, h, |x, y| {
        let v = gray.get_pixel(x, y)[0];
        let bin = if v > threshold { 255 } else { 0 };
        Rgba([bin, bin, bin, 255])
    })
}

fn truncate_at_char_boundary(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

fn clean_ocr_text(text: &str) -> String {
    let mut out_lines = Vec::new();
    for raw_line in text.lines() {
        let line = normalize_whitespace(raw_line);
        if line.len() < 2 {
            continue;
        }
        if is_gibberish_line(&line) {
            continue;
        }
        out_lines.push(line);
    }

    normalize_whitespace(&out_lines.join("\n"))
}

fn score_ocr_text(text: &str) -> f64 {
    if text.is_empty() {
        return 0.0;
    }
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len() as f64;
    let alpha = chars.iter().filter(|c| c.is_alphabetic()).count() as f64 / len;
    let digits = chars.iter().filter(|c| c.is_ascii_digit()).count() as f64 / len;
    let spaces = chars.iter().filter(|c| c.is_whitespace()).count() as f64 / len;
    let symbols = chars
        .iter()
        .filter(|c| !c.is_alphanumeric() && !c.is_whitespace() && !",.;:!?()[]{}'\"/@#&+-_".contains(**c))
        .count() as f64
        / len;
    let words = text.split_whitespace().count() as f64;
    let replacement = text.matches('�').count() as f64 / len;

    // Weighted score tuned for chat/code/web text.
    let mut score = 0.0;
    score += (alpha * 1.2).min(0.65);
    score += (spaces * 0.9).min(0.18);
    score += ((words / 20.0) * 0.25).min(0.25);
    score += (digits * 0.3).min(0.08);
    score -= (symbols * 1.2).min(0.5);
    score -= (replacement * 2.0).min(0.6);
    score.clamp(0.0, 1.0)
}

fn is_gibberish_line(line: &str) -> bool {
    if line.len() >= 80 {
        return false;
    }
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return true;
    }
    let total = chars.len() as f64;
    let weird = chars
        .iter()
        .filter(|c| !c.is_alphanumeric() && !c.is_whitespace() && !",.;:!?()[]{}'\"/@#&+-_".contains(**c))
        .count() as f64
        / total;
    let vowels = chars
        .iter()
        .filter(|c| "aeiouAEIOU".contains(**c))
        .count() as f64;
    let letters = chars.iter().filter(|c| c.is_alphabetic()).count() as f64;
    let vowel_ratio = if letters > 0.0 { vowels / letters } else { 0.0 };
    weird > 0.42 || (letters >= 8.0 && vowel_ratio < 0.08)
}

fn normalize_whitespace(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// ─── Windows OCR via StorageFile ───

fn run_windows_ocr(image_path: &PathBuf) -> Result<String, String> {
    use windows::Graphics::Imaging::BitmapDecoder;
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::{FileAccessMode, StorageFile};
    
    let path_str = image_path.to_string_lossy().to_string();
    let hpath = windows::core::HSTRING::from(&path_str);
    
    let file = StorageFile::GetFileFromPathAsync(&hpath)
        .map_err(|e| format!("GetFile: {}", e))?
        .get()
        .map_err(|e| format!("GetFile await: {}", e))?;
    
    let stream = file.OpenAsync(FileAccessMode::Read)
        .map_err(|e| format!("OpenStream: {}", e))?
        .get()
        .map_err(|e| format!("OpenStream await: {}", e))?;
    
    let decoder = BitmapDecoder::CreateAsync(&stream)
        .map_err(|e| format!("Decoder: {}", e))?
        .get()
        .map_err(|e| format!("Decoder await: {}", e))?;
    
    let bitmap = decoder.GetSoftwareBitmapAsync()
        .map_err(|e| format!("Bitmap: {}", e))?
        .get()
        .map_err(|e| format!("Bitmap await: {}", e))?;
    
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .map_err(|e| format!("OcrEngine: {}", e))?;
    
    let result = engine.RecognizeAsync(&bitmap)
        .map_err(|e| format!("Recognize: {}", e))?
        .get()
        .map_err(|e| format!("Recognize await: {}", e))?;
    
    let text = result.Text()
        .map_err(|e| format!("Text: {}", e))?
        .to_string();
    
    Ok(text)
}
