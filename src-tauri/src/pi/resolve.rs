//! Pi binary resolution: search known install locations, symlink / copy
//! into the bundle resource directory, or download from GitHub as last resort.
//!
//! Priority:
//!   1. Already present `resources/pi/` with matching `.version` → fast path
//!   2. Found at a known installation directory → symlink/copy here
//!   3. Not found anywhere → download from GitHub releases (future)

use log::{error, info, warn};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager as _};

/// The locked version baked in at compile time.
const PI_VERSION_JSON: &str = include_str!("../../../scripts/pi-version.json");

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
fn pi_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "pi.exe"
    } else {
        "pi"
    }
}

/// Check if a directory looks like a valid pi installation (has binary + package.json).
fn is_valid_pi_dir(dir: &Path) -> bool {
    let bin = dir.join(pi_binary_name());
    let pkg = dir.join("package.json");
    if !bin.is_file() || !pkg.is_file() {
        return false;
    }
    // Quick sanity: package.json should mention pi
    if let Ok(content) = std::fs::read_to_string(&pkg) {
        if content.contains("@earendil-works/pi-coding-agent") {
            return true;
        }
    }
    false
}

/// Score a pi installation by completeness (more files = higher score).
fn score_pi_dir(dir: &Path) -> u32 {
    let mut score = 0u32;
    if dir.join(pi_binary_name()).is_file() {
        score += 100;
    }
    if dir.join("package.json").is_file() {
        score += 50;
    }
    if dir.join("node_modules").is_dir() {
        score += 30;
    }
    if dir.join("theme").is_dir() {
        score += 10;
    }
    if dir.join("native").is_dir() {
        score += 10;
    }
    if dir.join("docs").is_dir() {
        score += 5;
    }
    if dir.join("assets").is_dir() {
        score += 5;
    }
    if dir.join(".version").is_file() {
        score += 5;
    }
    score
}

// ─── Platform-specific search paths ──────────────────────────────────────────

#[cfg(target_os = "windows")]
fn find_candidates() -> Vec<(PathBuf, String)> {
    let mut candidates: Vec<(PathBuf, String)> = Vec::new();

    // 1. PATH lookup
    if let Some(path_on_path) = resolve_pi_from_path() {
        candidates.push((path_on_path, "PATH".into()));
    }

    // 2. npm global
    if let Ok(appdata) = std::env::var("APPDATA") {
        let npm = Path::new(&appdata)
            .join("npm")
            .join("node_modules")
            .join("@earendil-works")
            .join("pi-coding-agent");
        candidates.push((npm, "npm:global".into()));
    }

    // 3. Picot installed (check nearby)
    if let Ok(exe) = std::env::current_exe() {
        // If we're running inside Picot or alongside it
        let beside = exe.parent().unwrap_or(Path::new(".")).join("pi");
        candidates.push((beside, "picot:beside-exe".into()));
    }

    // 4. USERPROFILE based
    if let Ok(home) = std::env::var("USERPROFILE") {
        let scoop = Path::new(&home)
            .join("scoop")
            .join("apps")
            .join("pi")
            .join("current");
        candidates.push((scoop, "scoop".into()));

        let bun = Path::new(&home)
            .join(".bun")
            .join("install")
            .join("global")
            .join("node_modules")
            .join("@earendil-works")
            .join("pi-coding-agent");
        candidates.push((bun, "bun:global".into()));

        // Also check the Picot pi install in their home area
        let picot_pi = Path::new(&home).join(".picot").join("pi");
        candidates.push((picot_pi, "picot:home".into()));
    }

    // 5. ProgramData / ProgramFiles
    if let Ok(progdata) = std::env::var("ProgramData") {
        let pd = Path::new(&progdata).join("pi");
        candidates.push((pd, "ProgramData".into()));
    }

    candidates
}

