use tauri::{AppHandle, Manager};

#[tauri::command]
pub async fn minimize_to_tray(
    app_handle: AppHandle,
) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window("main") {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn show_window(
    app_handle: AppHandle,
) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window("main") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn quit_app(
    app_handle: AppHandle,
) -> Result<(), String> {
    app_handle.exit(0);
    Ok(())
}
