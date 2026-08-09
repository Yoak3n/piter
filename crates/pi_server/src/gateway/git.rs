//! git 命令封装（消息撤回文件回滚 0.2.0 P3 用，全部只读、无副作用）。
//!
//! 做什么：提供 cwd 维度的高层 git 原语——仓库检测、`git stash create`
//! （快照 tracked 状态但不动工作区）、untracked 清单、`git restore --source`
//! （把工作区+暂存区恢复到某 commit 状态）。所有命令都是纯查询/恢复，不会
//! 改变仓库的提交历史。
//! 不做什么：不 clone/commit/push；不解析会话文件（那是 fork 链路的事）。
//! 依赖：系统 git 二进制（Git for Windows 自带；与 handlers/system.rs
//! 的 get_git_branch 共用同一启动方式）。

use std::path::Path;
use std::process::{Command, Output};

fn git_cmd() -> Command {
    // git 是控制台程序：GUI 子系统下不抑制会闪控制台窗口（同 system.rs）。
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut c = Command::new("git");
        c.creation_flags(CREATE_NO_WINDOW);
        c
    }
    #[cfg(not(target_os = "windows"))]
    Command::new("git")
}

fn run_git(cwd: &Path, args: &[&str]) -> Result<Output, String> {
    git_cmd()
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("git {} failed: {}", args.first().unwrap_or(&""), e))
}

/// Whether `cwd` sits inside a git work tree (`.git` exists via rev-parse).
pub fn is_git_repo(cwd: &Path) -> bool {
    run_git(cwd, &["rev-parse", "--is-inside-work-tree"])
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// `git stash create` → a commit-ish ref capturing the current tracked state
/// (index + worktree) **without touching the working tree**. Returns `None`
/// when there is nothing to snapshot (no changes / command failure).
pub fn stash_create(cwd: &Path) -> Option<String> {
    let out = run_git(cwd, &["stash", "create"]).ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// Untracked file paths (relative to cwd, `/` separators), from
/// `git status --porcelain -uall` (`?? ` lines). Untracked **directories**
/// are expanded to individual files by `-uall`.
pub fn list_untracked(cwd: &Path) -> Vec<String> {
    let Ok(out) = run_git(cwd, &["status", "--porcelain", "-uall"]) else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.starts_with("?? "))
        .map(|l| l.trim_start_matches("?? ").trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Restore tracked files (worktree + index) to the state captured by
/// `git_ref`. Files present in the worktree/index but absent from the ref are
/// removed — exactly the "roll back the agent's edits" semantics.
pub fn restore(cwd: &Path, git_ref: &str) -> Result<(), String> {
    let out = run_git(
        cwd,
        &["restore", "--source", git_ref, "--staged", "--worktree", "--", "."],
    )?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Skip git-dependent tests when git is unavailable in the sandbox.
    fn git_available() -> bool {
        is_git_repo(std::path::Path::new("."))
            || std::process::Command::new("git")
                .args(["rev-parse", "--is-inside-work-tree"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
    }

    #[test]
    fn stash_create_restore_roundtrip() {
        if !git_available() {
            eprintln!("git unavailable, skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path();
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(cwd)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "t@t"])
            .current_dir(cwd)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "t"])
            .current_dir(cwd)
            .output()
            .unwrap();

        // Empty repo → nothing to stash.
        assert!(stash_create(cwd).is_none());

        // Baseline commit.
        let f = cwd.join("a.txt");
        std::fs::write(&f, "v1\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "a.txt"])
            .current_dir(cwd)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-qm", "init"])
            .current_dir(cwd)
            .output()
            .unwrap();

        // Modify tracked file + create untracked file → stash create 捕获当前状态。
        std::fs::write(&f, "v2\n").unwrap();
        std::fs::write(cwd.join("new.txt"), "untracked\n").unwrap();
        let Some(git_ref) = stash_create(cwd) else {
            panic!("stash create should produce a ref");
        };
        // stash create 不动工作区。
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "v2\n");
        assert!(list_untracked(cwd).contains(&"new.txt".to_string()));

        // 之后再改到 v3 → restore 应回到快照时的 v2；untracked 不被 git restore 碰。
        std::fs::write(&f, "v3\n").unwrap();
        std::fs::write(cwd.join("new.txt"), "changed\n").unwrap();
        restore(cwd, &git_ref).unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap().trim_end(), "v2");
        assert_eq!(
            std::fs::read_to_string(cwd.join("new.txt")).unwrap(),
            "changed\n",
            "git restore 只处理 tracked，untracked 由 checkpoint 层处理"
        );
    }

    #[test]
    fn untracked_listing_detects_files() {
        if !git_available() {
            eprintln!("git unavailable, skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path();
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(cwd)
            .output()
            .unwrap();
        std::fs::create_dir_all(cwd.join("sub")).unwrap();
        std::fs::write(cwd.join("sub/nested.txt"), "x").unwrap();
        let list = list_untracked(cwd);
        assert!(list.contains(&"sub/nested.txt".to_string()), "got {:?}", list);
    }
}
