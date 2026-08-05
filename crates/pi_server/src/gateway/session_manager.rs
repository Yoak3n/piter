//! Session Manager — in-memory message tracking, breakpoint snapshots, idle lifecycle.
//!
//! Sessions are keyed by `instance_id` (our UUID for the pi process).
//! The frontend only uses `instance_id` — real session file paths are internal.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::GatewayState;

// ─── Tracked Event Types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum TrackedEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: Option<Value> },
    #[serde(rename = "message_update")]
    MessageUpdate { message: Option<Value> },
    #[serde(rename = "message_end")]
    MessageEnd { message: Option<Value> },
    #[serde(rename = "mirror_sync")]
    MirrorSync { messages: Option<Vec<Value>> },
    #[serde(rename = "turn_end")]
    TurnEnd,
    #[serde(rename = "agent_end")]
    AgentEnd,
    #[serde(rename = "tool_execution_start")]
    ToolExecutionStart {
        #[serde(rename = "toolCallId")]
        tool_call_id: Option<String>,
        #[serde(rename = "toolName")]
        tool_name: Option<String>,
    },
    #[serde(other)]
    Other,
}

// ─── Configuration ──────────────────────────────────────────────────────────

const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 600;

// ─── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum SessionActivity {
    Idle,
    Busy,
    WaitingReview,
    Unloaded,
}

/// Pi's session state from `get_state` response.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PiSessionState {
    pub session_file: Option<String>,
    pub session_id: Option<String>,
    pub session_name: Option<String>,
    pub model_id: Option<String>,
    pub model_name: Option<String>,
    pub model_provider: Option<String>,
    pub thinking_level: Option<String>,
    pub is_streaming: bool,
    pub is_compacting: bool,
    pub message_count: u32,
    pub pending_message_count: u32,
    pub context_window: Option<u32>,
}

pub struct ManagedSession {
    pub instance_id: String,
    pub cwd: String,
    pub activity: SessionActivity,
    pub disconnected_since: Option<Instant>,
    pub messages: Vec<Value>,
    pub partial_message: Option<Value>,
    pub subscribers: HashSet<u64>,
    pub last_active: Instant,
    /// Wall-clock epoch seconds for the last activity (for API responses).
    pub last_active_epoch: u64,
    pub message_seq: u64,
    pub pi_state: Option<PiSessionState>,
    /// Auto-generated or user-set session name.
    pub session_name: Option<String>,
    /// Number of completed turns (for auto-title timing).
    turn_count: u32,
    /// Whether a title has been generated/set.
    title_set: bool,
    /// Captured user message texts for title generation.
    title_candidates: Vec<String>,
}

pub struct SessionManager {
    /// Keyed by instance_id.
    pub sessions: HashMap<String, ManagedSession>,
    pub idle_timeout: Duration,
    /// Pending DB links: instance_id → project_id.
    /// Resolved when `get_state` response provides the sessionFile path.
    pub pending_links: HashMap<String, String>,
    /// Set when session state changes; caller checks via `take_dirty()`.
    dirty: bool,
    /// Session names that need to be persisted to DB.
    /// Drained by the event loop. Vec of (instance_id, name).
    pending_names: Vec<(String, String)>,
}

pub enum ActivateResult {
    Snapshot {
        messages: Vec<Value>,
        instance_id: String,
        message_seq: u64,
    },
    NeedSpawn {
        instance_id: String,
    },
}

pub enum SessionResult {
    Switched {
        instance_id: String,
        messages: Vec<Value>,
        message_seq: u64,
    },
    NeedSpawn {
        instance_id: String,
    },
}

/// Current wall-clock epoch seconds.
fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ─── Implementation ─────────────────────────────────────────────────────────

impl SessionManager {
    pub fn new(idle_timeout_secs: Option<u64>) -> Self {
        Self {
            sessions: HashMap::new(),
            idle_timeout: Duration::from_secs(
                idle_timeout_secs.unwrap_or(DEFAULT_IDLE_TIMEOUT_SECS),
            ),
            pending_links: HashMap::new(),
            dirty: false,
            pending_names: Vec::new(),
        }
    }

