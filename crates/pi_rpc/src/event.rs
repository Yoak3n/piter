//! Event types streamed from pi via stdout.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ext::ExtensionUiRequest;
use super::message::AgentMessage;

// ─── RPC Response (pi replies to commands) ──────────────────────────────────

/// A response from pi for a command. Sent as `{type: "response", command, success, ...}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    #[serde(rename = "type")]
    pub response_type: String, // always "response"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub command: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn from_json_line(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s.trim())
    }

    /// Whether this is a session-assigning response (new_session / switch_session).
    pub fn is_session_response(&self) -> bool {
        self.success
            && matches!(self.command.as_str(), "switch_session" | "new_session")
    }

    /// Extract the session file from a session response.
    pub fn session_file(&self) -> Option<&str> {
        self.data
            .as_ref()
            .and_then(|d| d.get("sessionFile").and_then(Value::as_str))
            .filter(|s| !s.is_empty())
    }
}

// ─── Event ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    // Lifecycle
    #[serde(rename = "agent_start")]
    AgentStart,
    #[serde(rename = "agent_end")]
    AgentEnd {
        messages: Vec<Value>,
        #[serde(rename = "willRetry")]
        will_retry: bool,
    },
    #[serde(rename = "agent_settled")]
    AgentSettled,

    // Turn
    #[serde(rename = "turn_start")]
    TurnStart,
    #[serde(rename = "turn_end")]
    TurnEnd {
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<Value>,
        #[serde(rename = "toolResults", skip_serializing_if = "Option::is_none")]
        tool_results: Option<Vec<Value>>,
    },

    // Message
    #[serde(rename = "message_start")]
    MessageStart {
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<Value>,
    },
    #[serde(rename = "message_update")]
    MessageUpdate {
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<Value>,
        #[serde(rename = "assistantMessageEvent", skip_serializing_if = "Option::is_none")]
        assistant_message_event: Option<AssistantMessageEvent>,
    },
    #[serde(rename = "message_end")]
    MessageEnd {
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<AgentMessage>,
    },

    // Bash
    #[serde(rename = "bash_execution_update")]
    BashExecutionUpdate {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        delta: String,
    },

    // Tool
    #[serde(rename = "tool_execution_start")]
    ToolExecutionStart {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        args: Option<Value>,
    },
    #[serde(rename = "tool_execution_update")]
    ToolExecutionUpdate {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        args: Option<Value>,
        #[serde(rename = "partialResult", skip_serializing_if = "Option::is_none")]
        partial_result: Option<Value>,
    },
    #[serde(rename = "tool_execution_end")]
    ToolExecutionEnd {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
        #[serde(rename = "isError", default)]
        is_error: bool,
    },

    // Queue
    #[serde(rename = "queue_update")]
    QueueUpdate {
        #[serde(default)]
        steering: Vec<String>,
        #[serde(rename = "followUp", default)]
        follow_up: Vec<String>,
    },

    // Compaction
    #[serde(rename = "compaction_start")]
    CompactionStart { reason: String },
    #[serde(rename = "compaction_end")]
    CompactionEnd {
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
        #[serde(default)]
        aborted: bool,
        #[serde(rename = "willRetry", default)]
        will_retry: bool,
    },

    // Retry
    #[serde(rename = "auto_retry_start")]
    AutoRetryStart {
        attempt: u32,
        #[serde(rename = "maxAttempts")]
        max_attempts: u32,
        #[serde(rename = "delayMs")]
        delay_ms: u64,
        #[serde(rename = "errorMessage")]
        error_message: String,
    },
    #[serde(rename = "auto_retry_end")]
    AutoRetryEnd {
        success: bool,
        attempt: u32,
        #[serde(rename = "finalError", skip_serializing_if = "Option::is_none")]
        final_error: Option<String>,
    },

    // Summarization retry
    #[serde(rename = "summarization_retry_scheduled")]
    SummarizationRetryScheduled {
        attempt: u32,
        #[serde(rename = "maxAttempts")]
        max_attempts: u32,
        #[serde(rename = "delayMs")]
        delay_ms: u64,
        #[serde(rename = "errorMessage")]
        error_message: String,
    },
    #[serde(rename = "summarization_retry_attempt_start")]
    SummarizationRetryAttemptStart {
        source: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    #[serde(rename = "summarization_retry_finished")]
    SummarizationRetryFinished,

    // Extension
    #[serde(rename = "extension_error")]
    ExtensionError {
        #[serde(rename = "extensionPath")]
        extension_path: String,
        event: String,
        error: String,
    },
    #[serde(rename = "extension_ui_request")]
    ExtensionUiRequest(ExtensionUiRequest),

    // Catch-all for unknown event types
    #[serde(other)]
    Unknown,
}

/// All known lifecycle event type strings, used by the gateway for envelope wrapping.
/// Keep in sync with the `Event` enum variants above (excluding `Unknown`).
pub const LIFECYCLE_EVENT_TYPES: &[&str] = &[
    // Lifecycle
    "agent_start",
    "agent_end",
    "agent_settled",
    // Turn
    "turn_start",
    "turn_end",
    // Message
    "message_start",
    "message_update",
    "message_end",
    // Bash
    "bash_execution_update",
    // Tool
    "tool_execution_start",
    "tool_execution_update",
    "tool_execution_end",
    // Queue
    "queue_update",
    // Compaction
    "compaction_start",
    "compaction_end",
    // Retry
    "auto_retry_start",
    "auto_retry_end",
    // Summarization retry
    "summarization_retry_scheduled",
    "summarization_retry_attempt_start",
    "summarization_retry_finished",
    // Extension
    "extension_error",
    "extension_ui_request",
];

impl Event {
    /// Parse an event from a JSON line string.
    pub fn from_json_line(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s.trim())
    }

    /// Serialize to a JSON line (with trailing newline).
    pub fn to_json_line(&self) -> String {
        let mut s = serde_json::to_string(self).unwrap_or_default();
        s.push('\n');
        s
    }
}

/// Helper: extract `instanceId` from a raw JSON value (injected by broker reader thread).
pub fn extract_instance_id(val: &Value) -> Option<&str> {
    val.get("instanceId").and_then(Value::as_str)
}

/// Helper: extract `sessionId` / `sessionFile` from a raw JSON value.
pub fn extract_session_id(val: &Value) -> Option<&str> {
    val.get("sessionId")
        .and_then(Value::as_str)
        .or_else(|| val.get("sessionFile").and_then(Value::as_str))
}

// ─── Assistant Message Event (streaming delta) ──────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessageEvent {
    #[serde(rename = "type")]
    pub event_type: AssistantEventType,
    #[serde(rename = "contentIndex", skip_serializing_if = "Option::is_none")]
    pub content_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(rename = "toolCall", skip_serializing_if = "Option::is_none")]
    pub tool_call: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssistantEventType {
    #[serde(rename = "start")]
    Start,
    #[serde(rename = "text_start")]
    TextStart,
    #[serde(rename = "text_delta")]
    TextDelta,
    #[serde(rename = "text_end")]
    TextEnd,
    #[serde(rename = "thinking_start")]
    ThinkingStart,
    #[serde(rename = "thinking_delta")]
    ThinkingDelta,
    #[serde(rename = "thinking_end")]
    ThinkingEnd,
    #[serde(rename = "toolcall_start")]
    ToolCallStart,
    #[serde(rename = "toolcall_delta")]
    ToolCallDelta,
    #[serde(rename = "toolcall_end")]
    ToolCallEnd,
    #[serde(rename = "done")]
    Done,
    #[serde(rename = "error")]
    Error,
}
