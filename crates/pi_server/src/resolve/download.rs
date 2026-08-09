//! GitHub 下载兜底：平台检测、HTTP 客户端（含代理）、流式下载 + 进度、解压落盘。
//!
//! 做什么：`download_pi_with_progress` 从 GitHub releases 下载当前平台的发布包
//! （zip/tar.gz），流式写临时文件并上报进度，随后交给 install.rs 解压；代理解析
//! 优先显式环境变量（HTTPS_PROXY 等，含小写变体），其次系统代理。
//! 不做什么：不搜索本地安装（locations.rs）；不决定是否下载（version.rs）。
//! 依赖：install::extract_archive。

use std::path::Path;

use log::info;

use super::install::extract_archive;

// ─── GitHub download fallback ────────────────────────────────────────────────

/// Progress events emitted while downloading / extracting / installing pi.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum DownloadProgress {
    /// Downloading the release archive.
    Downloading { downloaded: u64, total: Option<u64> },
    /// Extracting the archive (zip only reports per-entry progress).
    Extracting { current: usize, total: usize },
    /// Verifying the extracted binary.
    Verifying,
    /// Installation finished.
    Done,
}

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

/// Build the HTTP client used for downloads.
///
/// Proxy resolution priority:
///   1. Explicit proxy env vars (HTTPS_PROXY / HTTP_PROXY / ALL_PROXY,
///      incl. lowercase) — matched first, e.g. Clash 等工具的终端代理。
///   2. Windows/macOS system proxy (enabled via reqwest's `system-proxy`
///      feature) — covers apps that only set the system proxy, such as
///      Clash Verge Rev 的「系统代理」模式。
fn build_download_client() -> Result<reqwest::blocking::Client, String> {
    let mut builder = reqwest::blocking::Client::builder().user_agent("piter/0.1.0");

    if let Some(proxy) = proxy_from_env() {
        info!("Downloading via proxy: {}", proxy);
        builder = builder.proxy(
            reqwest::Proxy::all(&proxy)
                .map_err(|e| format!("Invalid proxy URL '{}': {}", proxy, e))?,
        );
    }

    builder
        .build()
        .map_err(|e| format!("Build HTTP client: {}", e))
}

/// Read the first available proxy URL from common proxy environment
/// variables. Windows env vars are case-insensitive, but lowercase variants
/// are checked too for portability (macOS/Linux shells).
fn proxy_from_env() -> Option<String> {
    const NAMES: [&str; 6] = [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ];
    for name in NAMES {
        if let Ok(raw) = std::env::var(name) {
            let val = raw.trim().to_string();
            if val.is_empty() {
                continue;
            }
            // reqwest requires a scheme; default to http:// if missing
            return Some(if val.contains("://") {
                val
            } else {
                format!("http://{}", val)
            });
        }
    }
    None
}

/// Download a pi release from GitHub and extract to `target_dir`.
pub fn download_pi(version: &str, target_dir: &Path) -> Result<(), String> {
    download_pi_with_progress(version, target_dir, |_| {})
}

/// Download a pi release from GitHub and extract to `target_dir`,
/// reporting progress through `on_progress`.
pub fn download_pi_with_progress(
    version: &str,
    target_dir: &Path,
    on_progress: impl Fn(DownloadProgress),
) -> Result<(), String> {
    let asset = detect_platform()?;
    let url = format!(
        "https://github.com/earendil-works/pi/releases/download/v{}/{}",
        version, asset.archive_name
    );

    info!("Downloading pi {} from {}...", version, url);

    // Download to temp file
    let tmp_dir = tempfile::tempdir().map_err(|e| format!("Create temp dir: {}", e))?;
    let archive_path = tmp_dir.path().join(&asset.archive_name);

    let client = build_download_client()?;

    let mut response = client
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

    // Stream the response body to file, reporting download progress.
    let total = response.content_length();
    if let Some(total) = total {
        info!("  Download size: {} MB", total / 1024 / 1024);
    }
    let mut file =
        std::fs::File::create(&archive_path).map_err(|e| format!("Create temp file: {}", e))?;
    let mut downloaded: u64 = 0;
    let mut buf = [0u8; 64 * 1024];
    loop {
        use std::io::{Read, Write};
        let n = response
            .read(&mut buf)
            .map_err(|e| format!("Download failed: {}", e))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("Write temp file: {}", e))?;
        downloaded += n as u64;
        on_progress(DownloadProgress::Downloading { downloaded, total });
    }
    drop(file);
    info!("Download complete. Extracting...");

    // Extract
    extract_archive(&archive_path, target_dir, asset.is_zip, &on_progress)?;

    // Verify binary
    let bin = target_dir.join(&asset.binary_name);
    if !bin.is_file() {
        return Err(format!(
            "Extraction succeeded but {} is missing. Archive layout may have changed.",
            bin.display()
        ));
    }
    on_progress(DownloadProgress::Verifying);

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

    on_progress(DownloadProgress::Done);

    info!("pi {} installed to {}", version, target_dir.display());
    Ok(())
}