    // ── Session Lifecycle ──────────────────────────────────────────────

    /// Create a new session: spawn pi, register in manager.
    /// Returns `(instance_id, session_dir)`.
    pub fn create_session(
        sm: &Arc<parking_lot::Mutex<SessionManager>>,
        gw: &GatewayState,
        cwd: &str,
        name: &str,
        client_id: u64,
        model: Option<&str>,
    ) -> Result<String, String> {
        // Resolve project: reuse existing (cwd, name) or create a new one
        let effective_project_id = if let Some(existing) = gw.db.find_project_by_cwd_and_name(cwd, name) {
            existing.id
        } else {
            match super::project::create_project(&gw.db, name, cwd, Vec::new()) {
                Ok(proj) => {
                    log::info!("[session_manager] auto-created project '{}' for cwd={}", proj.name, cwd);
                    proj.id
                }
                Err(e) => {
                    log::warn!("[session_manager] auto-create project failed: {}", e);
                    return Err(e.to_string());
                }
            }
        };

        let extensions = super::project::effective_project_extensions(&gw.db, &effective_project_id, cwd);

        let instance_id = super::handlers::pi::spawn_persistent_for_gateway(gw, cwd, &extensions, model)?;

        // Register session in DB (session_path filled later by get_state response)
        let _ = gw.db.register_session(&instance_id, cwd, Some(effective_project_id.as_str()));
        // the actual file via get_state response in the event loop.

        {
            let mut mgr = sm.lock();
            mgr.pending_links.insert(instance_id.clone(), effective_project_id);

            mgr.sessions.insert(
                instance_id.clone(),
                ManagedSession {
                    instance_id: instance_id.clone(),
                    cwd: cwd.to_string(),
                    activity: SessionActivity::Idle,
                    disconnected_since: None,
                    messages: Vec::new(),
                    partial_message: None,
                    subscribers: {
                        let mut s = HashSet::new();
                        s.insert(client_id);
                        s
                    },
                    last_active: Instant::now(),
                    last_active_epoch: now_epoch(),
                    message_seq: 0,
                    pi_state: None,
                    session_name: None,
                    turn_count: 0,
                    title_set: false,
                    title_candidates: Vec::new(),
                },
            );
            mgr.dirty = true;
        }

        log::info!(
            "[session_manager] created session {} for client {}",
            instance_id, client_id
        );

        Ok(instance_id)
    }

    /// Switch to an existing session by instance_id.
    pub fn switch_session(
        sm: &Arc<parking_lot::Mutex<SessionManager>>,
        instance_id: &str,
        client_id: u64,
    ) -> SessionResult {
        let existing = {
            let mgr = sm.lock();
            mgr.sessions.get(instance_id).map(|s| {
                (s.activity.clone(), s.messages.clone(), s.message_seq)
            })
        };

        match existing {
            Some((activity, messages, seq)) if activity != SessionActivity::Unloaded => {
                sm.lock().sessions.get_mut(instance_id).map(|s| {
                    s.subscribers.insert(client_id);
                    s.disconnected_since = None;
                    s.last_active = Instant::now();
                    s.last_active_epoch = now_epoch();
                });

                SessionResult::Switched {
                    instance_id: instance_id.to_string(),
                    messages,
                    message_seq: seq,
                }
            }
            _ => {
                SessionResult::NeedSpawn {
                    instance_id: instance_id.to_string(),
                }
            }
        }
    }

    /// Register a newly spawned instance (for switch_session NeedSpawn path).
    pub fn register_instance(
        sm: &Arc<parking_lot::Mutex<SessionManager>>,
        instance_id: &str,
        cwd: &str,
        client_id: u64,
    ) {
        {
            let mut mgr = sm.lock();
            mgr.sessions.insert(
                instance_id.to_string(),
                ManagedSession {
                    instance_id: instance_id.to_string(),
                    cwd: cwd.to_string(),
                    activity: SessionActivity::Idle,
                    disconnected_since: None,
                    messages: Vec::new(),
                    partial_message: None,
                    subscribers: {
                        let mut s = HashSet::new();
                        s.insert(client_id);
                        s
                    },
                    last_active: Instant::now(),
                    last_active_epoch: now_epoch(),
                    message_seq: 0,
                    pi_state: None,
                    session_name: None,
                    turn_count: 0,
                    title_set: false,
                    title_candidates: Vec::new(),
                },
            );
            mgr.dirty = true;
        }

        // Routes registered from get_state response
    }

