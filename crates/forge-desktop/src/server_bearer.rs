use std::sync::Arc;

use forge_server::ServerHandle;
use forge_storage::{CredentialId, CredentialsRepo, SERVER_BEARER_CREDENTIAL_ID};

pub(crate) async fn current_bearer_token(
    server: Option<ServerHandle>,
    credentials: Arc<dyn CredentialsRepo>,
) -> Option<String> {
    match server {
        Some(handle) => Some(handle.bearer_token().await.into_inner()),
        None => credentials
            .load(&CredentialId::new(SERVER_BEARER_CREDENTIAL_ID))
            .await
            .ok()
            .flatten(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use forge_storage::StorageError;
    use time::OffsetDateTime;

    use super::*;
    use crate::test_support::{runtime, stopped_handle, test_backend};

    const STORED_TOKEN: &str = "fg_stored";

    struct Vault {
        rows: Mutex<HashMap<String, String>>,
        load_fails: bool,
    }

    impl Vault {
        fn holding(stored: Option<&str>, load_fails: bool) -> Arc<dyn CredentialsRepo> {
            let rows = stored
                .map(|token| (SERVER_BEARER_CREDENTIAL_ID.to_owned(), token.to_owned()))
                .into_iter()
                .collect();
            Arc::new(Self {
                rows: Mutex::new(rows),
                load_fails,
            })
        }
    }

    #[async_trait::async_trait]
    impl CredentialsRepo for Vault {
        async fn store(&self, id: &CredentialId, plaintext: &str) -> Result<(), StorageError> {
            self.rows
                .lock()
                .unwrap()
                .insert(id.as_str().to_owned(), plaintext.to_owned());
            Ok(())
        }

        async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
            if self.load_fails {
                return Err(StorageError::Connection {
                    reason: "credentials store unreachable".to_owned(),
                });
            }
            Ok(self.rows.lock().unwrap().get(id.as_str()).cloned())
        }

        async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
            Ok(self.rows.lock().unwrap().remove(id.as_str()).is_some())
        }

        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .keys()
                .map(|key| CredentialId::new(key.clone()))
                .collect())
        }

        async fn last_refresh(
            &self,
            _: &CredentialId,
        ) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }

        async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
            Ok(())
        }
    }

    #[test]
    fn a_server_answers_with_its_live_token_over_the_stored_one() {
        let rt = runtime();
        let (backend, _writes) = test_backend();
        let server = stopped_handle(&rt, &backend);
        let live = rt.block_on(server.bearer_token()).into_inner();

        let answered = rt.block_on(current_bearer_token(
            Some(server),
            Vault::holding(Some(STORED_TOKEN), false),
        ));

        assert_eq!(answered, Some(live));
    }

    #[test]
    fn a_server_answers_with_the_regenerated_token() {
        let rt = runtime();
        let (backend, _writes) = test_backend();
        let server = stopped_handle(&rt, &backend);
        let retired = rt.block_on(server.bearer_token()).into_inner();
        let regenerated = rt
            .block_on(server.regenerate_bearer_token())
            .expect("regenerate")
            .into_inner();

        let answered = rt.block_on(current_bearer_token(
            Some(server),
            Vault::holding(Some(&retired), false),
        ));

        assert_eq!(answered, Some(regenerated));
    }

    #[test]
    fn without_a_server_the_stored_token_answers_and_a_missing_or_unreadable_one_is_none() {
        let rt = runtime();
        for (stored, load_fails, expected, case) in [
            (
                Some(STORED_TOKEN),
                false,
                Some(STORED_TOKEN),
                "a stored token",
            ),
            (None, false, None, "an empty store"),
            (Some(STORED_TOKEN), true, None, "an unreadable store"),
        ] {
            let answered = rt.block_on(current_bearer_token(
                None,
                Vault::holding(stored, load_fails),
            ));

            assert_eq!(answered.as_deref(), expected, "{case}");
        }
    }
}
