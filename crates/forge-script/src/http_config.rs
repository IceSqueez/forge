use forge_storage::{SettingsRepo, reserved_keys};

pub struct ScriptHttpConfig {
    pub allowed_domains: Vec<String>,
    pub max_calls_per_script: u32,
    pub timeout_ms: u32,
    pub allow_local: bool,
    pub max_response_bytes: u32,
}

/// Deny-all defaults: no domains allowed, local addresses blocked.
impl Default for ScriptHttpConfig {
    fn default() -> Self {
        Self {
            allowed_domains: Vec::new(),
            max_calls_per_script: 10,
            timeout_ms: 5_000,
            allow_local: false,
            max_response_bytes: 1_048_576,
        }
    }
}

pub async fn load_script_http_config(repo: &dyn SettingsRepo) -> ScriptHttpConfig {
    let defaults = ScriptHttpConfig::default();

    let allowed_domains = repo
        .get_string(reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS_KEY)
        .await
        .ok()
        .flatten()
        .map(|s| {
            s.split(',')
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let max_calls_per_script = repo
        .get_string(reserved_keys::SCRIPT_HTTP_MAX_CALLS_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(defaults.max_calls_per_script);

    let timeout_ms = repo
        .get_string(reserved_keys::SCRIPT_HTTP_TIMEOUT_MS_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(defaults.timeout_ms);

    let allow_local = repo
        .get_string(reserved_keys::SCRIPT_HTTP_ALLOW_LOCAL_KEY)
        .await
        .ok()
        .flatten()
        .map(|s| s.eq_ignore_ascii_case("true"))
        .unwrap_or(defaults.allow_local);

    let max_response_bytes = repo
        .get_string(reserved_keys::SCRIPT_HTTP_MAX_RESPONSE_BYTES_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(defaults.max_response_bytes);

    ScriptHttpConfig {
        allowed_domains,
        max_calls_per_script,
        timeout_ms,
        allow_local,
        max_response_bytes,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use forge_storage_sqlite::SqliteBackend;

    #[test]
    fn default_is_deny_by_default() {
        let cfg = ScriptHttpConfig::default();
        assert!(cfg.allowed_domains.is_empty());
        assert!(!cfg.allow_local);
    }

    #[tokio::test]
    async fn load_script_http_config_returns_defaults_when_keys_absent() {
        let backend = SqliteBackend::open_with_key("sqlite::memory:", [0xab; 32])
            .await
            .expect("in-memory SQLite");
        let cfg = load_script_http_config(&backend).await;
        let defaults = ScriptHttpConfig::default();
        assert_eq!(cfg.allowed_domains, defaults.allowed_domains);
        assert_eq!(cfg.max_calls_per_script, defaults.max_calls_per_script);
        assert_eq!(cfg.timeout_ms, defaults.timeout_ms);
        assert_eq!(cfg.allow_local, defaults.allow_local);
        assert_eq!(cfg.max_response_bytes, defaults.max_response_bytes);
    }
}