    // ── Activate / Deactivate ──────────────────────────────────────────

    pub fn activate(&mut self, instance_id: &str, client_id: u64) -> ActivateResult {
        let now = Instant::now();

        match self.sessions.get_mut(instance_id) {
            Some(session) if session.activity != SessionActivity::Unloaded => {
                let was_disconnected = session.disconnected_since.is_some();
                session.subscribers.insert(client_id);
                session.disconnected_since = None;
                session.last_active = now;
                session.last_active_epoch = now_epoch();

                if was_disconnected {
                    self.dirty = true;
                }

                ActivateResult::Snapshot {
                    messages: session.messages.clone(),
                    instance_id: instance_id.to_string(),
                    message_seq: session.message_seq,
                }
            }
            _ => ActivateResult::NeedSpawn {
                instance_id: instance_id.to_string(),
            },
        }
    }

    pub fn deactivate(&mut self, instance_id: &str, client_id: u64) {
        if let Some(session) = self.sessions.get_mut(instance_id) {
            session.subscribers.remove(&client_id);
            if session.subscribers.is_empty() && session.disconnected_since.is_none() {
                session.disconnected_since = Some(Instant::now());
                self.dirty = true;
            }
        }
    }

    pub fn deactivate_all_for_client(&mut self, client_id: u64) {
        for session in self.sessions.values_mut() {
            session.subscribers.remove(&client_id);
            if session.subscribers.is_empty() && session.disconnected_since.is_none() {
                session.disconnected_since = Some(Instant::now());
                self.dirty = true;
            }
        }
    }

    /// Check if state changed since last check, and clear the flag.
    pub fn take_dirty(&mut self) -> bool {
        let d = self.dirty;
        self.dirty = false;
        d
    }

