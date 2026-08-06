//! App self-update commands backing the admin Status page "Check for updates"
//! button + update modal.
//!
//! Non-Linux builds delegate to `crate::updater` (tauri-plugin-updater).
//! Linux (AUR) builds are stubs: updates are managed by the system package
//! manager, so the commands return a friendly error instead of failing at
//! compile time.

use tauri::AppHandle;

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckInfo {
    pub current_version: String,
    pub latest_version: String,
    pub available: bool,
    pub notes: Option<String>,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

/// Check for an app update without downloading. The frontend shows the result
/// (up to date / update available with notes) in a modal.
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<UpdateCheckInfo, String> {
    #[cfg(not(target_os = "linux"))]
    {
        let current_version = app.package_info().version.to_string();
        match crate::updater::check_update_info(&app).await? {
            Some((latest, notes)) => Ok(UpdateCheckInfo {
                current_version,
                latest_version: latest,
                available: true,
                notes,
            }),
            None => Ok(UpdateCheckInfo {
                current_version: current_version.clone(),
                latest_version: current_version,
                available: false,
                notes: None,
            }),
        }
    }
    #[cfg(target_os = "linux")]
    {
        Err("App updates are managed by the system package manager on this platform.".into())
    }
}

/// Download the pending update (streaming progress back to the frontend),
/// install it, and relaunch the app.
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    on_progress: tauri::ipc::Channel<UpdateProgress>,
) -> Result<(), String> {
    #[cfg(not(target_os = "linux"))]
    {
        crate::updater::download_install_update(app, move |downloaded, total| {
            let _ = on_progress.send(UpdateProgress { downloaded, total });
        })
        .await
    }
    #[cfg(target_os = "linux")]
    {
        Err("App updates are managed by the system package manager on this platform.".into())
    }
}
