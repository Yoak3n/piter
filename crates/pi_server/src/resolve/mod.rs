//! Pi 二进制解析：搜索已知安装位置、链接/复制进 bundle 资源目录、或从 GitHub 下载兜底。
//!
//! 做什么：在目标目录（bundle 资源目录）中准备 pi 可执行文件——① 目标已存在 →
//! 快速路径；② 在已知安装位置（PATH/npm/scoop/bun/nvm/Homebrew/Picot）找到 →
//! 链接或复制；③ 全部失败 → 从 GitHub releases 下载解压。
//! 不做什么：不管理 pi 进程生命周期（broker）；不解析 pi 版本之外的产品信息。
//! 依赖：上层（src-tauri / lib.rs）通过 `resolve::resolve_pi_binary(_local)` 与
//! `download_pi_with_progress` 使用；对外 API 经本文件重导出，引用路径不变。
//!
//! 布局（按职责分文件）：
//! - version.rs   版本锁定 + 高层解析入口（resolve_pi_binary / _local / locked_pi_version）
//! - locations.rs 已知安装位置搜索（find_candidates / is_valid_pi_dir / score_pi_dir）
//! - install.rs   链接优先/复制兜底（link_or_copy / copy_dir_all / extract_archive）
//! - download.rs  GitHub 下载（download_pi / download_pi_with_progress / DownloadProgress）

mod download;
mod install;
mod locations;
mod version;

pub use download::{download_pi, download_pi_with_progress, DownloadProgress};
pub use version::{locked_pi_version, pi_binary_name, resolve_pi_binary, resolve_pi_binary_local};
