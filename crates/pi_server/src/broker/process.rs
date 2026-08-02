//! Process management: spawn, kill, reader/writer threads.
//!
//! Uses a builder pattern for idiomatic usage:
//!
//! ```ignore
//! let instance_id = SpawnBuilder::new(inner, event_tx, pi_exe, static_dir, pi_version)
//!     .cwd("/project")
//!     .session("path.jsonl")
//!     .extensions(&["ext1", "ext2"])
//!     .run()?;
//! ```

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::mpsc;

use super::types::{BrokerInner, EventTx, PiInstance};
use super::util::{
    build_augmented_path, configure_child_process_for_windows, log_child_path_diagnostics,
    strip_verbatim_prefix,
};

use std::process::{Command, Stdio};

// ─── Spawn Builder ──────────────────────────────────────────────────────────

/// Builder for spawning a pi process instance.
pub struct SpawnBuilder {
    inner: Arc<BrokerInner>,
    event_tx: EventTx,
    pi_exe: PathBuf,
    static_dir: PathBuf,
    pi_version: String,
    cwd: Option<String>,
    session_path: Option<String>,
    extensions: Vec<String>,
    model: Option<String>,
    persistent: bool,
    id: Option<String>,
}

impl SpawnBuilder {
    pub fn new(
        inner: Arc<BrokerInner>,
        event_tx: EventTx,
        pi_exe: PathBuf,
        static_dir: PathBuf,
        pi_version: String,
        persistent: bool,
    ) -> Self {
        Self {
            inner,
            event_tx,
            pi_exe,
            static_dir,
            pi_version,
            cwd: None,
            session_path: None,
            extensions: Vec::new(),
            model: None,
            persistent,
            id: None,
        }
    }

    /// Set whether the instance should be persistent.
    pub fn persistent(mut self, persistent: bool) -> Self {
        self.persistent = persistent;
        self
    }

    /// Set the working directory for the pi process.
    pub fn cwd(mut self, cwd: &str) -> Self {
        self.cwd = Some(cwd.to_string());
        self
    }

    /// Set extensions to pass via `--extensions`.
    pub fn extensions(mut self, exts: &[String]) -> Self {
        self.extensions = exts.to_vec();
        self
    }

    /// Set the session path for the pi process.
    /// 只用当恢复对话确定了session file的路径时才调用，传给PiInstance
    pub fn session_path(mut self, session_path: &str) -> Self {
        self.session_path = Some(session_path.to_string());
        self
    }

    /// Use a specific ID instead of generating a new UUID.
    /// Used when resuming a session that already has a persisted instance_id.
    pub fn id(mut self, id: &str) -> Self {
        self.id = Some(id.to_string());
        self
    }

    /// Set the model for the pi process (format: "provider/modelId").
    pub fn model(mut self, model: &str) -> Self {
        self.model = Some(model.to_string());
        self
    }