    /// Mark session manager as dirty (state changed).
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Drain pending session names that need to be persisted to DB.
    pub fn take_pending_names(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.pending_names)
    }

    /// Set a user-provided session name for a loaded session.
    /// Marks the title as set so the auto-title logic won't override it.
    pub fn set_session_name(&mut self, instance_id: &str, name: String) {
        if let Some(session) = self.sessions.get_mut(instance_id) {
            session.session_name = Some(name);
            session.title_set = true;
        }
    }

    // ── Event Tracking ─────────────────────────────────────────────────

    /// Process a raw pi event and update in-memory message tracking.
    /// Returns `Some(message_seq)` if this event belongs to a tracked session.
    pub fn on_event(&mut self, event: &Value, instance_id: &str) -> Option<u64> {
        let session = self.sessions.get_mut(instance_id)?;
        if session.activity == SessionActivity::Unloaded {
            return Some(0);
        }

        session.last_active = Instant::now();
        session.last_active_epoch = now_epoch();

        let tracked: TrackedEvent = serde_json::from_value(event.clone()).ok()?;

        match tracked {
            TrackedEvent::MessageStart { message } => {
                let role = message
                    .as_ref()
                    .and_then(|m| m.get("role"))
                    .and_then(Value::as_str)
                    .unwrap_or("assistant");
                session.partial_message = Some(serde_json::json!({
                    "role": role,
                    "content": "",
                }));
                if session.activity != SessionActivity::Busy {
                    session.activity = SessionActivity::Busy;
                    self.dirty = true;
                }
            }

            TrackedEvent::MessageUpdate { message } => {
                if let Some(msg) = message {
                    session.partial_message = Some(msg);
                }
            }

            TrackedEvent::MessageEnd { message } => {
                // Eagerly consume the partial message. pi's message_end always
                // carries the full message object, so `or_else` never executed
                // `partial_message.take()`, leaving a residual copy that was
                // pushed a second time on turn_end/agent_end (duplicate final
                // answers). `Option::or` takes ownership of both, so the partial
                // is gone regardless of which side wins.
                let partial = session.partial_message.take();
                let msg = message.or(partial);
                if let Some(ref m) = msg {
                    // Capture user messages for auto-title
                    let role = m.get("role").and_then(Value::as_str).unwrap_or("");
                    if role == "user" {
                        if session.activity != SessionActivity::Busy {
                            session.activity = SessionActivity::Busy;
                            self.dirty = true;
                        }
                        if !session.title_set {
                            let text = extract_message_text(m);
                            if text.len() >= 10 {
                                session.title_candidates.push(text);
                            }
                        }
                    }
                    session.messages.push(m.clone());
                    session.message_seq += 1;
                }
            }

            TrackedEvent::MirrorSync { messages } => {
                if let Some(msgs) = messages {
                    session.messages = msgs;
                    session.partial_message = None;
                    session.message_seq = session.messages.len() as u64;
                }
            }

            TrackedEvent::TurnEnd | TrackedEvent::AgentEnd => {
                // If someone is viewing this session, go directly to Idle;
                // otherwise mark WaitingReview until the user switches to it.
                let next = if session.subscribers.is_empty() {
                    SessionActivity::WaitingReview
                } else {
                    SessionActivity::Idle
                };
                if session.activity != next {
                    session.activity = next;
                    self.dirty = true;
                }

                if let Some(msg) = session.partial_message.take() {
                    session.messages.push(msg);
                    session.message_seq += 1;
                }

                // Auto-title: after 2 turns, generate a session name from user messages
                if matches!(tracked, TrackedEvent::TurnEnd) {
                    session.turn_count += 1;
                    if !session.title_set
                        && session.turn_count >= 2
                        && !session.title_candidates.is_empty()
                    {
                        if let Some(title) = generate_session_title(&session.title_candidates) {
                            log::info!(
                                "[session_manager] auto-title for {}: {}",
                                session.instance_id, title
                            );
                            session.session_name = Some(title.clone());
                            session.title_set = true;
                            self.dirty = true;
                            self.pending_names.push((session.instance_id.clone(), title));
                        }
                    }
                }
            }

            TrackedEvent::ToolExecutionStart { tool_call_id, tool_name } => {
                if let Some(ref mut partial) = session.partial_message {
                    let id = tool_call_id.as_deref().unwrap_or("");
                    let name = tool_name.as_deref().unwrap_or("");
                    if let Some(execs) = partial.get_mut("tool_executions") {
                        if let Some(arr) = execs.as_array_mut() {
                            arr.push(serde_json::json!({"id": id, "name": name, "status": "running"}));
                        }
                    } else {
                        partial["tool_executions"] = serde_json::json!([{"id": id, "name": name, "status": "running"}]);
                    }
                }
            }

            TrackedEvent::Other => {}
        }

        Some(self.sessions.get(instance_id)?.message_seq)
    }

    // ── Queries ────────────────────────────────────────────────────────

    pub fn has_subscribers(&self, instance_id: &str) -> bool {
        self.sessions
            .get(instance_id)
            .map(|s| !s.subscribers.is_empty())
            .unwrap_or(false)
    }

    pub fn update_pi_state(&mut self, instance_id: &str, pi_state: PiSessionState) {
        if let Some(session) = self.sessions.get_mut(instance_id) {
            log::info!(
                "[session_manager] pi_state updated for {}: file={:?}, model={:?}, msgs={}",
                instance_id, pi_state.session_file, pi_state.model_id, pi_state.message_count,
            );
            session.pi_state = Some(pi_state);
            self.dirty = true;
        }
    }

    // ── Cleanup ────────────────────────────────────────────────────────

    pub fn find_expired_sessions(&self) -> Vec<String> {
        let now = Instant::now();
        self.sessions
            .values()
            .filter_map(|s| {
                // 已卸载的会话不应再次过期：mark_unloaded 只改 activity，
                // disconnected_since 仍在，否则每轮 cleanup 都会重复命中。
                if s.activity == SessionActivity::Unloaded {
                    return None;
                }
                if let Some(since) = &s.disconnected_since {
                    if now.duration_since(*since) > self.idle_timeout {
                        return Some(s.instance_id.clone());
                    }
                }
                None
            })
            .collect()
    }

    pub fn mark_unloaded(&mut self, instance_ids: &[String]) {
        for iid in instance_ids {
            if let Some(session) = self.sessions.get_mut(iid) {
                session.activity = SessionActivity::Unloaded;
                session.messages.clear();
                session.partial_message = None;
                self.dirty = true;
            }
        }
    }

    pub fn set_idle_timeout(&mut self, secs: u64) {
        self.idle_timeout = Duration::from_secs(secs);
    }
}

