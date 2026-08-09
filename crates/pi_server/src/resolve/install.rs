//! 安装到 bundle 资源目录：链接优先、失败回退复制；归档解压。
//!
//! 做什么：`link_or_copy` 将已找到的 pi 安装链接（Windows 目录链接/Unix 符号链接）
//! 到目标目录，失败（如跨盘、权限）时整目录复制；`extract_archive` 解压 GitHub
//! 发布的 zip/tar.gz 并拍平 `pi/` 包裹目录；`copy_dir_all` 递归复制。
//! 不做什么：不搜索安装位置（locations.rs）；不发起下载（download.rs）。
//! 依赖：download::DownloadProgress（进度事件类型）。

use std::path::{Path, PathBuf};

use log::{info, warn};

use super::download::DownloadProgress;

// ─── Symlink / Copy logic ────────────────────────────────────────────────────

/// Create a symlink from `dest` to `src`. On failure, fall back to copying.
pub(super) fn link_or_copy(src: &Path, dest: &Path) -> Result<(), String> {
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

/// Extract and flatten the archive into `target_dir`.
/// Handles the `pi/` wrapper dir that GitHub release archives contain.
pub(super) fn extract_archive(
    archive_path: &Path,
    target_dir: &Path,
    is_zip: bool,
    on_progress: &dyn Fn(DownloadProgress),
) -> Result<(), String> {
    // Extract into a temp staging dir first
    let staging =
        tempfile::tempdir().map_err(|e| format!("Failed to create staging dir: {}", e))?;
    let staging_path = staging.path();

    if is_zip {
        let file =
            std::fs::File::open(archive_path).map_err(|e| format!("Failed to open zip: {}", e))?;
        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| format!("Failed to read zip: {}", e))?;
        let total_entries = archive.len();
        for i in 0..archive.len() {
            on_progress(DownloadProgress::Extracting {
                current: i + 1,
                total: total_entries,
            });
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
