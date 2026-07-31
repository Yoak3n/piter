//! Utility functions: path handling, environment augmentation.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::types::PiAgentSettings;

// ─── Path & Environment ────────────────────────────────────────────────────

/// Strip Windows verbatim path prefix (`\\?\`).
pub fn strip_verbatim_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{}", rest)
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        path.to_string()
    }
}

/// Configure child process for Windows (no console window).
#[cfg(target_os = "windows")]
pub fn configure_child_process_for_windows(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
pub fn configure_child_process_for_windows(_command: &mut Command) {}

/// Build an augmented PATH for child processes.
pub fn build_augmented_path() -> String {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|v| std::env::split_paths(&v).collect())
        .unwrap_or_default();

    #[cfg(not(target_os = "windows"))]
    {
        let mut extras: Vec<PathBuf> = vec![
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/opt/homebrew/sbin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/local/sbin"),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
        ];

        if let Ok(home) = std::env::var("HOME") {
            let h = Path::new(&home);
            extras.push(pi_extension_npm_bin_dir(h));
            extras.push(h.join(".local/bin"));
            extras.push(h.join(".bun/bin"));
            extras.push(h.join(".cargo/bin"));
            extras.push(h.join(".local/share/mise/shims"));
            // nvm: enumerate installed node versions
            let nvm_root = h.join(".nvm/versions/node");
            if let Ok(entries) = std::fs::read_dir(nvm_root) {
                for entry in entries.flatten() {
                    let bin = entry.path().join("bin");
                    if bin.is_dir() {
                        extras.push(bin);
                    }
                }
            }
        }

        for extra in extras {
            if !dirs.iter().any(|d| d == &extra) {
                dirs.push(extra);
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let mut extras: Vec<PathBuf> = Vec::new();
        if let Ok(appdata) = std::env::var("APPDATA") {
            extras.push(Path::new(&appdata).join("npm"));
        }
        if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
            let h = Path::new(&home);
            extras.push(pi_extension_npm_bin_dir(h));
            extras.push(h.join(".cargo").join("bin"));
            extras.push(h.join(".bun").join("bin"));
            extras.push(h.join("scoop").join("shims"));
        }
        for extra in extras {
            if !dirs.iter().any(|d| d == &extra) {
                dirs.push(extra);
            }
        }
    }

    std::env::join_paths(dirs)
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| std::env::var("PATH").unwrap_or_default())
}

pub fn pi_extension_npm_bin_dir(home: &Path) -> PathBuf {
    home.join(".pi")
        .join("agent")
        .join("npm")
        .join("node_modules")
        .join(".bin")
}

pub fn log_child_path_diagnostics(context: &str, path: &str) {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok();
    let Some(home) = home else {
        log::info!(
            "[broker] child PATH diagnostics: context={} home=<unset> path={}",
            context,
            path
        );
        return;
    };

    let pi_extension_bin = pi_extension_npm_bin_dir(Path::new(&home));
    let dirs: Vec<PathBuf> = std::env::split_paths(path).collect();
    let contains_pi_extension_bin = dirs.iter().any(|dir| dir == &pi_extension_bin);

    log::info!(
        "[broker] child PATH diagnostics: context={} pi_extension_bin={} exists={} contains_pi_extension_bin={}",
        context,
        pi_extension_bin.display(),
        pi_extension_bin.is_dir(),
        contains_pi_extension_bin,
    );
}

// ─── Pi Agent Config Readers ──────────────────────────────────────────────

/// Returns the Pi agent config directory (`~/.pi/agent/`).
pub fn get_pi_agent_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".pi")
        .join("agent")
}

/// Read Pi's settings.json directly from disk — no Pi process needed.
pub fn read_pi_settings() -> Result<PiAgentSettings, String> {
    let path = get_pi_agent_dir().join("settings.json");
    let json = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    serde_json::from_str(&json)
        .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
}
