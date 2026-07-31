use tauri::AppHandle;

use crate::pi;

/// Get info about the current pi installation.
#[tauri::command]
pub fn get_pi_install_info(app: AppHandle) -> pi::PiInstallInfo {
    pi::get_pi_install_info(&app)
}

/// Download a specific pi version and install it into resources/pi/.
#[tauri::command]
pub async fn download_pi_version(app: AppHandle, version: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        pi::download_and_install(&app, &version)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Uninstall pi from resources/pi/.
#[tauri::command]
pub fn uninstall_pi(app: AppHandle) -> Result<String, String> {
    let origin = pi::uninstall_pi(&app)?;
    Ok(format!("Pi uninstalled (was {:?})", origin))
}
