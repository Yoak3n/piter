//! 已知安装位置搜索：PATH / npm global / scoop / bun / nvm / Homebrew / Picot。
//!
//! 做什么：`find_candidates` 按平台枚举可能的 pi 安装目录（含来源标签），
//! `is_valid_pi_dir` 校验目录是否像合法安装（有二进制 + package.json），
//! `score_pi_dir` 按完整性打分，`resolve_pi_from_path` 从 PATH 解析安装根。
//! 不做什么：不执行链接/复制（install.rs）；不决定最终选用哪个候选（version.rs）。
//! 依赖：version::pi_binary_name。

use std::path::{Path, PathBuf};

use super::version::pi_binary_name;

/// Check if a directory looks like a valid pi installation (has binary + package.json).
pub(super) fn is_valid_pi_dir(dir: &Path) -> bool {
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
pub(super) fn score_pi_dir(dir: &Path) -> u32 {
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
pub(super) fn find_candidates() -> Vec<(PathBuf, String)> {
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
pub(super) fn find_candidates() -> Vec<(PathBuf, String)> {
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
