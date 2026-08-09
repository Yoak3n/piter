use tauri::{AppHandle, Manager, ipc::Channel};

use crate::base::state::GatewaySlot;
use crate::pi_runtime;

/// Get info about the current pi installation.
#[tauri::command]
pub fn get_pi_install_info(app: AppHandle) -> pi_runtime::PiInstallInfo {
    pi_runtime::get_pi_install_info(&app)
}

/// Download a specific pi version and install it into resources/pi/.
///
/// Streams download/extract/install progress through the `on_progress` channel.
#[tauri::command]
pub async fn download_pi_version(
    app: AppHandle,
    version: String,
    on_progress: Channel<pi_server::DownloadProgress>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        pi_runtime::download_and_install(&app, &version, move |progress| {
            let _ = on_progress.send(progress);
        })
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Uninstall pi from resources/pi/.
#[tauri::command]
pub fn uninstall_pi(app: AppHandle) -> Result<String, String> {
    let origin = pi_runtime::uninstall_pi(&app)?;
    // The gateway now points at a deleted binary — stop it so a later
    // download can start a fresh gateway.
    if let Some(slot) = app.try_state::<GatewaySlot>() {
        if let Some(gw) = slot.lock().take() {
            log::info!("[pi] stopping gateway after uninstall");
            gw.kill_all();
        }
    }
    Ok(format!("Pi uninstalled (was {:?})", origin))
}