    /// Execute the spawn and register the instance in the broker.
    ///
    /// Returns the `instance_id` (UUID).
    pub fn run(self) -> Result<String, String> {
        let instance_id = self.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let pi_exe_str = strip_verbatim_prefix(&self.pi_exe.to_string_lossy());
        let static_dir_str = strip_verbatim_prefix(&self.static_dir.to_string_lossy());
        let cwd = if let Some(path) = self.cwd {
            strip_verbatim_prefix(path.as_str())
        } else {
            ".".to_string()
        };


        // ── Build pi CLI args ─────────────────────────────────────────
        let mut args: Vec<String> = vec!["--mode".into(), "rpc".into()];

        // Extensions are fully controlled by piter's whitelist: `-e` paths are
        // the only source pi loads (auto-discovery and settings packages are
        // ignored), which is what allows projects to exclude global extensions.
        args.push("--no-extensions".into());

        if let Some(ref sp) = self.session_path {
            args.push("--session".into());
            args.push(sp.clone());
        } else if !self.persistent {
            args.push("--no-session".into());
        }

        if !self.extensions.is_empty() {
            for ext in &self.extensions {
                args.push("-e".into());
                args.push(ext.clone());
            }
        }

        if let Some(ref m) = self.model {
            args.push("--model".into());
            args.push(m.clone());
        }

        log::info!(
            "[broker] spawning pi: id={} bin={} args={:?} cwd={} persistent={}",
            instance_id, pi_exe_str, args, cwd, self.persistent
        );

        let augmented_path = build_augmented_path();
        log_child_path_diagnostics("spawn", &augmented_path);

        let mut child_cmd = Command::new(&pi_exe_str);
        configure_child_process_for_windows(&mut child_cmd);
        child_cmd
            .args(&args)
            .current_dir(&cwd)
            .env("PATH", augmented_path)
            .env("PI_STUDIO_STATIC_DIR", &static_dir_str)
            .env("PI_STUDIO_PI_VERSION", &self.pi_version)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child = child_cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn pi ({}): {}", pi_exe_str, e))?;

        let stdout = child.stdout.take().ok_or_else(|| "No stdout".to_string())?;
        let mut stdin = child.stdin.take().ok_or_else(|| "No stdin".to_string())?;

        let running = Arc::new(AtomicBool::new(true));
        let running_r = running.clone();
        let running_w = running.clone();
        let inner = self.inner.clone();
        let event_tx = self.event_tx.clone();
        let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<String>();

        // ── Reader thread: pi stdout → event broadcast ────────────────
        let inner_clone = inner.clone();
        let iid = instance_id.clone();

        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if !running_r.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(text) = line else { continue };
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
                    continue;
                };

                let event_type = val.get("type").and_then(Value::as_str).unwrap_or("");

                // Pending RPC response — parse as typed Response
                if event_type == "response" {
                    if let Ok(resp) = pi_rpc::event::Response::from_json_line(trimmed) {
                        if let Some(ref req_id) = resp.id {
                            if let Some(pending) = inner_clone.pending_rpc.lock().remove(req_id) {
                                log::debug!("[broker] RPC response for id={}", req_id);
                                let _ = pending.sender.send(resp);
                            }
                        }
                    }
                }

                // Inject instanceId and broadcast raw JSON
                let mut event = val;
                if let Value::Object(ref mut map) = event {
                    map.insert(
                        "instanceId".to_string(),
                        Value::String(iid.clone()),
                    );
                }
                let _ = event_tx.send(event.to_string());
            }
            running_r.store(false, Ordering::SeqCst);
        });

        // ── Writer thread: stdin_rx → pi stdin ────────────────────────
        std::thread::spawn(move || {
            loop {
                match stdin_rx.try_recv() {
                    Ok(mut cmd) => {
                        if !running_w.load(Ordering::SeqCst) {
                            break;
                        }
                        if !cmd.ends_with('\n') {
                            cmd.push('\n');
                        }
                        if stdin.write_all(cmd.as_bytes()).is_err() {
                            log::error!("[broker] failed to write to pi stdin");
                            break;
                        }
                        if stdin.flush().is_err() {
                            log::error!("[broker] failed to flush pi stdin");
                            break;
                        }
                    }
                    Err(mpsc::error::TryRecvError::Empty) => {
                        if !running_w.load(Ordering::SeqCst) {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        log::info!("[broker] command channel closed, writer exiting");
                        break;
                    }
                }
            }
            running_w.store(false, Ordering::SeqCst);
        });

        let instance = PiInstance {
            id: instance_id.clone(),
            child,
            running,
            stdin_tx: Some(stdin_tx),
            session_path: self.session_path,
            persistent: self.persistent,
            cwd,
            created_at: std::time::Instant::now(),
        };

        // Register in broker
        self.inner.instances.lock().insert(instance_id.clone(), instance);
        Ok(instance_id)
    }
}