#[cfg(not(target_os = "windows"))]
fn find_candidates() -> Vec<(PathBuf, String)> {
    let mut candidates: Vec<(PathBuf, String)> = Vec::new();

    // 1. PATH lookup
    if let Some(path_on_path) = resolve_pi_from_path() {
        candidates.push((path_on_path, "PATH".into()));
    }

    // 2. npm global
    candidates.push((
        PathBuf::from("/usr/local/lib/node_modules/@earendil-works/pi-coding-agent"),
        "npm:global".into(),
    ));
    candidates.push((
        PathBuf::from("/usr/lib/node_modules/@earendil-works/pi-coding-agent"),
        "npm:global-alt".into(),
    ));

    // 3. Picot .app bundle
    candidates.push((
        PathBuf::from("/Applications/Picot.app/Contents/Resources/pi"),
        "picot:app".into(),
    ));

    // 4. Home directories (nvm, homebrew, bun)
    if let Ok(home) = std::env::var("HOME") {
        let h = Path::new(&home);

        // nvm: enumerate installed node versions
        let nvm_root = h.join(".nvm/versions/node");
        if let Ok(entries) = std::fs::read_dir(&nvm_root) {
            for entry in entries.flatten() {
                let bin = entry
                    .path()
                    .join("lib/node_modules/@earendil-works/pi-coding-agent");
                candidates.push((bin, "nvm".into()));
            }
        }

        // bun
        candidates.push((
            h.join(".bun/install/global/node_modules/@earendil-works/pi-coding-agent"),
            "bun:global".into(),
        ));

        // picot in home
        candidates.push((h.join(".picot/pi"), "picot:home".into()));
    }

    // 5. Homebrew
    candidates.push((
        PathBuf::from("/opt/homebrew/lib/node_modules/@earendil-works/pi-coding-agent"),
        "homebrew".into(),
    ));

    candidates
}

/// Try to resolve pi from PATH and find the installation root.
fn resolve_pi_from_path() -> Option<PathBuf> {
    let bin_name = pi_binary_name();
    // Look up in PATH via the `path` crate's `which` equivalent
    std::env::var_os("PATH").and_then(|paths| {
        for p in std::env::split_paths(&paths) {
            let candidate = p.join(bin_name);
            if candidate.is_file() {
                // Found the binary. Try to canonicalize and find the install root.
                if let Ok(canonical) = candidate.canonicalize() {
                    // The binary is at .../pi/pi.exe — parent is the install root
                    if let Some(parent) = canonical.parent() {
                        if is_valid_pi_dir(parent) {
                            return Some(parent.to_path_buf());
                        }
                        // Also check grandparent (npm: .../pi-coding-agent/dist/cli.js)
                        if let Some(grandparent) = parent.parent() {
                            if is_valid_pi_dir(grandparent) {
                                return Some(grandparent.to_path_buf());
                            }
                        }
                    }
                    // Last resort: use the binary's directory
                    if let Some(dir) = canonical.parent() {
                        return Some(dir.to_path_buf());
                    }
                }
                // Fallback: the path from PATH is a directory containing pi
                if p.is_dir() && is_valid_pi_dir(&p) {
                    return Some(p);
                }
                // Just return the parent of the binary
                if let Some(parent) = candidate.parent() {
                    if !parent.as_os_str().is_empty() {
                        return Some(parent.to_path_buf());
                    }
                }
            }
        }
        None
    })
}

// ─── Symlink / Copy logic ────────────────────────────────────────────────────

