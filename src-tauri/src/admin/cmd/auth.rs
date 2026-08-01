//! Pi provider credential management.
//!
//! Pi stores API keys and OAuth tokens in `~/.pi/agent/auth.json` as
//! `{ "<provider>": { "type": "api_key", "key": "..." } }`. Pi itself only
//! exposes an interactive `/login` (OAuth) and a read-only key lookup, so piter
//! implements the write path here — writing the exact format pi expects
//! (see pi docs: packages/coding-agent/docs/providers.md).
//!
//! Custom providers (baseUrl/api/compat/models) live in `~/.pi/agent/models.json`
//! and are edited as raw JSON, mirroring Picot's models-config editor.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use pi_server::broker::util::get_pi_agent_dir;

/// A known API-key provider that pi ships with (providers.md).
struct ProviderSpec {
    /// The `auth.json` key / RPC provider id.
    id: &'static str,
    /// Human-friendly display name.
    display_name: &'static str,
    /// Environment variable pi falls back to when no entry is stored.
    env_var: &'static str,
}

const KNOWN_PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec { id: "anthropic", display_name: "Anthropic", env_var: "ANTHROPIC_API_KEY" },
    ProviderSpec { id: "azure-openai-responses", display_name: "Azure OpenAI Responses", env_var: "AZURE_OPENAI_API_KEY" },
    ProviderSpec { id: "openai", display_name: "OpenAI", env_var: "OPENAI_API_KEY" },
    ProviderSpec { id: "deepseek", display_name: "DeepSeek", env_var: "DEEPSEEK_API_KEY" },
    ProviderSpec { id: "google", display_name: "Google Gemini", env_var: "GEMINI_API_KEY" },
    ProviderSpec { id: "mistral", display_name: "Mistral", env_var: "MISTRAL_API_KEY" },
    ProviderSpec { id: "groq", display_name: "Groq", env_var: "GROQ_API_KEY" },
    ProviderSpec { id: "cerebras", display_name: "Cerebras", env_var: "CEREBRAS_API_KEY" },
    ProviderSpec { id: "nvidia", display_name: "NVIDIA NIM", env_var: "NVIDIA_API_KEY" },
    ProviderSpec { id: "amazon-bedrock", display_name: "Amazon Bedrock", env_var: "AWS_BEARER_TOKEN_BEDROCK" },
    ProviderSpec { id: "cloudflare-ai-gateway", display_name: "Cloudflare AI Gateway", env_var: "CLOUDFLARE_API_KEY" },
    ProviderSpec { id: "cloudflare-workers-ai", display_name: "Cloudflare Workers AI", env_var: "CLOUDFLARE_API_KEY" },
    ProviderSpec { id: "xai", display_name: "xAI", env_var: "XAI_API_KEY" },
    ProviderSpec { id: "openrouter", display_name: "OpenRouter", env_var: "OPENROUTER_API_KEY" },
    ProviderSpec { id: "vercel-ai-gateway", display_name: "Vercel AI Gateway", env_var: "AI_GATEWAY_API_KEY" },
    ProviderSpec { id: "zai", display_name: "ZAI", env_var: "ZAI_API_KEY" },
    ProviderSpec { id: "opencode", display_name: "OpenCode Zen", env_var: "OPENCODE_API_KEY" },
    ProviderSpec { id: "opencode-go", display_name: "OpenCode Go", env_var: "OPENCODE_API_KEY" },
    ProviderSpec { id: "huggingface", display_name: "Hugging Face", env_var: "HF_TOKEN" },
    ProviderSpec { id: "fireworks", display_name: "Fireworks", env_var: "FIREWORKS_API_KEY" },
    ProviderSpec { id: "together", display_name: "Together AI", env_var: "TOGETHER_API_KEY" },
    ProviderSpec { id: "kimi-coding", display_name: "Kimi For Coding", env_var: "KIMI_API_KEY" },
    ProviderSpec { id: "minimax", display_name: "MiniMax", env_var: "MINIMAX_API_KEY" },
    ProviderSpec { id: "minimax-cn", display_name: "MiniMax (China)", env_var: "MINIMAX_CN_API_KEY" },
    ProviderSpec { id: "xiaomi", display_name: "Xiaomi MiMo", env_var: "XIAOMI_API_KEY" },
    ProviderSpec { id: "xiaomi-token-plan-cn", display_name: "Xiaomi MiMo Token Plan (China)", env_var: "XIAOMI_TOKEN_PLAN_CN_API_KEY" },
    ProviderSpec { id: "xiaomi-token-plan-ams", display_name: "Xiaomi MiMo Token Plan (Amsterdam)", env_var: "XIAOMI_TOKEN_PLAN_AMS_API_KEY" },
    ProviderSpec { id: "xiaomi-token-plan-sgp", display_name: "Xiaomi MiMo Token Plan (Singapore)", env_var: "XIAOMI_TOKEN_PLAN_SGP_API_KEY" },
];

/// Where a provider's credential comes from (mirrors pi's resolution order:
/// auth.json beats environment variable).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PiAuthSource {
    Stored,
    Environment,
    None,
}

