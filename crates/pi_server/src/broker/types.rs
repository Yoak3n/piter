//! Type definitions for the broker module.
//!
//! Each pi process is wrapped as a `PiInstance`, keyed by UUID `instance_id`.
//! Instances can be *persistent* (normal session, `--session <path>`) or
//! *ephemeral* (`--no-session`, one-shot for backend queries).

use std::collections::HashMap;
use std::process::Child;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use parking_lot::Mutex as PLMutex;
use tokio::sync::{broadcast, mpsc};

use pi_rpc::event::Response;

// ─── Channel Types ──────────────────────────────────────────────────────────

/// Broadcast sender for pi stdout events → subscribers (gateway, etc.).
pub type EventTx = broadcast::Sender<String>;

pub const EVENT_CHANNEL_CAP: usize = 4096;
pub const PROTOCOL_VERSION: u8 = 1;

// ─── Pi Instance ──────────────────────────────────────────────────────────

/// A single pi process instance.
pub struct PiInstance {
    pub id: String,
    pub child: Child,
    pub running: Arc<std::sync::atomic::AtomicBool>,
    /// Set true when the instance is deliberately killed (kill_all / kill_instance /
    /// cleanup). Distinguishes intentional termination from startup self-exit
    /// (used to avoid false "startup failed" reports).
    pub killed: Arc<std::sync::atomic::AtomicBool>,
    pub stdin_tx: Option<mpsc::UnboundedSender<String>>,
    /// Session file path (known when resuming, None for new sessions).
    pub session_path: Option<String>,
    pub persistent: bool,
    pub cwd: String,
    pub created_at: std::time::Instant,
}

// ─── Pending RPC ────────────────────────────────────────────────────────────

/// A pending RPC request awaiting a pi response.
pub struct PendingRpc {
    pub sender: tokio::sync::oneshot::Sender<Response>,
}

// ─── Broker Inner State ────────────────────────────────────────────────────

#[derive(Default)]
pub struct BrokerInner {
    pub instances: PLMutex<HashMap<String, PiInstance>>,
    pub routes: PLMutex<HashMap<String, String>>,
    pub next_client_id: AtomicU64,
    pub pending_rpc: PLMutex<HashMap<String, PendingRpc>>,
}

// ─── Pi Agent Settings ─────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiAgentSettings {
    #[serde(default)]
    pub default_provider: String,
    #[serde(default)]
    pub default_model: String,
    #[serde(default)]
    pub default_thinking_level: String,
    /// Pi 允许 packages 数组里每个元素是普通 source 字符串（`"npm:foo"`）
    /// 或过滤对象（`{ "source": ..., "extensions": [...] }`）。
    /// piter 不解释这些内容，原样透传 JSON 即可。
    #[serde(default)]
    pub packages: Vec<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::PiAgentSettings;

    /// Pi 的 packages 数组允许混合普通 source 字符串和过滤对象
    /// （见 Pi packages.md「Package Filtering」）。此前这里误用
    /// Vec<String> 导致「invalid type: map, expected a string」解析失败。
    #[test]
    fn parses_mixed_packages_entries() {
        let json = r#"{
            "defaultProvider": "opencode-go",
            "defaultModel": "deepseek-v4-flash",
            "defaultThinkingLevel": "medium",
            "packages": [
                "npm:pi-subagents",
                {
                    "source": "npm:@hypabolic/pi-hypa",
                    "extensions": ["-extensions/index.ts"]
                },
                {
                    "source": "npm:pi-web-access",
                    "extensions": ["-index.ts"],
                    "skills": ["-skills/librarian/SKILL.md"]
                }
            ]
        }"#;
        let settings: PiAgentSettings =
            serde_json::from_str(json).expect("mixed packages entries should parse");
        assert_eq!(settings.packages.len(), 3);
        assert_eq!(settings.packages[0], serde_json::json!("npm:pi-subagents"));
        assert_eq!(
            settings.packages[1]["source"],
            serde_json::json!("npm:@hypabolic/pi-hypa")
        );
        // 回写必须保持对象形式，不能把过滤配置丢成纯字符串
        let roundtrip = serde_json::to_string(&settings).unwrap();
        assert!(roundtrip.contains("\"extensions\":[\"-extensions/index.ts\"]"));
        assert!(roundtrip.contains("\"source\":\"npm:@hypabolic/pi-hypa\""));
    }
}
