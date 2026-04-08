use crate::models::{ApiProtocol, AuthType, SessionProviderEnv};
use base64::Engine;
use serde_json::json;
use std::collections::HashMap;
use tracing::info;

const BRIDGE_PORT: u16 = 53686;

pub struct ProviderRuntime {
    pub env: HashMap<String, String>,
}

fn encode_bridge_config(provider_env: &SessionProviderEnv) -> Result<String, String> {
    let payload = json!({
        "baseUrl": provider_env.base_url.clone().unwrap_or_default(),
        "upstreamFormat": provider_env.upstream_format.clone().unwrap_or_else(|| "chat_completions".to_string()),
    });
    let text = serde_json::to_string(&payload).map_err(|error| error.to_string())?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(text.as_bytes()))
}

fn apply_auth(env: &mut HashMap<String, String>, provider_env: &SessionProviderEnv) {
    let api_key = provider_env.api_key.clone().unwrap_or_default();
    let auth_type = provider_env.auth_type.unwrap_or(AuthType::Both);

    match auth_type {
        // Claude CLI 对第三方兼容供应商用 x-api-key 更稳定，
        // 不再注入 ANTHROPIC_AUTH_TOKEN，避免触发 bearer/oauth 分支。
        AuthType::ApiKey | AuthType::AuthToken | AuthType::Both => {
            env.insert("ANTHROPIC_API_KEY".to_string(), api_key);
            env.remove("ANTHROPIC_AUTH_TOKEN");
        }
    }
}

fn mask_env_value(key: &str, value: &str) -> String {
    let upper_key = key.to_ascii_uppercase();
    let is_secret = upper_key.contains("KEY")
        || upper_key.contains("TOKEN")
        || upper_key.contains("SECRET")
        || upper_key.contains("PASSWORD");

    if !is_secret {
        return value.to_string();
    }

    let char_count = value.chars().count();
    if char_count <= 8 {
        return "****".to_string();
    }

    let prefix: String = value.chars().take(4).collect();
    let suffix: String = value
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{}****{}", prefix, suffix)
}

fn log_provider_runtime_env(env: &HashMap<String, String>, provider_env: &SessionProviderEnv) {
    let mut entries: Vec<(String, String)> = env
        .iter()
        .map(|(key, value)| (key.clone(), mask_env_value(key, value)))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    info!(
        "🧩 Provider runtime env prepared: protocol={:?}, base_url={:?}, upstream_format={:?}, env={:?}",
        provider_env.api_protocol,
        provider_env.base_url,
        provider_env.upstream_format,
        entries
    );
}

fn ensure_node_backend() -> Result<(), String> {
    let mut process = crate::node::process::GLOBAL_NODE_PROCESS
        .lock()
        .map_err(|error| format!("Failed to lock node process: {}", error))?;

    if process.is_running() {
        return Ok(());
    }

    process.start().map_err(|error| error.to_string())
}

pub async fn prepare_provider_env(
    _session_id: Option<String>,
    provider_env: SessionProviderEnv,
) -> Result<ProviderRuntime, String> {
    let mut env = HashMap::new();

    if provider_env.api_protocol.unwrap_or(ApiProtocol::Anthropic) == ApiProtocol::Openai {
        ensure_node_backend()?;
        let encoded = encode_bridge_config(&provider_env)?;
        env.insert(
            "ANTHROPIC_BASE_URL".to_string(),
            format!("http://127.0.0.1:{}/bridge/{}", BRIDGE_PORT, encoded),
        );
        env.insert("NO_PROXY".to_string(), "127.0.0.1,localhost".to_string());
        env.insert("no_proxy".to_string(), "127.0.0.1,localhost".to_string());
        apply_auth(&mut env, &provider_env);
    } else {
        if let Some(base_url) = provider_env.base_url.clone() {
            env.insert("ANTHROPIC_BASE_URL".to_string(), base_url);
        }
        apply_auth(&mut env, &provider_env);
    }

    for (key, value) in &provider_env.extra_env {
        env.insert(key.clone(), value.clone());
    }

    log_provider_runtime_env(&env, &provider_env);

    Ok(ProviderRuntime { env })
}
