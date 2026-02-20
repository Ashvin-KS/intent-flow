use tauri::AppHandle;

use crate::models::DashboardOverview;

#[tauri::command]
pub async fn get_dashboard_overview(
    app_handle: AppHandle,
    refresh: Option<bool>,
) -> Result<DashboardOverview, String> {
    if refresh.unwrap_or(false) {
        return crate::services::dashboard_engine::refresh_dashboard_snapshot(&app_handle).await;
    }

    if let Some(snapshot) = crate::services::dashboard_engine::get_dashboard_snapshot(&app_handle)? {
        return Ok(snapshot);
    }

    crate::services::dashboard_engine::refresh_dashboard_snapshot(&app_handle).await
}

#[tauri::command]
pub async fn refresh_dashboard_overview(
    app_handle: AppHandle,
) -> Result<DashboardOverview, String> {
    crate::services::dashboard_engine::refresh_dashboard_snapshot(&app_handle).await
}
