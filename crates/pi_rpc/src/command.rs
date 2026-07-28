//! RPC command types sent to pi via stdin.

use serde::{Deserialize, Serialize};

use super::model::{ImageContent, QueueMode, StreamingBehavior, ThinkingLevel};

// ─── Command ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Command {
    // ── Prompting ──────────────────────────────────────────────────────
    #[serde(rename = "prompt")]
    Prompt {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<ImageContent>,
        #[serde(rename = "streamingBehavior", skip_serializing_if = "Option::is_none")]
        streaming_behavior: Option<StreamingBehavior>,
    },

    #[serde(rename = "steer")]
    Steer {
        message: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<ImageContent>,
    },

    #[serde(rename = "follow_up")]
    FollowUp {
        message: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<ImageContent>,
    },

    #[serde(rename = "abort")]
    Abort,

    // ── State ──────────────────────────────────────────────────────────
    #[serde(rename = "get_state")]
    GetState,

    #[serde(rename = "get_messages")]
    GetMessages,

    // ── Model ──────────────────────────────────────────────────────────
    #[serde(rename = "set_model")]
    SetModel {
        provider: String,
        #[serde(rename = "modelId")]
        model_id: String,
    },

    #[serde(rename = "cycle_model")]
    CycleModel,

    #[serde(rename = "get_available_models")]
    GetAvailableModels,

    // ── Thinking ───────────────────────────────────────────────────────
    #[serde(rename = "set_thinking_level")]
    SetThinkingLevel { level: ThinkingLevel },

    #[serde(rename = "cycle_thinking_level")]
    CycleThinkingLevel,

    #[serde(rename = "get_available_thinking_levels")]
    GetAvailableThinkingLevels,

    // ── Queue Modes ────────────────────────────────────────────────────
    #[serde(rename = "set_steering_mode")]
    SetSteeringMode { mode: QueueMode },

    #[serde(rename = "set_follow_up_mode")]
    SetFollowUpMode { mode: QueueMode },

    // ── Compaction ─────────────────────────────────────────────────────
    #[serde(rename = "compact")]
    Compact {
        #[serde(rename = "customInstructions", skip_serializing_if = "Option::is_none")]
        custom_instructions: Option<String>,
    },

    #[serde(rename = "set_auto_compaction")]
    SetAutoCompaction { enabled: bool },

    // ── Retry ──────────────────────────────────────────────────────────
    #[serde(rename = "set_auto_retry")]
    SetAutoRetry { enabled: bool },

    #[serde(rename = "abort_retry")]
    AbortRetry,

    // ── Bash ───────────────────────────────────────────────────────────
    #[serde(rename = "bash")]
    Bash {
        command: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },

    #[serde(rename = "abort_bash")]
    AbortBash,

    // ── Session ────────────────────────────────────────────────────────
    #[serde(rename = "get_session_stats")]
    GetSessionStats,

    #[serde(rename = "export_html")]
    ExportHtml {
        #[serde(rename = "outputPath", skip_serializing_if = "Option::is_none")]
        output_path: Option<String>,
    },

    #[serde(rename = "switch_session")]
    SwitchSession {
        #[serde(rename = "sessionPath")]
        session_path: String,
    },

    #[serde(rename = "fork")]
    Fork {
        #[serde(rename = "entryId")]
        entry_id: String,
    },

    #[serde(rename = "clone")]
    Clone,

    #[serde(rename = "get_fork_messages")]
    GetForkMessages,

    #[serde(rename = "get_entries")]
    GetEntries {
        #[serde(skip_serializing_if = "Option::is_none")]
        since: Option<String>,
    },

    #[serde(rename = "get_tree")]
    GetTree,

    #[serde(rename = "get_last_assistant_text")]
    GetLastAssistantText,

    #[serde(rename = "set_session_name")]
    SetSessionName { name: String },

    // ── Commands ───────────────────────────────────────────────────────
    #[serde(rename = "get_commands")]
    GetCommands,
}

// ─── Builder shortcuts ──────────────────────────────────────────────────────

impl Command {
    /// Serialize to a JSON line (with trailing newline).
    pub fn to_json_line(&self) -> String {
        let mut s = serde_json::to_string(self).unwrap_or_default();
        s.push('\n');
        s
    }

    /// Parse a command from a JSON line string.
    pub fn from_json_line(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s.trim())
    }

    // ── Convenience constructors ───────────────────────────────────────

    pub fn prompt(msg: impl Into<String>) -> Self {
        Self::Prompt {
            message: msg.into(),
            id: None,
            images: Vec::new(),
            streaming_behavior: None,
        }
    }

    pub fn steer(msg: impl Into<String>) -> Self {
        Self::Steer {
            message: msg.into(),
            images: Vec::new(),
        }
    }

    pub fn follow_up(msg: impl Into<String>) -> Self {
        Self::FollowUp {
            message: msg.into(),
            images: Vec::new(),
        }
    }

    pub fn bash(cmd: impl Into<String>) -> Self {
        Self::Bash {
            command: cmd.into(),
            id: None,
        }
    }

    pub fn set_model(provider: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self::SetModel {
            provider: provider.into(),
            model_id: model_id.into(),
        }
    }

    pub fn switch_session(path: impl Into<String>) -> Self {
        Self::SwitchSession {
            session_path: path.into(),
        }
    }

    pub fn set_session_name(name: impl Into<String>) -> Self {
        Self::SetSessionName { name: name.into() }
    }

    // ── Builder methods (chainable) ────────────────────────────────────

    /// Set the request correlation ID.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        match &mut self {
            Self::Prompt { id: ref mut cmd_id, .. } => *cmd_id = Some(id.into()),
            Self::Bash { id: ref mut cmd_id, .. } => *cmd_id = Some(id.into()),
            _ => {}
        }
        self
    }

    /// Add an image to a prompt command.
    pub fn with_image(mut self, image: ImageContent) -> Self {
        match &mut self {
            Self::Prompt { images, .. } => images.push(image),
            Self::Steer { images, .. } => images.push(image),
            Self::FollowUp { images, .. } => images.push(image),
            _ => {}
        }
        self
    }

    /// Set streaming behavior on a prompt command.
    pub fn with_streaming_behavior(mut self, behavior: StreamingBehavior) -> Self {
        match &mut self {
            Self::Prompt { streaming_behavior, .. } => *streaming_behavior = Some(behavior),
            _ => {}
        }
        self
    }
}
