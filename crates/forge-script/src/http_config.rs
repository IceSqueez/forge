use forge_storage::{SettingsRepo, get_bool_setting, get_json_setting, reserved_keys};

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

    let allowed_domains =
        match get_json_setting::<Vec<String>>(repo, reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS_KEY)
            .await
        {
            Some(domains) => domains,
            None => legacy_csv_domains(repo).await,
        };

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

    let allow_local = get_bool_setting(
        repo,
        reserved_keys::SCRIPT_HTTP_ALLOW_LOCAL_KEY,
        defaults.allow_local,
    )
    .await;

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

/// Tolerant read of the comma-separated allow-list rows written before the JSON format.
async fn legacy_csv_domains(repo: &dyn SettingsRepo) -> Vec<String> {
    repo.get_string(reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS_KEY)
        .await
        .ok()
        .flatten()
        .map(|s| {
            s.split(',')
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use forge_storage::set_json_setting;
    use forge_storage_sqlite::SqliteBackend;

    async fn backend() -> SqliteBackend {
        SqliteBackend::open_with_key("sqlite::memory:", [0xab; 32])
            .await
            .expect("in-memory SQLite")
    }

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

    #[tokio::test]
    async fn allowed_domains_reads_legacy_csv_when_value_is_not_json() {
        let backend = backend().await;
        backend
            .set_string(
                reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS_KEY,
                "a.com, b.com ,c.com,",
            )
            .await
            .unwrap();
        let cfg = load_script_http_config(&backend).await;
        // Pre-JSON rows survive: split on commas, trim, drop the trailing empty.
        assert_eq!(cfg.allowed_domains, vec!["a.com", "b.com", "c.com"]);
    }

    #[tokio::test]
    async fn allowed_domains_prefers_json_array_over_csv_split() {
        let backend = backend().await;
        let domains = vec!["good.com".to_owned(), "also.com".to_owned()];
        set_json_setting(
            &backend,
            reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS_KEY,
            &domains,
        )
        .await
        .unwrap();
        let cfg = load_script_http_config(&backend).await;
        // A CSV split of `["good.com","also.com"]` would mangle into bracket/quote
        // fragments; getting the clean pair proves the JSON path wins.
        assert_eq!(cfg.allowed_domains, domains);
    }

    #[tokio::test]
    async fn allowed_domains_blank_value_stays_deny_by_default() {
        for blank in ["", "   ", ",,", " , , "] {
            let backend = backend().await;
            backend
                .set_string(reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS_KEY, blank)
                .await
                .unwrap();
            let cfg = load_script_http_config(&backend).await;
            assert!(
                cfg.allowed_domains.is_empty(),
                "blank value {blank:?} must grant no domains",
            );
        }
    }
}
