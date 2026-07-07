use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use log::info;
use tokio::sync::{broadcast, mpsc};
use tokio::sync::mpsc::error::TryRecvError;

/// Manages multiple `pi --mode rpc` subprocesses, keyed by session path.
/// Each session gets its own pi process (like Picot's architecture).
/// Switching sessions doesn't stop the old process — it keeps running.
pub struct PiRpcClient {
    processes: Arc<Mutex<HashMap<String, PiProcessState>>>,
    pi_exe: PathBuf,
    event_tx: Mutex<Option<broadcast::Sender<String>>>,
    active_session: Arc<Mutex<Option<String>>>,
}

struct PiProcessState {
    child: Option<Child>,
    stdin: ChildStdin,
    running: Arc<AtomicBool>,
}

impl PiRpcClient {
    pub fn new(pi_exe: PathBuf) -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
            pi_exe,
            event_tx: Mutex::new(None),
            active_session: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_event_channel(&self, tx: broadcast::Sender<String>) {
        *self.event_tx.lock() = Some(tx);
    }

    fn stub_extension_path() -> String {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("extensions")
            .join("stub.mjs");
        p.to_string_lossy().to_string()
    }

    #[cfg(target_os = "windows")]
    fn spawn_pi(session_path: Option<&str>) -> Result<(Child, String), String> {
        let mut args: Vec<String> = vec![
            "--extension".to_string(), Self::stub_extension_path(),
            "--mode".to_string(), "rpc".to_string(),
        ];
        if let Some(session) = session_path {
            args.push("--session".to_string());
            args.push(session.to_string());
        }
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let child = Command::new(Self::pi_exe_path())
            .args(&args).current_dir(".")
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit())
            .spawn().map_err(|e| format!("Failed to spawn pi: {}", e))?;
        let key = session_path.unwrap_or("default").to_string();
        Ok((child, key))
    }

    fn pi_exe_path() -> PathBuf {
        // This is set at construction time, but spawn_pi is static for testing
        PathBuf::from("pi")
    }

    pub fn start_or_switch(
        &self, app_handle: AppHandle, cwd: String, session_path: String,
    ) -> Result<(), String> {
        let mut procs = self.processes.lock();
        let key = session_path.clone();

        // Already running → just switch active
        if procs.contains_key(&key) {
            *self.active_session.lock() = Some(key);
            let _ = app_handle.emit("pi:event", serde_json::json!({"type": "session_switched", "sessionPath": &session_path}));
            return Ok(());
        }

        // Spawn new pi for this session
        let mut args: Vec<String> = vec![
            "--extension".to_string(), Self::stub_extension_path(),
            "--mode".to_string(), "rpc".to_string(),
        ];
        args.push("--session".to_string());
        args.push(session_path.clone());

        #[cfg(target_os = "windows")]
        use std::os::windows::process::CommandExt;
        #[cfg(target_os = "windows")]
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let mut child = if cfg!(target_os = "windows") {
            #[cfg(target_os = "windows")]
            { Command::new(&self.pi_exe).args(&args).current_dir(&cwd).creation_flags(CREATE_NO_WINDOW).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn() }
            #[cfg(not(target_os = "windows"))]
            { Err("unreachable".to_string()) }
        } else {
            Command::new(&self.pi_exe).args(&args).current_dir(&cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn()
        }.map_err(|e| format!("Failed to spawn pi: {}", e))?;

        let stdout = child.stdout.take().ok_or_else(|| "No stdout".to_string())?;
        let stdin = child.stdin.take().ok_or_else(|| "No stdin".to_string())?;

        let key = session_path.clone();
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        let ev_tx = self.event_tx.lock().clone();
        let app_clone = app_handle.clone();
        let session_key = key.clone();

        // Reader thread — tag all events with sessionPath
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if !running_clone.load(Ordering::SeqCst) { break; }
                if let Ok(text) = line {
                    let trimmed = text.trim();
                    if trimmed.is_empty() { continue; }
                    // Wrap with session identifier so frontend can filter
                    if let Ok(json) = serde_json::from_str::<Value>(trimmed) {
                        let wrapped = serde_json::json!({
                            "sessionPath": session_key,
                            "payload": json
                        });
                        if let Some(ref tx) = ev_tx { let _ = tx.send(wrapped.to_string()); }
                        let _ = app_clone.emit("pi:event", wrapped);
                    }
                }
            }
            let _exit_wrapped = serde_json::json!({"sessionPath": session_key, "payload": {"type": "pi_exited"}});
        });

        procs.insert(key.clone(), PiProcessState { child: Some(child), stdin, running });
        *self.active_session.lock() = Some(key);
        let _ = app_handle.emit("pi:event", serde_json::json!({"type": "pi_started", "sessionPath": &session_path}));
        info!("Pi session started: {}", session_path);
        Ok(())
    }

    /// Send a command to the active pi process.
    pub fn send(&self, command: Value) -> Result<(), String> {
        let active = self.active_session.lock().clone().unwrap_or_default();
        let mut procs = self.processes.lock();
        let proc = procs.get_mut(&active).ok_or_else(|| "No active pi session".to_string())?;
        let mut line = serde_json::to_string(&command).map_err(|e| format!("Serialize: {}", e))?;
        line.push('\n');
        proc.stdin.write_all(line.as_bytes()).map_err(|e| format!("Write error: {}", e))?;
        proc.stdin.flush().map_err(|e| format!("Flush error: {}", e))?;
        Ok(())
    }

    /// Stop a specific session's pi process (or all if no key given).
    pub fn stop_session(&self, key: &str) {
        let mut procs = self.processes.lock();
        if let Some(mut proc) = procs.remove(key) {
            proc.running.store(false, Ordering::SeqCst);
            let _ = proc.stdin.write_all(b"\n");
            let _ = proc.stdin.flush();
            if let Some(ref mut child) = proc.child { let _ = child.kill(); let _ = child.wait(); }
            info!("Pi session stopped: {}", key);
        }
        if self.active_session.lock().as_deref() == Some(key) {
            *self.active_session.lock() = procs.keys().next().cloned();
        }
    }

    pub fn stop_all(&self) {
        let keys: Vec<String> = self.processes.lock().keys().cloned().collect();
        for k in keys { self.stop_session(&k); }
    }

    /// Get the active session key.
    pub fn active_session_key(&self) -> Option<String> {
        self.active_session.lock().clone()
    }

    // ─── Backward-compat API (used by cmd.rs / init.rs) ────────────────

    /// Start pi for the default session (backward compat).
    pub fn start(
        &self, app_handle: AppHandle, cwd: String, session_path: Option<String>, _no_session: bool,
    ) -> Result<(), String> {
        let path = session_path.unwrap_or_else(|| format!("default-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()));
        self.start_or_switch(app_handle, cwd, path)
    }

    /// Stop all processes (backward compat).
    pub fn stop(&self) {
        self.stop_all();
    }

    /// Check if any pi process is running (backward compat).
    pub fn is_running(&self) -> bool {
        !self.processes.lock().is_empty()
    }

    /// Spawn a background thread that reads commands from the WS broker
    /// (`command_rx`) and forwards them to the active pi process's stdin.
    /// This bridges the async broker to the sync child process I/O.
    pub fn set_command_channel(&self, mut rx: mpsc::UnboundedReceiver<String>) {
        let procs = self.processes.clone();
        let active = self.active_session.clone();

        std::thread::spawn(move || {
            loop {
                match rx.try_recv() {
                    Ok(command) => {
                        let session_key = active.lock().clone().unwrap_or_default();
                        let mut procs_lock = procs.lock();
                        if let Some(state) = procs_lock.get_mut(&session_key) {
                            let mut line = command;
                            if !line.ends_with('\n') {
                                line.push('\n');
                            }
                            if let Err(e) = state.stdin.write_all(line.as_bytes()) {
                                log::error!("[client] failed to write to pi stdin: {}", e);
                                break;
                            }
                            if let Err(e) = state.stdin.flush() {
                                log::error!("[client] failed to flush pi stdin: {}", e);
                                break;
                            }
                        } else {
                            log::warn!(
                                "[client] no active session ({:?}) for incoming command",
                                session_key
                            );
                        }
                    }
                    Err(TryRecvError::Empty) => {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Err(TryRecvError::Disconnected) => {
                        log::info!("[client] command channel closed, exiting");
                        break;
                    }
                }
            }
        });
    }

    /// List all active session keys.
    pub fn list_sessions(&self) -> Vec<String> {
        self.processes.lock().keys().cloned().collect()
    }
}

impl Default for PiRpcClient {
    fn default() -> Self { Self::new(PathBuf::from("pi")) }
}
