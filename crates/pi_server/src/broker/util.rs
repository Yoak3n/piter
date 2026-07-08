//! Utility functions: path handling, environment augmentation, diagnostics.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

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

// ─── Port & Network ────────────────────────────────────────────────────────

/// Check if a port is in use.
pub fn is_port_in_use(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_err()
}

/// Check if an IPv4 address is in a private (RFC 1918) range.
pub fn is_private_ipv4(ip: std::net::IpAddr) -> bool {
    if !ip.is_ipv4() || ip.is_loopback() {
        return false;
    }
    match ip {
        std::net::IpAddr::V4(v4) => match v4.octets() {
            [10, _, _, _] => true,
            [172, b, _, _] if (16..=31).contains(&b) => true,
            [192, 168, _, _] => true,
            _ => false,
        },
        _ => false,
    }
}

// ─── Payload Parsing ────────────────────────────────────────────────────────

/// Extract session id from a payload.
pub fn extract_session_id(payload: &Value) -> Option<&str> {
    payload
        .get("sessionId")
        .and_then(Value::as_str)
        .or_else(|| payload.get("sessionFile").and_then(Value::as_str))
}

// ─── Session File Helpers ──────────────────────────────────────────────────

pub fn get_sessions_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".pi")
        .join("agent")
        .join("sessions")
}

/// Decode a pi-encoded project directory name back to a path.
/// pi encodes `E:\Project\RustProject\piter` as `--E--Project-RustProject-piter--`.
/// Rule: `--` prefix/suffix, `:\` → `--`, `\` → `-`.
pub fn decode_project_name(encoded: &str) -> String {
    let trimmed = encoded
        .strip_prefix("--")
        .and_then(|s| s.strip_suffix("--"))
        .unwrap_or(encoded);
    // First "--" separates drive letter from path
    if let Some(pos) = trimmed.find("--") {
        let drive = &trimmed[..pos];
        let rest = &trimmed[pos + 2..];
        format!("{}:\\{}", drive, rest.replace('-', "\\"))
    } else {
        trimmed.replace('-', "\\")
    }
}

/// Encode a filesystem path into pi's project directory name format.
/// `E:\Project\RustProject\piter` → `--E--Project-RustProject-piter--`.
pub fn encode_project_name(path: &str) -> String {
    let step1 = path.replace(":\\", "--");
    let step2 = step1.replace('\\', "-");
    format!("--{}--", step2)
}

pub fn format_timestamp(secs: u64) -> String {
    use chrono::{DateTime, Utc};
    let dt = DateTime::from_timestamp(secs as i64, 0).unwrap_or_else(|| Utc::now());
    dt.to_rfc3339()
}

/// Generate an SVG QR code from a string.
pub fn generate_qr_svg(data: &str) -> String {
    use qrcode::QrCode;
    use qrcode::render::svg as qr_svg;
    match QrCode::new(data) {
        Ok(code) => {
            code.render()
                .min_dimensions(200, 200)
                .dark_color(qr_svg::Color("#000000"))
                .light_color(qr_svg::Color("#ffffff"))
                .build()
        }
        Err(_) => String::new(),
    }
}
