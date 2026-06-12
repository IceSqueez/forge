mod identity;
mod send_announcement;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};
use forge_storage::CredentialsRepo;

pub use identity::SelfIdentity;
pub use send_announcement::SendAnnouncementRunner;

use crate::helix::HelixTransport;

pub fn register_twitch_sub_actions(
    reg: &mut SubActionRegistry,
    transport: Arc<dyn HelixTransport>,
    creds: Arc<dyn CredentialsRepo>,
) -> Result<(), RegistryError> {
    let identity = Arc::new(SelfIdentity::new(creds));
    reg.register(Box::new(SendAnnouncementRunner::new(transport, identity)))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) mod test_support {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use forge_events::{Event, EventPublisher};
    use forge_registry::RunContext;
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};
    use forge_types::{ArgStack, EventId};
    use time::OffsetDateTime;

    use crate::helix::{HelixError, HelixRequest, HelixTransport};

    pub(crate) const TOKEN_SENTINEL: &str = "FAKE_TWITCH_TOKEN_SENTINEL_qq123";
    pub(crate) const SELF_USER_ID: &str = "9876";

    pub(crate) struct MockTransport {
        calls: Mutex<Vec<HelixRequest>>,
        response: Mutex<Option<Result<serde_json::Value, HelixError>>>,
    }

    impl MockTransport {
        pub(crate) fn returning(response: Result<serde_json::Value, HelixError>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                response: Mutex::new(Some(response)),
            }
        }

        pub(crate) fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }

        pub(crate) fn last_request(&self) -> HelixRequest {
            self.calls.lock().unwrap().last().unwrap().clone()
        }
    }

    #[async_trait]
    impl HelixTransport for MockTransport {
        async fn execute(&self, request: HelixRequest) -> Result<serde_json::Value, HelixError> {
            self.calls.lock().unwrap().push(request);
            self.response
                .lock()
                .unwrap()
                .take()
                .unwrap_or(Ok(serde_json::Value::Null))
        }
    }

    pub(crate) struct MockCreds {
        bundle: Option<String>,
    }

    impl MockCreds {
        /// Canned `twitch:broadcaster` bundle with a sentinel token, so leak
        /// assertions can prove the token never surfaces in outcomes.
        pub(crate) fn with_identity() -> Self {
            Self {
                bundle: Some(
                    serde_json::json!({
                        "access_token": TOKEN_SENTINEL,
                        "user_id": SELF_USER_ID,
                        "login": "streamer",
                        "expires_at_unix": null,
                    })
                    .to_string(),
                ),
            }
        }

        pub(crate) fn empty() -> Self {
            Self { bundle: None }
        }
    }

    #[async_trait]
    impl CredentialsRepo for MockCreds {
        async fn store(&self, _: &CredentialId, _: &str) -> Result<(), StorageError> {
            Ok(())
        }

        async fn load(&self, _: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(self.bundle.clone())
        }

        async fn delete(&self, _: &CredentialId) -> Result<bool, StorageError> {
            Ok(false)
        }

        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(Vec::new())
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

    struct NoopPublisher;

    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    pub(crate) fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext {
            arg_stack: stack,
            index: 0,
            parent_event_id: EventId::new(),
            publisher: &NoopPublisher,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::test_support::{MockCreds, MockTransport};
    use super::*;

    #[test]
    fn register_twitch_sub_actions_resolves_announcement_runner() {
        let mut reg = SubActionRegistry::new();

        register_twitch_sub_actions(
            &mut reg,
            Arc::new(MockTransport::returning(Ok(serde_json::Value::Null))),
            Arc::new(MockCreds::empty()),
        )
        .unwrap();

        assert!(reg.get("twitch.chat.send_announcement").is_some());
    }
}
