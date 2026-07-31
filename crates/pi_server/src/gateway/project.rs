//! Project and session metadata management.
//!
//! All metadata is stored in SQLite (`~/.pi/agent/piter.db`).
//! Extension names are resolved to file paths at spawn time.
//!
//! Extension resolution order (per name):
//! 1. `~/.pi/agent/extensions/<name>.ts` (global)
//! 2. `~/.pi/agent/extensions/<name>/index.ts` (global directory)
//! 3. `<cwd>/.pi/extensions/<name>.ts` (project-local)
//! 4. `<cwd>/.pi/extensions/<name>/index.ts` (project-local directory)

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::db::Db;

// ─── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub cwd: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub pinned: i32,
    #[serde(default)]
    pub archived: bool,
    pub created_at: String,
    pub updated_at: String,
}

// ─── Project CRUD (delegates to db) ─────────────────────────────────────────

pub fn create_project(db: &Db, name: &str, cwd: &str, extensions: Vec<String>) -> Result<Project, String> {
    let id = uuid::Uuid::new_v4().to_string();
    db.create_project(&id, name, cwd)?;
    if !extensions.is_empty() {
        db.update_project(&id, None, Some(&extensions))?;
    }
    let now = chrono::Utc::now().to_rfc3339();
    Ok(Project {
        id,
        name: name.to_string(),
        cwd: cwd.to_string(),
        extensions,
        pinned: 0,
        archived: false,
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn update_project(db: &Db, id: &str, name: Option<&str>, extensions: Option<Vec<String>>) -> Result<Project, String> {
    db.update_project(id, name, extensions.as_deref())?;
    let row = db.get_project(id).ok_or_else(|| format!("project not found: {}", id))?;
    let exts = db.get_project_extensions(id);
    Ok(Project {
        id: row.id,
        name: row.name,
        cwd: row.cwd,
        extensions: exts,
        pinned: row.pinned,
        archived: row.archived,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

pub fn delete_project(db: &Db, id: &str) -> Result<(), String> {
    db.delete_project(id)
}

pub fn list_projects(db: &Db, include_archived: bool) -> Vec<Project> {
    db.list_projects(include_archived)
        .into_iter()
        .map(|row| {
            let exts = db.get_project_extensions(&row.id);
            Project {
                id: row.id,
                name: row.name,
                cwd: row.cwd,
                extensions: exts,
                pinned: row.pinned,
                archived: row.archived,
                created_at: row.created_at,
                updated_at: row.updated_at,
            }
        })
        .collect()
}

// ─── Extension Resolution ───────────────────────────────────────────────────

/// Discover extension names present in a directory.
///
/// Matches the auto-discovery locations documented by pi:
/// - `<dir>/<name>.ts` (single file)
/// - `<dir>/<name>/index.ts` (subdirectory)
pub fn discover_extensions(dir: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_file() && name.ends_with(".ts") {
            names.push(name.trim_end_matches(".ts").to_string());
        } else if path.is_dir() && path.join("index.ts").is_file() {
            names.push(name);
        }
    }
    names.sort();
    names
}

/// A discovered extension candidate: display/database name plus the resolved
/// entry file (when it can be determined).
#[derive(Debug, Clone)]
pub struct ExtensionEntry {
    pub name: String,
    pub path: Option<PathBuf>,
}

// ─── Package extension discovery ────────────────────────────────────────────

fn has_glob_chars(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[')
}

/// Convert a glob pattern (relative, `/`-separated) to a regex.
/// Supports `**` (any depth), `*` (within one path segment) and `?`.
fn glob_to_regex(pattern: &str) -> String {
    let mut re = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => {
                if chars.peek() == Some(&'*') {
                    chars.next();
                    re.push_str(".*");
                } else {
                    re.push_str("[^/]*");
                }
            }
            '?' => re.push_str("[^/]"),
            '.' | '(' | ')' | '+' | '|' | '^' | '$' | '{' | '}' | '\\' => {
                re.push('\\');
                re.push(c);
            }
            '[' => {
                let mut cls = String::from("[");
                for c2 in chars.by_ref() {
                    cls.push(c2);
                    if c2 == ']' {
                        break;
                    }
                }
                re.push_str(&cls);
            }
            _ => re.push(c),
        }
    }
    re
}

fn walk_files(dir: &Path, base: &Path, re: &regex::Regex, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk_files(&p, base, re, out);
        } else if p.is_file() {
            let rel = p
                .strip_prefix(base)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            if re.is_match(&rel) {
                out.push(p);
            }
        }
    }
}

/// Expand a glob pattern into matching files, walking from the pattern's
/// non-glob ancestor directory.
fn glob_files(pattern: &Path) -> Vec<PathBuf> {
    let pat_str = pattern.to_string_lossy().replace('\\', "/");
    let Some(gs) = pat_str.find(|c| c == '*' || c == '?' || c == '[') else {
        return Vec::new();
    };
    let (base_str, glob_part) = match pat_str[..gs].rfind('/') {
        Some(i) => (&pat_str[..i], &pat_str[i + 1..]),
        None => ("", pat_str.as_str()),
    };
    let base = if base_str.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(base_str)
    };
    let Ok(re) = regex::Regex::new(&format!("^{}$", glob_to_regex(glob_part))) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    walk_files(&base, &base, &re, &mut out);
    out.sort();
    out
}

/// Expand a package's `pi.extensions` glob list into concrete entry files.
/// A non-glob entry that is a directory resolves to `index.ts` inside it
/// (or its top-level `.ts`/`.js` files when no `index.ts` exists).
fn expand_pi_extensions(pkg_dir: &Path) -> Vec<PathBuf> {
    let Ok(json_str) = std::fs::read_to_string(pkg_dir.join("package.json")) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&json_str) else {
        return Vec::new();
    };
    let Some(globs) = json
        .get("pi")
        .and_then(|p| p.get("extensions"))
        .and_then(|e| e.as_array())
    else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = Vec::new();
    for g in globs.iter().filter_map(|v| v.as_str()) {
        let g = g.strip_prefix("./").unwrap_or(g);
        let p = pkg_dir.join(g);
        if has_glob_chars(g) {
            out.extend(glob_files(&p));
        } else if p.is_file() {
            out.push(p);
        } else if p.is_dir() {
            let idx_ts = p.join("index.ts");
            let idx_js = p.join("index.js");
            if idx_ts.is_file() {
                out.push(idx_ts);
            } else if idx_js.is_file() {
                out.push(idx_js);
            } else if let Ok(rd) = std::fs::read_dir(&p) {
                let mut files: Vec<PathBuf> = rd
                    .flatten()
                    .map(|e| e.path())
                    .filter(|f| {
                        f.is_file()
                            && f.extension()
                                .is_some_and(|e| e == "ts" || e == "js")
                    })
                    .collect();
                files.sort();
                out.extend(files);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Return the first extension entry file declared by a package directory.
fn first_pi_extension(pkg_dir: &Path) -> Option<PathBuf> {
    expand_pi_extensions(pkg_dir).into_iter().next()
}

/// Resolve an `npm:<pkg>` reference to an entry file, checking the global
/// and project-local package installs.
fn resolve_npm_package_extension(pkg: &str, cwd: &str) -> Option<PathBuf> {
    let global_npm = crate::broker::util::get_pi_agent_dir()
        .join("npm")
        .join("node_modules");
    let cwd_npm = Path::new(cwd).join(".pi").join("npm").join("node_modules");
    for base in [global_npm, cwd_npm] {
        if let Some(f) = first_pi_extension(&base.join(pkg)) {
            return Some(f);
        }
    }
    None
}

/// Discover extension entries contributed by installed npm/git packages
/// under `base` (e.g. `~/.pi/agent` or `<cwd>/.pi`). Names use the package
/// source format (`npm:...` / `git:...`) so they match DB registrations.
pub fn discover_package_extensions(base: &Path) -> Vec<ExtensionEntry> {
    let mut entries = Vec::new();

    // npm packages: <base>/npm/node_modules/<pkg> (scoped: <base>/npm/node_modules/@scope/<name>)
    let npm_dir = base.join("npm").join("node_modules");
    if let Ok(rd) = std::fs::read_dir(&npm_dir) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            let pkg_dirs: Vec<PathBuf> = if name.starts_with('@') && path.is_dir() {
                std::fs::read_dir(&path)
                    .map(|srd| {
                        srd.flatten()
                            .map(|e| e.path())
                            .filter(|p| p.is_dir())
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                vec![path]
            };
            for pkg_dir in pkg_dirs {
                let Some(pkg_name) = pkg_dir.file_name().map(|n| n.to_string_lossy().to_string())
                else {
                    continue;
                };
                let full_name = if name.starts_with('@') {
                    format!("npm:{}/{}", name, pkg_name)
                } else {
                    format!("npm:{}", pkg_name)
                };
                let files = expand_pi_extensions(&pkg_dir);
                if !files.is_empty() {
                    entries.push(ExtensionEntry {
                        name: full_name,
                        path: files.into_iter().next(),
                    });
                }
            }
        }
    }

    // git packages: <base>/git/<host>/<path...>
    let git_dir = base.join("git");
    if let Ok(rd) = std::fs::read_dir(&git_dir) {
        for host in rd.flatten() {
            let host_path = host.path();
            if !host_path.is_dir() {
                continue;
            }
            collect_git_packages(&host_path, &host_path, &mut entries, 0);
        }
    }

    entries
}

/// Recursively scan a git checkout tree (bounded depth) for package dirs.
fn collect_git_packages(
    dir: &Path,
    base: &Path,
    entries: &mut Vec<ExtensionEntry>,
    depth: usize,
) {
    if depth > 4 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name.starts_with('.') {
            continue;
        }
        if p.join("package.json").is_file() && first_pi_extension(&p).is_some() {
            let rel = p
                .strip_prefix(base)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| file_name.clone());
            let files = expand_pi_extensions(&p);
            entries.push(ExtensionEntry {
                name: format!("git:{}", rel),
                path: files.into_iter().next(),
            });
        }
        collect_git_packages(&p, base, entries, depth + 1);
    }
}

/// Discover every extension candidate for a scope: direct files in `ext_dir`
/// plus entries from installed packages under `base`. `cwd` is used for
/// path resolution (project root, or "" for the global scope).
pub fn discover_scope_extensions(ext_dir: &Path, base: &Path, cwd: &str) -> Vec<ExtensionEntry> {
    let mut entries: Vec<ExtensionEntry> = discover_extensions(ext_dir)
        .into_iter()
        .map(|name| ExtensionEntry {
            name: name.clone(),
            path: resolve_extension_name(&name, cwd),
        })
        .collect();
    entries.extend(discover_package_extensions(base));
    entries
}

/// Discover extension candidates for a project scope: the global candidates
/// (direct extensions + installed packages) merged with the project-local
/// ones (`.pi/extensions/` + `.pi/npm|git/`), deduped by name.
pub fn discover_project_extensions(agent_dir: &Path, cwd: &str) -> Vec<ExtensionEntry> {
    let proj_dir = Path::new(cwd).join(".pi");
    let mut entries = discover_scope_extensions(&agent_dir.join("extensions"), agent_dir, "");
    let proj = discover_scope_extensions(&proj_dir.join("extensions"), &proj_dir, cwd);
    let mut seen: std::collections::HashSet<String> =
        entries.iter().map(|e| e.name.clone()).collect();
    for e in proj {
        if seen.insert(e.name.clone()) {
            entries.push(e);
        }
    }
    entries
}

/// Resolve an extension name to a file path using filesystem probes.
///
/// Search order:
/// 1. `~/.pi/agent/extensions/<name>.ts` (global single file)
/// 2. `~/.pi/agent/extensions/<name>/index.ts` (global directory)
/// 3. `<cwd>/.pi/extensions/<name>.ts` (project-local single file)
/// 4. `<cwd>/.pi/extensions/<name>/index.ts` (project-local directory)
/// 5. `npm:<pkg>` — package extension entry (global then project installs)
/// 6. `git:<host>/<path>` — git package extension entry (global then project)
pub fn resolve_extension_name(name: &str, cwd: &str) -> Option<PathBuf> {
    let global_dir = crate::broker::util::get_pi_agent_dir().join("extensions");

    // Package-backed extensions
    if let Some(pkg) = name.strip_prefix("npm:") {
        return resolve_npm_package_extension(pkg, cwd);
    }
    if let Some(repo) = name.strip_prefix("git:") {
        let global_git = crate::broker::util::get_pi_agent_dir().join("git").join(repo);
        let cwd_git = Path::new(cwd).join(".pi").join("git").join(repo);
        for dir in [global_git, cwd_git] {
            if let Some(f) = first_pi_extension(&dir) {
                return Some(f);
            }
        }
        return None;
    }

    let p = global_dir.join(format!("{}.ts", name));
    if p.is_file() {
        return Some(p);
    }

    let p = global_dir.join(name).join("index.ts");
    if p.is_file() {
        return Some(p);
    }

    let p = Path::new(cwd).join(".pi").join("extensions").join(format!("{}.ts", name));
    if p.is_file() {
        return Some(p);
    }

    let p = Path::new(cwd).join(".pi").join("extensions").join(name).join("index.ts");
    if p.is_file() {
        return Some(p);
    }

    None
}

/// Resolve project extensions to file paths for passing to pi via `-e` flags.
///
/// Reads already-resolved paths from the database. Logs a warning for any
/// extension whose path column is NULL or points to a missing file.
pub fn resolve_project_extensions(db: &super::db::Db, project_id: &str, cwd: &str) -> Vec<String> {
    let exts = db.get_project_extensions_with_paths(project_id);
    let mut paths = Vec::new();
    for (name, path) in &exts {
        match path {
            Some(p) if Path::new(p).is_file() => paths.push(p.clone()),
            _ => {
                // Path missing or file gone — try to re-resolve
                if let Some(resolved) = resolve_extension_name(name, cwd) {
                    let rp = resolved.to_string_lossy().to_string();
                    let _ = db.set_project_extension_path(project_id, name, &rp);
                    paths.push(rp);
                } else {
                    log::warn!(
                        "[project] extension '{}' not found (project={}, cwd={})",
                        name, project_id, cwd
                    );
                }
            }
        }
    }
    paths
}
