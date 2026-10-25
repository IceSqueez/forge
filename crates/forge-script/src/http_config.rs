use forge_storage::{
    SettingsRepo, StorageError, get_bool_setting, reserved_keys, stored_json_setting,
};

pub struct ScriptHttpConfig {
    pub allowed_domains: Vec<String>,
    pub max_calls_per_script: u32,
    pub timeout_ms: u32,
    pub allow_local: bool,
    pub max_response_bytes: u32,
}

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

    let allowed_domains = stored_script_http_allowed_domains(repo)
        .await
        .unwrap_or_default();

    let max_calls_per_script = repo
        .get_string(reserved_keys::SCRIPT_HTTP_MAX_CALLS)
        .await
        .ok()
        .flatten()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(defaults.max_calls_per_script);

    let timeout_ms = repo
        .get_string(reserved_keys::SCRIPT_HTTP_TIMEOUT_MS)
        .await
        .ok()
        .flatten()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(defaults.timeout_ms);

    let allow_local = get_bool_setting(
        repo,
        reserved_keys::SCRIPT_HTTP_ALLOW_LOCAL,
        defaults.allow_local,
    )
    .await;

    let max_response_bytes = repo
        .get_string(reserved_keys::SCRIPT_HTTP_MAX_RESPONSE_BYTES)
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

pub async fn stored_script_http_allowed_domains(
    repo: &dyn SettingsRepo,
) -> Result<Vec<String>, StorageError> {
    let key = reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS;
    match stored_json_setting::<Vec<String>>(repo, key).await {
        Ok(domains) => Ok(domains.unwrap_or_default()),
        Err(StorageError::UndecodableSetting { .. }) => Ok(repo
            .get_string(key)
            .await?
            .map(|legacy| parse_legacy_csv_domains(&legacy))
            .unwrap_or_default()),
        Err(error) => Err(error),
    }
}

fn parse_legacy_csv_domains(legacy: &str) -> Vec<String> {
    legacy
        .split(',')
        .map(str::trim)
        .filter(|domain| !domain.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_storage_sqlite::SqliteBackend;

    use crate::test_support::{Sandboxed, sandboxed_backend};

    async fn backend() -> Sandboxed<SqliteBackend> {
        sandboxed_backend([0xab; 32]).await
    }

    #[test]
    fn default_is_deny_by_default() {
        let cfg = ScriptHttpConfig::default();
        assert!(cfg.allowed_domains.is_empty());
        assert!(!cfg.allow_local);
    }

    #[tokio::test]
    async fn load_script_http_config_returns_defaults_when_keys_absent() {
        let backend = backend().await;
        let cfg = load_script_http_config(&*backend).await;
        let defaults = ScriptHttpConfig::default();
        assert_eq!(cfg.allowed_domains, defaults.allowed_domains);
        assert_eq!(cfg.max_calls_per_script, defaults.max_calls_per_script);
        assert_eq!(cfg.timeout_ms, defaults.timeout_ms);
        assert_eq!(cfg.allow_local, defaults.allow_local);
        assert_eq!(cfg.max_response_bytes, defaults.max_response_bytes);
    }

    struct UnreachableStore;

    #[async_trait::async_trait]
    impl SettingsRepo for UnreachableStore {
        async fn get_string(&self, _: &str) -> Result<Option<String>, StorageError> {
            Err(StorageError::Connection {
                reason: "settings store unreachable".to_owned(),
            })
        }
        async fn set_string(&self, _: &str, _: &str) -> Result<(), StorageError> {
            unreachable!("loading the allowlist never writes")
        }
        async fn delete(&self, _: &str) -> Result<bool, StorageError> {
            unreachable!("loading the allowlist never deletes")
        }
        async fn load_all(
            &self,
        ) -> Result<std::collections::HashMap<String, String>, StorageError> {
            unreachable!("loading the allowlist reads single keys")
        }
    }

    #[tokio::test]
    async fn stored_allowlist_reads_json_falls_back_to_legacy_csv_and_keeps_blank_values_empty() {
        let json = serde_json::to_string(&["good.com", "also.com"]).unwrap();
        for (stored, expected) in [
            (None, Vec::<&str>::new()),
            (Some(json.as_str()), vec!["good.com", "also.com"]),
            (Some("[]"), Vec::new()),
            (
                Some("a.com, b.com ,c.com,"),
                vec!["a.com", "b.com", "c.com"],
            ),
            (Some("solo.com"), vec!["solo.com"]),
            (Some(""), Vec::new()),
            (Some("   "), Vec::new()),
            (Some(",,"), Vec::new()),
            (Some(" , , "), Vec::new()),
        ] {
            let backend = backend().await;
            if let Some(raw) = stored {
                backend
                    .set_string(reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS, raw)
                    .await
                    .unwrap();
            }

            let domains = stored_script_http_allowed_domains(&*backend).await.unwrap();

            assert_eq!(domains, expected, "{stored:?}");
        }
    }

    #[tokio::test]
    async fn stored_allowlist_reports_a_store_failure_instead_of_an_empty_list() {
        let loaded = stored_script_http_allowed_domains(&UnreachableStore).await;

        assert!(matches!(loaded, Err(StorageError::Connection { .. })));
    }

    #[tokio::test]
    async fn script_http_config_stays_deny_by_default_when_the_allowlist_cannot_be_read() {
        let cfg = load_script_http_config(&UnreachableStore).await;

        assert!(cfg.allowed_domains.is_empty());
    }
}
