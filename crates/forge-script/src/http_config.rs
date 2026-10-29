use forge_storage::{SettingsRepo, StorageError, read_setting_or_default, stored_json_setting};

use crate::settings;

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
            max_calls_per_script: settings::HTTP_MAX_CALLS.codec().default,
            timeout_ms: settings::HTTP_TIMEOUT_MS.codec().default,
            allow_local: settings::HTTP_ALLOW_LOCAL.codec().default,
            max_response_bytes: settings::HTTP_MAX_RESPONSE_BYTES.codec().default,
        }
    }
}

impl ScriptHttpConfig {
    pub async fn read(repo: &dyn SettingsRepo) -> Self {
        let allowed_domains = match stored_script_http_allowed_domains(repo).await {
            Ok(domains) => domains,
            Err(error) => {
                tracing::warn!(
                    key = settings::HTTP_ALLOWED_DOMAINS.key.name(),
                    error = %error,
                    "stored setting cannot be read; using its default"
                );
                Vec::new()
            }
        };
        Self {
            allowed_domains,
            max_calls_per_script: read_setting_or_default(repo, &settings::HTTP_MAX_CALLS).await,
            timeout_ms: read_setting_or_default(repo, &settings::HTTP_TIMEOUT_MS).await,
            allow_local: read_setting_or_default(repo, &settings::HTTP_ALLOW_LOCAL).await,
            max_response_bytes: read_setting_or_default(repo, &settings::HTTP_MAX_RESPONSE_BYTES)
                .await,
        }
    }
}

pub async fn stored_script_http_allowed_domains(
    repo: &dyn SettingsRepo,
) -> Result<Vec<String>, StorageError> {
    let key = settings::HTTP_ALLOWED_DOMAINS.key.name();
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
    async fn read_returns_defaults_when_keys_absent() {
        let backend = backend().await;
        let cfg = ScriptHttpConfig::read(&*backend).await;
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
                    .set_string(settings::HTTP_ALLOWED_DOMAINS.key.name(), raw)
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
        let cfg = ScriptHttpConfig::read(&UnreachableStore).await;

        assert!(cfg.allowed_domains.is_empty());
    }

    #[tokio::test]
    async fn read_clamps_persisted_http_limits_into_the_sandbox_range() {
        for (stored, max_calls, timeout_ms, max_response_bytes) in [
            ("0", 1, 100, 1_024),
            ("1", 1, 100, 1_024),
            ("2", 2, 100, 1_024),
            ("99", 99, 100, 1_024),
            ("100", 100, 100, 1_024),
            ("101", 100, 101, 1_024),
            ("1023", 100, 1_023, 1_024),
            ("1024", 100, 1_024, 1_024),
            ("1025", 100, 1_025, 1_025),
            ("29999", 100, 29_999, 29_999),
            ("30000", 100, 30_000, 30_000),
            ("30001", 100, 30_000, 30_001),
            ("10485759", 100, 30_000, 10_485_759),
            ("10485760", 100, 30_000, 10_485_760),
            ("10485761", 100, 30_000, 10_485_760),
        ] {
            let backend = backend().await;
            for setting in [
                &settings::HTTP_MAX_CALLS,
                &settings::HTTP_TIMEOUT_MS,
                &settings::HTTP_MAX_RESPONSE_BYTES,
            ] {
                backend.set_string(setting.name(), stored).await.unwrap();
            }

            let cfg = ScriptHttpConfig::read(&*backend).await;

            assert_eq!(
                (
                    cfg.max_calls_per_script,
                    cfg.timeout_ms,
                    cfg.max_response_bytes
                ),
                (max_calls, timeout_ms, max_response_bytes),
                "stored {stored:?}"
            );
        }
    }
}