/// Create a symlink from `dest` to `src`. On failure, fall back to copying.
fn link_or_copy(src: &Path, dest: &Path) -> Result<(), String> {
    if dest.exists() {
        // Remove existing (could be stale symlink or old copy)
        if dest.is_dir() {
            std::fs::remove_dir_all(dest)
                .map_err(|e| format!("Failed to remove existing dest dir: {}", e))?;
        } else {
            std::fs::remove_file(dest)
                .map_err(|e| format!("Failed to remove existing dest file: {}", e))?;
        }
    }

    // Try symlink first
    #[cfg(target_os = "windows")]
    {
        match std::os::windows::fs::symlink_dir(src, dest) {
            Ok(()) => {
                info!(
                    "Created directory symlink: {} → {}",
                    dest.display(),
                    src.display()
                );
                return Ok(());
            }
            Err(e) => {
                warn!("Directory symlink failed ({}), falling back to copy.", e);
                // Check if cross-drive
                let cross_drive = src
                    .ancestors()
                    .last()
                    .and_then(|s| s.to_str())
                    .zip(dest.ancestors().last().and_then(|d| d.to_str()))
                    .map(|(s, d)| {
                        let s_drive = s.split(':').next().unwrap_or("");
                        let d_drive = d.split(':').next().unwrap_or("");
                        s_drive != d_drive
                    })
                    .unwrap_or(false);
                if cross_drive {
                    warn!(
                        "Cross-drive symlink; Developer Mode may be needed. Falling back to copy."
                    );
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        match std::os::unix::fs::symlink(src, dest) {
            Ok(()) => {
                info!("Created symlink: {} → {}", dest.display(), src.display());
                return Ok(());
            }
            Err(e) => {
                warn!("Symlink failed ({}), falling back to copy.", e);
            }
        }
    }

    // Fallback: copy the entire directory
    info!(
        "Copying {} → {} (this may take a while)...",
        src.display(),
        dest.display()
    );
    copy_dir_all(src, dest).map_err(|e| format!("Failed to copy pi directory: {}", e))?;
    info!("Copy complete.");
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

// ─── GitHub download fallback ────────────────────────────────────────────────

/// Platform-specific release asset descriptor.
struct PlatformAsset {
    #[allow(dead_code)]
    key: &'static str,
    archive_name: String,
    binary_name: String,
    is_zip: bool,
}

fn detect_platform() -> Result<PlatformAsset, String> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let (key, is_zip) = match (os, arch) {
        ("windows", "x86_64") => ("windows-x64", true),
        ("windows", "aarch64") => ("windows-arm64", true),
        ("macos", "aarch64") => ("darwin-arm64", false),
        ("macos", "x86_64") => ("darwin-x64", false),
        ("linux", "x86_64") => ("linux-x64", false),
        ("linux", "aarch64") => ("linux-arm64", false),
        _ => return Err(format!("Unsupported platform: {} {}", os, arch)),
    };
    let is_windows = key.starts_with("windows-");
    Ok(PlatformAsset {
        key,
        archive_name: if is_zip {
            format!("pi-{}.zip", key)
        } else {
            format!("pi-{}.tar.gz", key)
        },
        binary_name: if is_windows {
            "pi.exe".into()
        } else {
            "pi".into()
        },
        is_zip,
    })
}

/// Extract and flatten the archive into `target_dir`.
/// Handles the `pi/` wrapper dir that GitHub release archives contain.
fn extract_archive(archive_path: &Path, target_dir: &Path, is_zip: bool) -> Result<(), String> {
    // Extract into a temp staging dir first
    let staging =
        tempfile::tempdir().map_err(|e| format!("Failed to create staging dir: {}", e))?;
    let staging_path = staging.path();

    if is_zip {
        let file =
            std::fs::File::open(archive_path).map_err(|e| format!("Failed to open zip: {}", e))?;
        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| format!("Failed to read zip: {}", e))?;
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| format!("Zip entry {}: {}", i, e))?;
            let raw_name = entry.name().to_string();
            if raw_name.is_empty() || raw_name.ends_with('/') {
                continue;
            }
            // Sanitize: skip absolute paths and parent-dir traversal
            let clean = raw_name.replace('\\', "/");
            let name = PathBuf::from(&clean);
            let target = staging_path.join(&name);
            if let Some(p) = target.parent() {
                std::fs::create_dir_all(p)
                    .map_err(|e| format!("Create dir {}: {}", p.display(), e))?;
            }
            let mut out = std::fs::File::create(&target)
                .map_err(|e| format!("Create {}: {}", target.display(), e))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| format!("Extract {}: {}", target.display(), e))?;
        }
    } else {
        let file = std::fs::File::open(archive_path)
            .map_err(|e| format!("Failed to open tar.gz: {}", e))?;
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        archive
            .unpack(staging_path)
            .map_err(|e| format!("Failed to extract tar.gz: {}", e))?;
    }

    // Flatten wrapper dir: the archive wraps everything in a single `pi/` dir
    let wrapper = staging_path.join("pi");
    if wrapper.is_dir() && target_dir.exists() {
        std::fs::remove_dir_all(target_dir)
            .map_err(|e| format!("Failed to remove target: {}", e))?;
    }
    std::fs::create_dir_all(target_dir).map_err(|e| format!("Create target dir: {}", e))?;

    if wrapper.is_dir() {
        // Promote contents of wrapper up one level
        for entry in std::fs::read_dir(&wrapper).map_err(|e| format!("Read wrapper dir: {}", e))? {
            let entry = entry.map_err(|e| format!("Dir entry: {}", e))?;
            let src = entry.path();
            let dst = target_dir.join(entry.file_name());
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                copy_dir_all(&src, &dst)
                    .map_err(|e| format!("Copy dir {}: {}", src.display(), e))?;
            } else {
                std::fs::copy(&src, &dst)
                    .map_err(|e| format!("Copy file {}: {}", src.display(), e))?;
            }
        }
    } else {
        // No wrapper — copy staging as-is
        for entry in
            std::fs::read_dir(staging_path).map_err(|e| format!("Read staging dir: {}", e))?
        {
            let entry = entry.map_err(|e| format!("Dir entry: {}", e))?;
            let src = entry.path();
            let dst = target_dir.join(entry.file_name());
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                copy_dir_all(&src, &dst)
                    .map_err(|e| format!("Copy dir {}: {}", src.display(), e))?;
            } else {
                std::fs::copy(&src, &dst)
                    .map_err(|e| format!("Copy file {}: {}", src.display(), e))?;
            }
        }
    }

    Ok(())
}

