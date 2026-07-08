use std::path::PathBuf;

use tauri::AppHandle;
use tauri::Manager;

pub use pi_server::PiBroker;
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

/// Ensure the pi binary is available in the bundle resource directory.
///
/// Returns the path to the pi executable.
pub fn ensure_pi_binary(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let target_dir = bundle_pi_dir(app_handle);
    let bin_path = target_dir.join(resolve::pi_binary_name());

    // Fast path: binary already exists at bundle target
    if bin_path.is_file() {
        log::info!("[pi] binary ready at {}", bin_path.display());
        return Ok(bin_path);
    }

    // Delegate to pi_server's resolver (searches known locations + downloads)
    let resolved = resolve::resolve_pi_binary(&target_dir)?;

    if resolved.is_file() {
        log::info!("[pi] binary ready at {}", resolved.display());
        return Ok(resolved);
    }

    Err(format!(
        "[pi] resolved path does not exist: {}",
        resolved.display()
    ))
}
