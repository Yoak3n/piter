use std::path::PathBuf;

use serde::Serialize;
use tauri::AppHandle;
use tauri::Manager;

pub use pi_server::GatewayState;
use pi_server::resolve;

// ─── Public API ──────────────────────────────────────────────────────────────

/// Return the locked pi version string.
pub fn locked_pi_version() -> &'static str {
    resolve::locked_pi_version()
}

/// Return the path to the `resources/pi/` directory inside the Tauri bundle.
fn bundle_pi_dir(app_handle: &AppHandle) -> PathBuf {
    if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("pi")
    } else {
        app_handle
            .path()
            .resource_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("pi")
    }
}

/// Origin of the pi installation in resources/pi/.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PiOrigin {
    /// Downloaded by piter from GitHub releases.
    Downloaded,
    /// Linked or copied from an external (pre-existing) installation.
    Linked,
    /// Not currently installed.
    Missing,
}

/// Information about the current pi installation in resources/pi/.
#[derive(Debug, Clone, Serialize)]
pub struct PiInstallInfo {
    /// The installed version (read from .version marker), if available.
    pub version: Option<String>,
    /// How pi was installed.
    pub origin: PiOrigin,
    /// Whether the pi binary actually exists.
    pub binary_present: bool,
    /// The locked version from pi-version.json.
    pub locked_version: String,
}

/// Get information about the current pi installation.
pub fn get_pi_install_info(app_handle: &AppHandle) -> PiInstallInfo {
    let target_dir = bundle_pi_dir(app_handle);
    let bin_path = target_dir.join(resolve::pi_binary_name());
    let binary_present = bin_path.is_file();

    let version = std::fs::read_to_string(target_dir.join(".version"))
        .ok()
        .map(|s| s.trim().to_string());

    let origin = if !binary_present {
        PiOrigin::Missing
    } else {
        match std::fs::read_to_string(target_dir.join(".origin")) {
            Ok(s) => {
                let s = s.trim();
                if s == "downloaded" {
                    PiOrigin::Downloaded
                } else {
                    PiOrigin::Linked
                }
            }
            // No .origin marker → came from resolve_pi_binary_local (linked/copied from external)
            Err(_) => PiOrigin::Linked,
        }
    };

    PiInstallInfo {
        version,
        origin,
        binary_present,
        locked_version: locked_pi_version().to_string(),
    }
}

/// Check whether the pi binary is available in resources/pi/.
pub fn is_pi_binary_available(app_handle: &AppHandle) -> bool {
    let target_dir = bundle_pi_dir(app_handle);
    target_dir.join(resolve::pi_binary_name()).is_file()
}

/// Try to resolve pi from local sources only (existing install + known locations).
/// Does NOT download from GitHub.
pub fn try_resolve_pi_binary(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let target_dir = bundle_pi_dir(app_handle);
    resolve::resolve_pi_binary_local(&target_dir)
}

/// Download a specific pi version and install it into resources/pi/.
///
/// - If resources/pi/ already contains a piter-downloaded install, it is replaced.
/// - If resources/pi/ contains a linked install, the link is removed and replaced
///   with a fresh download.
/// - After download, writes `.version` and `.origin` markers.
///
/// `on_progress` is invoked with download/extract/install progress events.
pub fn download_and_install(
    app_handle: &AppHandle,
    version: &str,
    on_progress: impl Fn(pi_server::DownloadProgress) + Send + 'static,
) -> Result<(), String> {
    let target_dir = bundle_pi_dir(app_handle);

    // Clear existing contents
    if target_dir.exists() {
        std::fs::remove_dir_all(&target_dir)
            .map_err(|e| format!("Failed to clear pi directory: {}", e))?;
    }
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("Failed to create pi directory: {}", e))?;

    // Download to the target directory directly
    resolve::download_pi_with_progress(version, &target_dir, on_progress)?;

    // Write origin marker
    std::fs::write(target_dir.join(".origin"), "downloaded")
        .map_err(|e| format!("Write .origin: {}", e))?;

    log::info!("[pi] downloaded and installed version {}", version);
    Ok(())
}

/// Uninstall pi from resources/pi/.
///
/// - If downloaded by piter: removes the entire directory.
/// - If linked from external: removes the link/copy (NOT the original external install).
/// - If missing: returns an error.
pub fn uninstall_pi(app_handle: &AppHandle) -> Result<PiOrigin, String> {
    let target_dir = bundle_pi_dir(app_handle);
    let bin_path = target_dir.join(resolve::pi_binary_name());

    if !bin_path.is_file() {
        return Err("Pi is not installed in resources/pi/".into());
    }

    // Determine origin before removing
    let origin = match std::fs::read_to_string(target_dir.join(".origin")) {
        Ok(s) => {
            if s.trim() == "downloaded" {
                PiOrigin::Downloaded
            } else {
                PiOrigin::Linked
            }
        }
        Err(_) => PiOrigin::Linked,
    };

    // Remove resources/pi/ entirely (safe for both symlink and real copy)
    std::fs::remove_dir_all(&target_dir)
        .map_err(|e| format!("Failed to remove pi directory: {}", e))?;

    log::info!("[pi] uninstalled pi (origin: {:?})", origin);
    Ok(origin)
}