/// Download a pi release from GitHub and extract to `target_dir`.
fn download_pi(version: &str, target_dir: &Path) -> Result<(), String> {
    let asset = detect_platform()?;
    let url = format!(
        "https://github.com/earendil-works/pi/releases/download/v{}/{}",
        version, asset.archive_name
    );

    info!("Downloading pi {} from {}...", version, url);

    // Download to temp file
    let tmp_dir = tempfile::tempdir().map_err(|e| format!("Create temp dir: {}", e))?;
    let archive_path = tmp_dir.path().join(&asset.archive_name);

    let client = reqwest::blocking::Client::builder()
        .user_agent("piter/0.1.0")
        .build()
        .map_err(|e| format!("Build HTTP client: {}", e))?;

    let response = client
        .get(&url)
        .send()
        .map_err(|e| format!("HTTP request failed: {} (URL: {})", e, url))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download failed: HTTP {} for {}",
            response.status(),
            url
        ));
    }

    // Write response body to file
    let total = response.content_length().unwrap_or(0);
    if total > 0 {
        info!("  Download size: {} MB", total / 1024 / 1024);
    }
    let mut file =
        std::fs::File::create(&archive_path).map_err(|e| format!("Create temp file: {}", e))?;
    let mut response = response;
    std::io::copy(&mut response, &mut file).map_err(|e| format!("Download failed: {}", e))?;
    drop(file);
    info!("Download complete. Extracting...");

    // Extract
    extract_archive(&archive_path, target_dir, asset.is_zip)?;

    // Verify binary
    let bin = target_dir.join(&asset.binary_name);
    if !bin.is_file() {
        return Err(format!(
            "Extraction succeeded but {} is missing. Archive layout may have changed.",
            bin.display()
        ));
    }

    // Set executable bit on Unix
    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&bin) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(&bin, perms);
        }
    }

    // Write version marker
    std::fs::write(target_dir.join(".version"), version)
        .map_err(|e| format!("Write .version: {}", e))?;

    info!("pi {} installed to {}", version, target_dir.display());
    Ok(())
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Return the path to the `resources/pi/` directory inside the Tauri bundle.
fn bundle_pi_dir(app_handle: &AppHandle) -> PathBuf {
    // During dev: src-tauri/resources/pi/
    // During production: {resource_dir}/pi/
    if cfg!(debug_assertions) {
        // Use CARGO_MANIFEST_DIR relative path for dev
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("pi")
    } else {
        // Production: use resource dir
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
    let bin_path = target_dir.join(pi_binary_name());
    let mut log: Vec<String> = Vec::new();
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    log.push(format!(
        "[Pi resolver] target={} platform={} {} locked_version={}",
        bin_path.display(),
        os,
        arch,
        locked_pi_version()
    ));

    // ── ① Fast path: binary already exists at target ──
    log.push(format!(
        "  → Step 1: Check {} for existing binary",
        bin_path.display()
    ));
    if bin_path.is_file() {
        log.push("    ✓ Found".into());
        for l in &log {
            info!("{}", l);
        }
        info!("pi binary ready at {}", bin_path.display());
        return Ok(bin_path);
    }
    log.push("    ✗ Not found".into());
    log.push("      Hint: place or symlink pi here for fastest startup".into());

    // ── ② Search known install locations ──
    log.push("  → Step 2: Search known installation locations".into());
    let candidates = find_candidates();
    #[cfg(target_os = "windows")]
    log.push("      Paths: PATH, %APPDATA%\\npm\\..., Picot, scoop, bun, ProgramData".into());
    #[cfg(target_os = "macos")]
    log.push(
        "      Paths: PATH, /usr/local/lib/..., /Applications/Picot.app, nvm, homebrew, bun".into(),
    );
    #[cfg(target_os = "linux")]
    log.push("      Paths: PATH, /usr/local/lib/..., nvm, bun".into());

    // Filter and score in two passes to avoid closure borrow conflicts with `log`
    let valid: Vec<(PathBuf, String)> = candidates
        .into_iter()
        .filter(|(path, _)| is_valid_pi_dir(path))
        .collect();
    for (ref path, ref source) in &valid {
        log.push(format!("      Check {} ({}): ✓", path.display(), source));
    }
    let mut scored: Vec<(u32, PathBuf, String)> = valid
        .into_iter()
        .map(|(path, source)| {
            let score = score_pi_dir(&path);
            (score, path, source)
        })
        .collect();
    // Log scores
    for (score, ref _path, ref source) in &scored {
        log.push(format!("        → {} score={}", source, score));
    }

    // Sort by score descending, then by source name
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(&b.2)));

    if let Some((score, best_path, source)) = scored.first() {
        log.push(format!(
            "    ✓ Best candidate: {} (source={}, score={})",
            best_path.display(),
            source,
            score
        ));

        if let Some(parent) = target_dir.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                log.push(format!("    ✗ Failed to create target dir: {}", e));
                for l in &log {
                    error!("{}", l);
                }
                format!("Pi resolution failed: {}", e)
            })?;
        }

        match link_or_copy(best_path, &target_dir) {
            Ok(()) => {
                if bin_path.is_file() {
                    log.push(format!("    ✓ Linked to {}", target_dir.display()));
                    for l in &log {
                        info!("{}", l);
                    }
                    info!("pi binary ready at {}", bin_path.display());
                    return Ok(bin_path);
                }
            }
            Err(e) => {
                log.push(format!("    ✗ Link/copy failed: {}", e));
            }
        }
    } else {
        log.push("    ✗ No valid pi installation found in any location".into());
    }
    log.push("      Hint: install pi via your package manager or from https://pi.dev".into());

    // ── ③ Download from GitHub ──
    let locked_ver = locked_pi_version();
    log.push(format!(
        "  → Step 3: Download pi {} from GitHub releases",
        locked_ver
    ));
    let url = format!(
        "https://github.com/earendil-works/pi/releases/download/v{}/{}-{}.zip",
        locked_ver,
        "pi",
        if std::env::consts::OS == "windows" {
            if std::env::consts::ARCH == "x86_64" {
                "windows-x64"
            } else {
                "windows-arm64"
            }
        } else if std::env::consts::OS == "macos" {
            if std::env::consts::ARCH == "aarch64" {
                "darwin-arm64"
            } else {
                "darwin-x64"
            }
        } else {
            if std::env::consts::ARCH == "x86_64" {
                "linux-x64"
            } else {
                "linux-arm64"
            }
        }
    );
    log.push(format!("      URL: {}", url));

    if let Some(parent) = target_dir.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            log.push(format!("    ✗ Failed to create target dir: {}", e));
            for l in &log {
                error!("{}", l);
            }
            format!("Pi resolution failed: {}", e)
        })?;
    }

    match download_pi(locked_ver, &target_dir) {
        Ok(()) => {
            if bin_path.is_file() {
                log.push("    ✓ Download and extraction complete".into());
                for l in &log {
                    info!("{}", l);
                }
                info!("pi binary ready at {}", bin_path.display());
                return Ok(bin_path);
            }
        }
        Err(e) => {
            log.push(format!("    ✗ Download failed: {}", e));
            log.push(
                "      Hint: verify the version in scripts/pi-version.json exists on GitHub".into(),
            );
        }
    }

    // ── ④ All strategies exhausted ──
    for l in &log {
        error!("{}", l);
    }
    let detail = log.join("\n");
    Err(format!(
        "[Pi resolver] All strategies exhausted — could not obtain pi binary.\n\n{}\n\nResolution:\n  \
         1. Install pi via the official installer from https://pi.dev\n  \
         2. Or symlink an existing pi installation:\n  \
            mklink /D src-tauri\\resources\\pi <path-to-pi>   (Windows, admin)\n  \
            ln -s <path-to-pi> src-tauri/resources/pi          (macOS/Linux)\n  \
         3. Or fix scripts/pi-version.json and ensure network access for download",
        detail
    ))
}