#[derive(Debug, Clone, Serialize)]
pub struct PiProviderStatus {
    pub provider: String,
    pub display_name: String,
    pub configured: bool,
    pub source: PiAuthSource,
    /// Set for auth.json entries that are not one of the known API-key
    /// providers (typically OAuth subscriptions stored by `pi /login`).
    pub custom: bool,
    /// Environment variable providing the key (only when source is Environment).
    pub env_var: Option<String>,
}

fn auth_json_path() -> PathBuf {
    get_pi_agent_dir().join("auth.json")
}

fn read_auth_map() -> Result<BTreeMap<String, serde_json::Value>, String> {
    let path = auth_json_path();
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let json = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    serde_json::from_str(&json)
        .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
}

/// Persist the auth map. `~/.pi/agent/` is created on demand and the file is
/// locked down to 0600 on unix, matching pi's own behavior.
fn write_auth_map(map: &BTreeMap<String, serde_json::Value>) -> Result<(), String> {
    let path = auth_json_path();
    let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("Failed to create {}: {}", dir.display(), e))?;
    let json = serde_json::to_string_pretty(map)
        .map_err(|e| format!("Failed to serialize auth.json: {}", e))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    log::info!("[admin] wrote {}", path.display());
    Ok(())
}

fn env_is_set(name: &str) -> bool {
    std::env::var_os(name)
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

/// List every provider pi knows plus any extra entries stored in auth.json,
/// with their current credential status.
#[tauri::command]
pub fn list_pi_auth_status() -> Result<Vec<PiProviderStatus>, String> {
    let stored = read_auth_map()?;

    let mut result: Vec<PiProviderStatus> = KNOWN_PROVIDERS
        .iter()
        .map(|spec| {
            let stored_entry = stored.contains_key(spec.id);
            let env_configured = env_is_set(spec.env_var);
            let (source, env_var) = if stored_entry {
                (PiAuthSource::Stored, None)
            } else if env_configured {
                (PiAuthSource::Environment, Some(spec.env_var.to_string()))
            } else {
                (PiAuthSource::None, None)
            };
            PiProviderStatus {
                provider: spec.id.to_string(),
                display_name: spec.display_name.to_string(),
                configured: source != PiAuthSource::None,
                source,
                custom: false,
                env_var,
            }
        })
        .collect();

    // Surface unknown entries read-only so OAuth subscriptions (from
    // `pi /login`) are visible and can be removed.
    for (id, _) in stored {
        if !KNOWN_PROVIDERS.iter().any(|s| s.id == id) {
            result.push(PiProviderStatus {
                provider: id.clone(),
                display_name: id.clone(),
                configured: true,
                source: PiAuthSource::Stored,
                custom: true,
                env_var: None,
            });
        }
    }

    Ok(result)
}

/// Store an API key for a provider in auth.json (0600).
#[tauri::command]
pub fn set_pi_api_key(provider: String, api_key: String) -> Result<(), String> {
    let provider = provider.trim().to_string();
    if !KNOWN_PROVIDERS.iter().any(|s| s.id == provider) {
        return Err(format!("Unknown provider '{}'", provider));
    }
    let key = api_key.trim().to_string();
    if key.is_empty() {
        return Err("API key cannot be empty".into());
    }
    let mut map = read_auth_map()?;
    map.insert(provider.clone(), serde_json::json!({ "type": "api_key", "key": key }));
    write_auth_map(&map)?;
    log::info!("[admin] stored API key for provider {}", provider);
    Ok(())
}

/// Remove the stored credential for a provider from auth.json.
#[tauri::command]
pub fn remove_pi_api_key(provider: String) -> Result<(), String> {
    let provider = provider.trim().to_string();
    let mut map = read_auth_map()?;
    if map.remove(&provider).is_none() {
        return Err(format!("No stored credentials for provider '{}'", provider));
    }
    write_auth_map(&map)?;
    log::info!("[admin] removed credential for provider {}", provider);
    Ok(())
}

/// Read models.json (custom providers). Returns pretty JSON; a sane default
/// when the file does not exist yet.
#[tauri::command]
pub fn get_pi_models_config() -> Result<String, String> {
    let path = get_pi_agent_dir().join("models.json");
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let value: serde_json::Value = serde_json::from_str(&content)
                .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;
            serde_json::to_string_pretty(&value)
                .map_err(|e| format!("Failed to serialize models config: {}", e))
        }
        Err(_) => Ok("{\n  \"providers\": {}\n}".to_string()),
    }
}

/// Save models.json, validating that the content is a JSON object.
#[tauri::command]
pub fn save_pi_models_config(content: String) -> Result<(), String> {
    let value: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Invalid JSON: {}", e))?;
    if !value.is_object() {
        return Err("models.json must be a JSON object".into());
    }
    let path = get_pi_agent_dir().join("models.json");
    let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("Failed to create {}: {}", dir.display(), e))?;
    let json = serde_json::to_string_pretty(&value)
        .map_err(|e| format!("Failed to serialize models config: {}", e))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    log::info!("[admin] pi models config saved to {}", path.display());
    Ok(())
}
