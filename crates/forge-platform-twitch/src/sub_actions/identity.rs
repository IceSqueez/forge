use std::sync::Arc;

use forge_storage::CredentialsRepo;

use crate::credentials;
use crate::helix::{HelixError, HelixMethod, HelixRequest, HelixTransport};

pub struct SelfIdentity {
    creds: Arc<dyn CredentialsRepo>,
}

impl SelfIdentity {
    pub fn new(creds: Arc<dyn CredentialsRepo>) -> Self {
        Self { creds }
    }

    /// Loaded per call, so a re-auth under a different account takes effect without re-registering runners.
    pub async fn user_id(&self) -> Result<String, HelixError> {
        let cred = credentials::load(self.creds.as_ref())
            .await
            .map_err(|e| HelixError::Credentials(e.to_string()))?
            .ok_or_else(|| HelixError::Credentials("no twitch credentials stored".to_owned()))?;
        Ok(cred.user_id)
    }
}

/// Costs one Helix rate-limit token per call.
pub async fn resolve_user_id(
    transport: &dyn HelixTransport,
    login: &str,
) -> Result<String, HelixError> {
    let request = HelixRequest::new(HelixMethod::Get, "/helix/users").query("login", login);
    let resp = transport.execute(request).await?;
    resp["data"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|u| u["id"].as_str())
        .map(|s| s.to_owned())
        .ok_or_else(|| HelixError::Http {
            status: 404,
            body: format!("user not found: {login}"),
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::sub_actions::test_support::{MockTransport, users_fixture};

    #[tokio::test]
    async fn resolve_user_id_queries_users_by_login_and_returns_first_id() {
        let transport = MockTransport::returning(users_fixture("555"));

        let id = resolve_user_id(&transport, "target").await.unwrap();

        assert_eq!(id, "555");
        let request = transport.last_request();
        assert_eq!(request.method, HelixMethod::Get);
        assert_eq!(request.path, "/helix/users");
        assert!(
            request
                .query
                .contains(&("login".to_owned(), "target".to_owned())),
            "lookup must be keyed by the login query parameter"
        );
    }

    #[tokio::test]
    async fn resolve_user_id_errs_when_no_user_matches() {
        let transport = MockTransport::returning(Ok(serde_json::json!({ "data": [] })));

        let err = resolve_user_id(&transport, "ghost").await.unwrap_err();

        assert!(
            matches!(err, HelixError::Http { status: 404, .. }),
            "empty data array must map to a not-found error, got: {err}"
        );
    }
}