// ─── Auto-title helpers ────────────────────────────────────────────────────

/// Extract plain text from a user message (handles string or content blocks).
fn extract_message_text(msg: &Value) -> String {
    let content = match msg.get("content") {
        Some(c) => c,
        None => return String::new(),
    };
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        return arr
            .iter()
            .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n");
    }
    String::new()
}

/// Generate a session title from captured user messages (Picot-style heuristic).
fn generate_session_title(messages: &[String]) -> Option<String> {
    static GREETING_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static OPENER_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();

    let greeting = GREETING_RE.get_or_init(|| {
        regex::Regex::new(r"^(hey|hello|hi|morning|good morning|howdy|yo|sup)[\s!.:,]*$").unwrap()
    });
    let opener = OPENER_RE.get_or_init(|| {
        regex::Regex::new(r"(?i)^(ok|okay|so|actually|hey|please|can you|could you|i want(?:ed)? to|i wanna|let'?s)\s+").unwrap()
    });

    // Find first substantive message
    let text = messages
        .iter()
        .find(|m| {
            let trimmed = m.trim();
            !trimmed.is_empty()
                && !greeting.is_match(trimmed)
                && !trimmed.to_lowercase().starts_with("read your memory")
                && !trimmed.to_lowercase().starts_with("read your seed")
                && trimmed.len() >= 10
        })
        .or_else(|| messages.first())?;

    // Strip conversational openers
    let cleaned = opener.replace(text.trim(), "").to_string();
    let first_line = cleaned.lines().next().unwrap_or(&cleaned);

    // Extract first sentence (boundary between char 10-80)
    let char_count = first_line.chars().count();
    let start = 10.min(char_count);
    let title = if let Some(pos) = first_line.chars().skip(start)
        .position(|c| c == '.' || c == '!' || c == '?')
    {
        let end = start + pos + 1;
        first_line.chars().take(end.min(char_count)).collect::<String>()
    } else {
        first_line.to_string()
    };

    // Truncate at 60 chars
    let title = if title.chars().count() > 60 {
        let truncated: String = title.chars().take(57).collect();
        let cut = truncated.rfind(' ').unwrap_or(truncated.len());
        format!("{}…", &truncated[..cut])
    } else {
        title
    };

    // Capitalize first letter
    let mut chars = title.chars();
    let first = chars.next()?;
    let capitalized: String = first.to_uppercase().collect::<String>() + chars.as_str();

    if capitalized.is_empty() {
        None
    } else {
        Some(capitalized)
    }
}

// ─── Cleanup Task ───────────────────────────────────────────────────────────

pub fn spawn_cleanup_task(
    session_manager: Arc<parking_lot::Mutex<SessionManager>>,
    inner: Arc<crate::broker::types::BrokerInner>,
    event_tx: crate::broker::types::EventTx,
    cleanup_interval: Duration,
) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(cleanup_interval);

            let expired = session_manager.lock().find_expired_sessions();
            if expired.is_empty() {
                continue;
            }

            log::info!(
                "[session_manager] cleanup: {} sessions expired",
                expired.len()
            );

            for iid in &expired {
                let mut instances = inner.instances.lock();
                if let Some(mut inst) = instances.remove(iid) {
                    use std::sync::atomic::Ordering;
                    inst.running.store(false, Ordering::SeqCst);
                    let _ = inst.child.kill();
                }
            }

            // Mark unloaded (sets dirty flag)
            session_manager.lock().mark_unloaded(&expired);

            // Trigger event loop to push updated sessions list
            let _ = event_tx.send(
                serde_json::json!({"type": "session_cleanup"}).to_string()
            );
        }
    });
}
