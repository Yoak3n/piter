//! Extension UI request/response types.

use serde::{Deserialize, Serialize};

// ─── Extension UI Request (stdout → client) ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method")]
pub enum ExtensionUiRequest {
    #[serde(rename = "select")]
    Select {
        id: String,
        title: String,
        options: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<u64>,
    },
    #[serde(rename = "confirm")]
    Confirm {
        id: String,
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<u64>,
    },
    #[serde(rename = "input")]
    Input {
        id: String,
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
    },
    #[serde(rename = "editor")]
    Editor {
        id: String,
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        prefill: Option<String>,
    },
    #[serde(rename = "notify")]
    Notify {
        id: String,
        message: String,
        #[serde(rename = "notifyType", skip_serializing_if = "Option::is_none")]
        notify_type: Option<NotifyType>,
    },
    #[serde(rename = "setStatus")]
    SetStatus {
        id: String,
        #[serde(rename = "statusKey")]
        status_key: String,
        #[serde(rename = "statusText", skip_serializing_if = "Option::is_none")]
        status_text: Option<String>,
    },
    #[serde(rename = "setWidget")]
    SetWidget {
        id: String,
        #[serde(rename = "widgetKey")]
        widget_key: String,
        #[serde(rename = "widgetLines", skip_serializing_if = "Option::is_none")]
        widget_lines: Option<Vec<String>>,
        #[serde(rename = "widgetPlacement", skip_serializing_if = "Option::is_none")]
        widget_placement: Option<WidgetPlacement>,
    },
    #[serde(rename = "setTitle")]
    SetTitle { id: String, title: String },
    #[serde(rename = "set_editor_text")]
    SetEditorText { id: String, text: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NotifyType {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WidgetPlacement {
    AboveEditor,
    BelowEditor,
}

// ─── Extension UI Response (stdin ← client) ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionUiResponse {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancelled: Option<bool>,
}

impl ExtensionUiResponse {
    pub fn value(id: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            value: Some(value.into()),
            confirmed: None,
            cancelled: None,
        }
    }

    pub fn confirmed(id: impl Into<String>, confirmed: bool) -> Self {
        Self {
            id: id.into(),
            value: None,
            confirmed: Some(confirmed),
            cancelled: None,
        }
    }

    pub fn cancelled(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            value: None,
            confirmed: None,
            cancelled: Some(true),
        }
    }
}
