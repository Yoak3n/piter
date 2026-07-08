use std::collections::HashMap;
use std::io::Write;
use std::process::{Child, ChildStdin};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;
use serde_json::Value;
use log::info;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;

/// Manages multiple `pi --mode rpc` subprocesses, keyed by session path.
/// Each session gets its own pi process (like Picot's architecture).
/// Switching sessions doesn't stop the old process — it keeps running.
pub struct PiRpcClient {
    processes: Arc<Mutex<HashMap<String, PiProcessState>>>,
    active_session: Arc<Mutex<Option<String>>>,
}

struct PiProcessState {
    child: Option<Child>,
    stdin: ChildStdin,
    running: Arc<AtomicBool>,
}

impl PiRpcClient {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
            active_session: Arc::new(Mutex::new(None)),
        }
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

    /// Stop a specific session's pi process.
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

    /// Check if any pi process is running.
    pub fn is_running(&self) -> bool {
        !self.processes.lock().is_empty()
    }

    /// Spawn a background thread that reads commands from an mpsc receiver
    /// and forwards them to the active pi process's stdin.
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
    fn default() -> Self { Self::new() }
}
