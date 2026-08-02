use std::path::Path;
use std::process::{Command, Stdio};

use pi_server::broker::util::{build_augmented_path, configure_child_process_for_windows, strip_verbatim_prefix};
use pi_server::gateway::handlers::pi::restart_pi_instance;
use tauri::Manager;

use crate::base::state::GatewaySlot;

#[tauri::command]
pub fn restart_pi(gw: tauri::State<'_, GatewaySlot>) -> Result<String, String> {
    match gw.inner().lock().as_ref() {
        Some(gw) => {
            // Collect all instance IDs first (to avoid holding lock during restart)
            let instance_ids: Vec<String> = {
                let instances = gw.inner.instances.lock();
                instances.keys().cloned().collect()
            };

            if instance_ids.is_empty() {
                return Ok("No active pi processes to restart".into());
            }

            log::info!("[admin] restarting {} pi processes", instance_ids.len());
            let mut restarted = 0;
            for id in &instance_ids {
                match restart_pi_instance(gw, id) {
                    Ok(_) => restarted += 1,
                    Err(e) => log::warn!("[admin] failed to restart instance {}: {}", id, e),
                }
            }
            Ok(format!("Restarted {}/{} pi processes", restarted, instance_ids.len()))
        }
        None => Err("Pi binary not available. Download it from Settings > Versions.".into()),
    }
}

#[tauri::command]
pub fn stop_pi(gw: tauri::State<'_, GatewaySlot>) -> Result<String, String> {
    match gw.inner().lock().as_ref() {
        Some(gw) => {
            log::info!("[admin] stopping all pi processes");
            gw.kill_all();
            Ok("pi processes stopped".into())
        }
        None => Err("Pi binary not available. Download it from Settings > Versions.".into()),
    }
}

/// Start the gateway now that pi is installed (e.g. right after downloading
/// it in this session — no app restart needed). Returns the gateway URL.
#[tauri::command]
pub fn start_pi_gateway(app: tauri::AppHandle) -> Result<String, String> {
    let slot = app.state::<GatewaySlot>();
    if slot.lock().is_some() {
        return Ok("Gateway already running".into());
    }
    match crate::base::init::try_start_gateway(&app) {
        Ok(Some((gw, web_url))) => {
            *slot.lock() = Some(gw);
            log::info!("[gateway] started on demand at {}", web_url);
            Ok(web_url)
        }
        Ok(None) => Err("Pi binary not found. Download it from Settings > Versions.".into()),
        Err(e) => Err(e),
    }
}

#[tauri::command]
pub fn get_pi_agent_settings() -> Result<pi_server::PiAgentSettings, String> {
    pi_server::read_pi_settings()
}

/// Save Pi agent settings back to ~/.pi/agent/settings.json
#[tauri::command]
pub fn save_pi_agent_settings(settings: pi_server::PiAgentSettings) -> Result<(), String> {
    let path = pi_server::get_pi_agent_dir().join("settings.json");
    let json = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    log::info!("[admin] pi agent settings saved to {}", path.display());
    Ok(())
}

// ─── Package management (extension marketplace) ─────────────────────────────

/// Run `pi <args...>` with the bundled binary and return trimmed stdout.
fn run_pi_command(bin: &Path, args: &[String]) -> Result<String, String> {
    let bin_str = strip_verbatim_prefix(&bin.to_string_lossy());
    let augmented_path = build_augmented_path();
    let mut command = Command::new(&bin_str);
    configure_child_process_for_windows(&mut command);
    command
        .args(args)
        .env("PATH", augmented_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command
        .output()
        .map_err(|e| format!("Failed to run embedded pi command ({} {:?}): {}", bin_str, args, e))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let details = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("exit status {}", output.status)
    };
    Err(format!("pi command failed ({} {:?}): {}", bin_str, args, details))
}

/// Parse `pi list` output and extract package sources.
fn parse_list_output(output: &str) -> Vec<String> {
    let mut sources = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.eq_ignore_ascii_case("No packages installed.") {
            continue;
        }
        if trimmed.ends_with(':') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('-') {
            let value = rest.trim();
            if !value.is_empty() {
                sources.push(value.to_string());
            }
            continue;
        }
        // `pi list` may emit entries prefixed with two spaces.
        if let Some(value) = trimmed.strip_prefix("npm:") {
            sources.push(format!("npm:{}", value));
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("git:") {
            sources.push(format!("git:{}", value));
            continue;
        }
        if trimmed.starts_with('/') || trimmed.starts_with("./") || trimmed.starts_with("../") {
            sources.push(trimmed.to_string());
        }
    }
    sources
}

/// List packages currently installed via `pi list`.
#[tauri::command]
pub async fn list_pi_packages(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        let bin = crate::pi::try_resolve_pi_binary(&app)?;
        let output = run_pi_command(&bin, &["list".to_string()])?;
        Ok(parse_list_output(&output))
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Install a pi package via `pi install <source>`.
/// On success the package source is registered in the global extensions table.
#[tauri::command]
pub async fn install_pi_package(
    app: tauri::AppHandle,
    gw: tauri::State<'_, GatewaySlot>,
    source: String,
) -> Result<(), String> {
    if source.trim().is_empty() {
        return Err("Package source is empty".into());
    }
    let source = source.trim().to_string();
    let gw_opt: Option<std::sync::Arc<pi_server::gateway::GatewayState>> = gw.inner().lock().clone();
    tokio::task::spawn_blocking(move || {
        let bin = crate::pi::try_resolve_pi_binary(&app)?;
        run_pi_command(&bin, &["install".to_string(), source.clone()])?;
        if let Some(gw) = gw_opt.as_ref() {
            if let Err(e) = gw.db.add_global_extension(&source) {
                log::warn!("[admin] failed to register {} in DB: {}", source, e);
            }
            // Disk layout changed — drop the candidate cache and rescan in the
            // background so Installed reflects the new package.
            pi_server::gateway::ext_cache::invalidate_all(gw);
            super::extensions::refresh_extension_cache(gw.clone(), app.clone());
        }
        log::info!("[admin] installed pi package {}", source);
        Ok(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Remove a pi package via `pi remove <source>`.
/// On success the package source is removed from the global extensions table.
#[tauri::command]
pub async fn remove_pi_package(
    app: tauri::AppHandle,
    gw: tauri::State<'_, GatewaySlot>,
    source: String,
) -> Result<(), String> {
    if source.trim().is_empty() {
        return Err("Package source is empty".into());
    }
    let source = source.trim().to_string();
    let gw_opt: Option<std::sync::Arc<pi_server::gateway::GatewayState>> = gw.inner().lock().clone();
    tokio::task::spawn_blocking(move || {
        let bin = crate::pi::try_resolve_pi_binary(&app)?;
        run_pi_command(&bin, &["remove".to_string(), source.clone()])?;
        if let Some(gw) = gw_opt.as_ref() {
            if let Err(e) = gw.db.remove_global_extension(&source) {
                log::warn!("[admin] failed to remove {} from DB: {}", source, e);
            }
            // Disk layout changed — drop the candidate cache and rescan in the
            // background so Installed reflects the removal.
            pi_server::gateway::ext_cache::invalidate_all(gw);
            super::extensions::refresh_extension_cache(gw.clone(), app.clone());
        }
        log::info!("[admin] removed pi package {}", source);
        Ok(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}
