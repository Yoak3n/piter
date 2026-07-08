//! Process management: spawn, kill, port/route management, LAN discovery.

use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::mpsc;

use super::super::PiBroker;
use super::types::BrokerState;

use super::util::{
    build_augmented_path, configure_child_process_for_windows, is_port_in_use, is_private_ipv4,
    log_child_path_diagnostics, strip_verbatim_prefix,
};

use std::process::{Command, Stdio};

// ─── PiBroker methods (only used by init.rs with Arc<PiBroker>) ──────────

impl PiBroker {
    /// Kill all pi processes.
    pub fn kill_all(&self) {
        let mut processes = self.inner.pi_processes.lock();
        for (_, mut process) in processes.drain() {
            process.running.store(false, Ordering::SeqCst);
            let _ = process.child.kill();
        }
        log::info!("[broker] all pi processes stopped");
    }

    // ─── LAN IP Discovery ────────────────────────────────────────────────

    pub fn discover_lan_ips() -> Vec<String> {
        let mut ips: Vec<String> = Vec::new();

        // Primary: UDP socket trick — connect to public DNS to find route IP
        if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if socket.connect("8.8.8.8:53").is_ok() {
                if let Ok(addr) = socket.local_addr() {
                    let ip = addr.ip();
                    if is_private_ipv4(ip) {
                        ips.push(ip.to_string());
                    }
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            if let Ok(output) = std::process::Command::new("ipconfig").output() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let line = line.trim();
                    if line.contains("IPv4") && line.contains(':') {
                        if let Some(ip_str) = line.split(':').next_back() {
                            let ip_str = ip_str.trim();
                            if let Ok(addr) = ip_str.parse::<std::net::IpAddr>() {
                                if is_private_ipv4(addr) && !ips.contains(&ip_str.to_string()) {
                                    ips.push(ip_str.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            for (cmd, args) in &[("ifconfig", &["-a"] as &[&str]), ("ip", &["addr"])] {
                if let Ok(output) = std::process::Command::new(cmd).args(args).output() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    for line in stdout.lines() {
                        let trimmed = line.trim();
                        if let Some(inet) = trimmed.strip_prefix("inet ") {
                            if let Some(ip_part) = inet.split_whitespace().next() {
                                if let Ok(addr) = ip_part.parse::<std::net::IpAddr>() {
                                    if is_private_ipv4(addr) && !ips.contains(&ip_part.to_string()) {
                                        ips.push(ip_part.to_string());
                                    }
                                }
                            }
                        }
                    }
                    if !ips.is_empty() {
                        break;
                    }
                }
            }
        }

        ips
    }
}

// ─── BrokerState methods (used by ws.rs with Arc<BrokerState>) ────────────

impl BrokerState {
    /// Get the next available port.
    pub fn next_port(&self) -> u16 {
        let processes = self.inner.pi_processes.lock();
        let mut port = 47821u16;
        while processes.contains_key(&port) || is_port_in_use(port) {
            port += 1;
        }
        port
    }

    /// Set the active port.
    pub fn set_active_port(&self, port: u16) {
        *self.inner.active_port.lock() = Some(port);
    }

    /// Spawn a new pi process on the given port.
    ///
    /// If `session_path` is provided, pi will be started with `--session <path>`
    /// so it can immediately resume that session.
    pub fn spawn_pi(
        &self,
        cwd: &str,
        port: u16,
        session_path: Option<&str>,
    ) -> Result<(), String> {
        let pi_exe_str = strip_verbatim_prefix(&self.pi_exe.to_string_lossy());
        let static_dir = strip_verbatim_prefix(&self.static_dir.to_string_lossy());
        let cwd = strip_verbatim_prefix(cwd);

        let mut args: Vec<String> = vec![
            "--mode".to_string(),
            "rpc".to_string(),
        ];
        if let Some(session) = session_path {
            args.push("--session".to_string());
            args.push(session.to_string());
        }

        log::info!(
            "[broker] spawning pi: bin={} args={:?} cwd={} port={} static_dir={}",
            pi_exe_str, args, cwd, port, static_dir
        );

        let augmented_path = build_augmented_path();
        log_child_path_diagnostics("spawn", &augmented_path);

        let mut child_cmd = Command::new(&pi_exe_str);
        configure_child_process_for_windows(&mut child_cmd);
        child_cmd
            .args(&args)
            .current_dir(&cwd)
            .env("PATH", augmented_path)
            .env("PI_STUDIO_STATIC_DIR", &static_dir)
            .env("PI_STUDIO_PORT", port.to_string())
            .env("PI_STUDIO_PI_VERSION", &self.pi_version)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child = child_cmd.spawn().map_err(|e| {
            format!("Failed to spawn embedded pi ({}): {}", pi_exe_str, e)
        })?;

        let stdout = child.stdout.take().ok_or_else(|| "No stdout".to_string())?;
        let mut stdin = child.stdin.take().ok_or_else(|| "No stdin".to_string())?;

        let running = Arc::new(AtomicBool::new(true));
        let running_r = running.clone();
        let running_w = running.clone();
        let inner = self.inner.clone();
        let event_tx = self.event_tx.clone();
        let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<String>();

        // Reader thread: pi stdout → event_tx broadcast
        let inner_clone = inner.clone();
        let port_clone = port;

        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if !running_r.load(Ordering::SeqCst) {
                    break;
                }
                if let Ok(text) = line {
                    let trimmed = text.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
                        // Pending RPC — match by command type in response events
                        let event_type = val.get("type").and_then(Value::as_str);
                        if event_type == Some("response") {
                            let command = val.get("command").and_then(Value::as_str);
                            if let Some(cmd) = command {
                                if let Some(tx) = inner_clone.pending_rpc.lock().remove(cmd) {
                                    log::debug!("[broker] RPC response for command={}", cmd);
                                    let _ = tx.send(val.clone());
                                }
                            }
                        }

                        // Learn route table (session_id → port)
                        if let Some(session_id) = super::util::extract_session_id(&val) {
                            log::debug!("[broker] learn route session_id={} -> port={}", session_id, port_clone);
                            inner_clone.routes.lock().insert(session_id.to_string(), port_clone);
                        }

                        // Update port_sessions ONLY on confirmed session switch.
                        // This prevents old streaming output from being mis-labeled
                        // when the user switches while a response is still streaming.
                        let event_type = val.get("type").and_then(Value::as_str);
                        if event_type == Some("response") {
                            let command = val.get("command").and_then(Value::as_str);
                            let success = val.get("success").and_then(Value::as_bool).unwrap_or(false);
                            if success && matches!(command, Some("switch_session") | Some("new_session")) {
                                if let Some(sf) = val.get("data")
                                    .and_then(|d| d.get("sessionFile"))
                                    .and_then(Value::as_str)
                                {
                                    log::info!("[broker] pi confirmed session: port={} session={}", port_clone, sf);
                                    inner_clone.port_sessions.lock().insert(port_clone, sf.to_string());
                                    inner_clone.routes.lock().insert(sf.to_string(), port_clone);
                                }
                            }
                        }

                        // Tag event with sessionPath so frontend can filter
                        let session_path = inner_clone.port_sessions.lock().get(&port_clone).cloned();
                        if let Some(sp) = session_path {
                            let wrapped = serde_json::json!({
                                "sessionPath": sp,
                                "payload": val,
                            });
                            let _ = event_tx.send(wrapped.to_string());
                        } else {
                            let _ = event_tx.send(text);
                        }
                    }
                }
            }
            running_r.store(false, Ordering::SeqCst);
        });

        // Writer thread: stdin_rx → pi stdin
        std::thread::spawn(move || {
            loop {
                match stdin_rx.try_recv() {
                    Ok(mut cmd) => {
                        if !running_w.load(Ordering::SeqCst) { break; }
                        if !cmd.ends_with('\n') { cmd.push('\n'); }
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
                        if !running_w.load(Ordering::SeqCst) { break; }
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

        let pi_process = super::types::PiProcess {
            child,
            running,
            stdin_tx: Some(stdin_tx),
        };

        self.inner.pi_processes.lock().insert(port, pi_process);
        log::info!("[broker] pi started on port {}", port);

        Ok(())
    }
}
