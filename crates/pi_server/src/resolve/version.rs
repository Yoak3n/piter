//! 版本锁定与高层解析入口。
//!
//! 做什么：`locked_pi_version`（编译期锁定的版本，读取 scripts/pi-version.json）、
//! `pi_binary_name`（按平台返回 pi.exe/pi）、`resolve_pi_binary`（本地查找 +
//! 下载兜底）与 `resolve_pi_binary_local`（仅本地，不下载）。
//! 不做什么：不搜索具体安装位置（locations.rs）；不执行链接/复制与解压（install.rs）；
//! 不发起下载（download.rs）。
//! 依赖：locations / install / download 三个子模块。

use std::path::{Path, PathBuf};

use log::{error, info, warn};
use serde::Deserialize;

use super::download::download_pi;
use super::install::link_or_copy;
use super::locations::{find_candidates, is_valid_pi_dir, score_pi_dir};

/// The locked version baked in at compile time.
const PI_VERSION_JSON: &str = include_str!("../../../../scripts/pi-version.json");

#[derive(Deserialize)]
struct PiVersionLock {
    version: String,
}

/// Return the locked pi version string (e.g. "0.79.10").
pub fn locked_pi_version() -> &'static str {
    // thread_local / OnceCell to avoid re-parsing on every call
    use std::sync::OnceLock;
    static CACHED: OnceLock<String> = OnceLock::new();
    CACHED.get_or_init(|| {
        let lock: PiVersionLock =
            serde_json::from_str(PI_VERSION_JSON).expect("scripts/pi-version.json is invalid");
        lock.version
    })
}

/// Path to pi binary inside an installation directory.
pub fn pi_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "pi.exe"
    } else {
        "pi"
    }
}

/// Resolve the pi binary path using only local sources (already-existing at
/// target or found at known install locations). Does NOT download from GitHub.
///
/// Returns the path to the pi executable inside `target_dir`, or an error
/// if no local copy can be found.
pub fn resolve_pi_binary_local(target_dir: &Path) -> Result<PathBuf, String> {
    let bin_path = target_dir.join(pi_binary_name());
    let mut log_msgs: Vec<String> = Vec::new();

    log_msgs.push(format!(
        "[pi resolver/local] target={}",
        bin_path.display()
    ));

    // ① Fast path: binary already exists at target
    if bin_path.is_file() {
        log_msgs.push("  ✓ Found at target".into());
        for l in &log_msgs { info!("{}", l); }
        return Ok(bin_path);
    }

    // ② Search known install locations
    log_msgs.push("  → Searching known installation locations".into());
    let candidates = find_candidates();
    let valid: Vec<(PathBuf, String)> = candidates
        .into_iter()
        .filter(|(path, _)| is_valid_pi_dir(path))
        .collect();
    let mut scored: Vec<(u32, PathBuf, String)> = valid
        .into_iter()
        .map(|(path, source)| {
            let score = score_pi_dir(&path);
            (score, path, source)
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(&b.2)));

    if let Some((_score, best_path, source)) = scored.first() {
        log_msgs.push(format!(
            "    ✓ Best candidate: {} (source={})",
            best_path.display(), source
        ));

        if let Some(parent) = target_dir.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        match link_or_copy(best_path, target_dir) {
            Ok(()) => {
                if bin_path.is_file() {
                    for l in &log_msgs { info!("{}", l); }
                    return Ok(bin_path);
                }
            }
            Err(e) => {
                log_msgs.push(format!("    ✗ Link/copy failed: {}", e));
            }
        }
    }

    for l in &log_msgs { warn!("{}", l); }
    Err("No local pi installation found. Use the Versions tab in Settings to download pi.".into())
}

/// Resolve the pi binary path. Searches known install locations, copies/links
/// into `target_dir`, or downloads from GitHub as a last resort.
///
/// Returns the path to the pi executable inside `target_dir`.
pub fn resolve_pi_binary(target_dir: &Path) -> Result<PathBuf, String> {
    let bin_path = target_dir.join(pi_binary_name());
    let mut log_msgs: Vec<String> = Vec::new();
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    log_msgs.push(format!(
        "[pi resolver] target={} platform={} {} locked_version={}",
        bin_path.display(), os, arch, locked_pi_version()
    ));

    // ── ① Fast path: binary already exists at target ──
    if bin_path.is_file() {
        log_msgs.push("  ✓ Found at target".into());
        for l in &log_msgs { info!("{}", l); }
        return Ok(bin_path);
    }

    // ── ② Search known install locations ──
    log_msgs.push("  → Searching known installation locations".into());
    let candidates = find_candidates();
    let valid: Vec<(PathBuf, String)> = candidates
        .into_iter()
        .filter(|(path, _)| is_valid_pi_dir(path))
        .collect();
    for (path, source) in &valid {
        log_msgs.push(format!("    Check {} ({}): ✓", path.display(), source));
    }
    let mut scored: Vec<(u32, PathBuf, String)> = valid
        .into_iter()
        .map(|(path, source)| {
            let score = score_pi_dir(&path);
            (score, path, source)
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(&b.2)));

    if let Some((score, best_path, source)) = scored.first() {
        log_msgs.push(format!(
            "    ✓ Best candidate: {} (source={}, score={})",
            best_path.display(), source, score
        ));

        if let Some(parent) = target_dir.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                log_msgs.push(format!("    ✗ Failed to create target dir: {}", e));
                for l in &log_msgs { error!("{}", l); }
                format!("Pi resolution failed: {}", e)
            })?;
        }

        match link_or_copy(best_path, target_dir) {
            Ok(()) => {
                if bin_path.is_file() {
                    for l in &log_msgs { info!("{}", l); }
                    info!("pi binary ready at {}", bin_path.display());
                    return Ok(bin_path);
                }
            }
            Err(e) => {
                log_msgs.push(format!("    ✗ Link/copy failed: {}", e));
            }
        }
    } else {
        log_msgs.push("    ✗ No valid pi installation found".into());
    }

    // ── ③ Download from GitHub ──
    let locked_ver = locked_pi_version();
    log_msgs.push(format!("  → Downloading pi {} from GitHub", locked_ver));

    if let Some(parent) = target_dir.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            for l in &log_msgs { error!("{}", l); }
            format!("Pi resolution failed: {}", e)
        })?;
    }

    match download_pi(locked_ver, target_dir) {
        Ok(()) => {
            if bin_path.is_file() {
                for l in &log_msgs { info!("{}", l); }
                info!("pi binary ready at {}", bin_path.display());
                return Ok(bin_path);
            }
        }
        Err(e) => {
            log_msgs.push(format!("    ✗ Download failed: {}", e));
        }
    }

    // ── ④ All strategies exhausted ──
    for l in &log_msgs { error!("{}", l); }
    let detail = log_msgs.join("\n");
    Err(format!(
        "[Pi resolver] Could not obtain pi binary.\n\n{}\n\nResolution:\n  \
         1. Install pi via the official installer from https://pi.dev\n  \
         2. Or symlink an existing pi installation into resources/pi/\n  \
         3. Or fix scripts/pi-version.json and ensure network access",
        detail
    ))
}
