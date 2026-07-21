use std::time::Duration;

use forge_platform_core::auth::{CALLBACK_PATH, PkceClientConfig, PkceFlow};
use forge_platform_core::{AuthFlow, PlatformError};
use serde::Deserialize;
use time::OffsetDateTime;

pub const KICK_AUTHORIZE_ENDPOINT: &str = "https://id.kick.com/oauth/authorize";
pub const KICK_TOKEN_ENDPOINT: &str = "https://id.kick.com/oauth/token";
pub const KICK_USERS_ENDPOINT: &str = "https://api.kick.com/public/v1/users";

const KICK_SCOPES: &[&str] = &[
    "user:read",
    "channel:read",
    "channel:write",
    "channel:rewards:read",
    "channel:rewards:write",
    "chat:write",
    "moderation:chat_message:manage",
    "moderation:ban",
];

pub fn kick_auth_flow() -> AuthFlow {
    AuthFlow::LocalCallback {
        authorize_url: KICK_AUTHORIZE_ENDPOINT.to_owned(),
        token_endpoint: KICK_TOKEN_ENDPOINT.to_owned(),
        redirect_path: CALLBACK_PATH.to_owned(),
        scopes: KICK_SCOPES.iter().map(|s| (*s).to_owned()).collect(),
    }
}

#[derive(Debug, Clone)]
pub struct LoopbackCode {
    pub auth_url: String,
}

#[derive(Debug, Clone)]
pub struct KickAuthBundle {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: u64,
    pub username: String,
    pub client_id: String,
    pub expires_at: OffsetDateTime,
}

pub struct KickAuthFlow {
    client_id: String,
    http: reqwest::Client,
    users_endpoint: String,
    pkce: PkceFlow,
}

impl KickAuthFlow {
    pub fn new(client_id: String) -> Self {
        Self::with_endpoints(
            client_id,
            KICK_AUTHORIZE_ENDPOINT.to_owned(),
            KICK_TOKEN_ENDPOINT.to_owned(),
            KICK_USERS_ENDPOINT.to_owned(),
        )
    }

    pub(crate) fn with_endpoints(
        client_id: String,
        authorize_endpoint: String,
        token_endpoint: String,
        users_endpoint: String,
    ) -> Self {
        let pkce = PkceFlow::new(PkceClientConfig {
            client_id: client_id.clone(),
            client_secret: None,
            authorize_endpoint,
            token_endpoint,
            scopes: KICK_SCOPES.iter().map(|s| (*s).to_owned()).collect(),
            // "redirect=127.0.0.1" must precede "redirect_uri" to prevent NextJS
            // host-rewriting on id.kick.com.
            authorize_pre_redirect_params: vec![("redirect".to_owned(), "127.0.0.1".to_owned())],
            authorize_trailing_params: Vec::new(),
        });
        Self {
            client_id,
            http: reqwest::Client::new(),
            users_endpoint,
            pkce,
        }
    }

    /// Binds a loopback listener, generates PKCE + state, stores the driver, and returns
    /// the URL the caller should open in the user's browser.
    pub async fn start(&mut self) -> Result<LoopbackCode, PlatformError> {
        let url = self.pkce.start(&[]).await?;
        Ok(LoopbackCode {
            auth_url: url.auth_url,
        })
    }

    /// Consumes the pending driver, waits for the loopback callback, exchanges the code for a
    /// Kick access token (PKCE - no `client_secret`), then resolves `user_id` + `username` from
    /// the authenticated-user endpoint.
    pub async fn wait_for_authorization(
        &mut self,
        timeout: Duration,
    ) -> Result<KickAuthBundle, PlatformError> {
        let token = self.pkce.exchange(timeout).await?;
        let refresh_token = token.refresh_token.ok_or_else(|| PlatformError::Auth {
            reason: "Kick token exchange did not return a refresh_token".into(),
        })?;
        let expires_in = token.expires_in.unwrap_or(0);

        let user = fetch_user_info(&self.http, &self.users_endpoint, &token.access_token).await?;

        let expires_at = OffsetDateTime::now_utc() + time::Duration::seconds(expires_in as i64);

        Ok(KickAuthBundle {
            access_token: token.access_token,
            refresh_token,
            user_id: user.user_id,
            username: user.name,
            client_id: self.client_id.clone(),
            expires_at,
        })
    }
}

#[derive(Debug, Deserialize)]
struct UserRecord {
    user_id: u64,
    name: String,
}

#[derive(Debug, Deserialize)]
struct UsersResponse {
    data: Vec<UserRecord>,
}

async fn fetch_user_info(
    http: &reqwest::Client,
    users_endpoint: &str,
    access_token: &str,
) -> Result<UserRecord, PlatformError> {
    let resp = http
        .get(users_endpoint)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {access_token}"),
        )
        .send()
        .await
        .map_err(|e| PlatformError::Network {
            reason: e.without_url().to_string(),
        })?;

    let status = resp.status().as_u16();
    if status != 200 {
        let body = resp.text().await.unwrap_or_default();
        return Err(PlatformError::Http { status, body });
    }

    let parsed = resp
        .json::<UsersResponse>()
        .await
        .map_err(|e| PlatformError::Network {
            reason: format!("users response parse failed: {e}"),
        })?;

    parsed
        .data
        .into_iter()
        .next()
        .ok_or_else(|| PlatformError::Http {
            status: 200,
            body: "users endpoint returned empty data array".to_owned(),
        })
}

/// Priority: runtime env `FORGE_KICK_CLIENT_ID` → compile-time `option_env!` → `None`.
pub fn client_credentials() -> Option<String> {
    resolve_client_id(
        std::env::var("FORGE_KICK_CLIENT_ID").ok().as_deref(),
        option_env!("FORGE_KICK_CLIENT_ID"),
    )
}

fn resolve_client_id(
    runtime_env: Option<&str>,
    compile_env: Option<&'static str>,
) -> Option<String> {
    runtime_env
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| compile_env.filter(|s| !s.is_empty()).map(str::to_owned))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn kick_auth_flow_yields_local_callback_variant() {
        let flow = kick_auth_flow();
        let AuthFlow::LocalCallback {
            authorize_url,
            token_endpoint,
            redirect_path,
            scopes,
        } = flow
        else {
            unreachable!("kick_auth_flow must return LocalCallback variant");
        };
        assert_eq!(authorize_url, KICK_AUTHORIZE_ENDPOINT);
        assert_eq!(token_endpoint, KICK_TOKEN_ENDPOINT);
        assert_eq!(redirect_path, CALLBACK_PATH);
        // Why: a silently-dropped write scope breaks moderation / rewards / chat-send
        // features at runtime with an opaque 403. Pin each by literal string so an
        // accidental removal from KICK_SCOPES fails here, not in production.
        for required in [
            "chat:write",
            "moderation:ban",
            "moderation:chat_message:manage",
            "channel:write",
            "channel:rewards:read",
            "channel:rewards:write",
        ] {
            assert!(
                scopes.iter().any(|s| s == required),
                "requested scopes must contain {required}"
            );
        }
    }

    #[test]
    fn client_credentials_prefers_runtime_over_compile_time() {
        assert_eq!(
            resolve_client_id(Some("runtime_id"), Some("compile_id")),
            Some("runtime_id".to_owned()),
        );
    }

    #[test]
    fn client_credentials_falls_back_to_compile_time_when_runtime_absent() {
        assert_eq!(
            resolve_client_id(None, Some("compile_id")),
            Some("compile_id".to_owned()),
        );
    }

    #[test]
    fn client_credentials_returns_none_when_both_absent() {
        assert_eq!(resolve_client_id(None, None), None);
    }

    #[test]
    fn client_credentials_treats_empty_runtime_as_absent() {
        assert_eq!(
            resolve_client_id(Some(""), Some("compile_id")),
            Some("compile_id".to_owned()),
        );
    }

    #[test]
    fn client_credentials_treats_empty_compile_time_as_absent() {
        assert_eq!(resolve_client_id(None, Some("")), None);
    }

    #[tokio::test]
    async fn start_builds_authorize_url_with_kick_endpoint_and_redirect_quirk() {
        let mut flow = KickAuthFlow::new("test_client".to_owned());
        let url = flow.start().await.unwrap().auth_url;
        assert!(url.starts_with(KICK_AUTHORIZE_ENDPOINT));
        assert!(url.contains("client_id=test_client"));
        assert!(!url.to_lowercase().contains("client_secret"));
        let redirect_pos = url.find("redirect=127.0.0.1").unwrap();
        let redirect_uri_pos = url.find("redirect_uri=").unwrap();
        assert!(
            redirect_pos < redirect_uri_pos,
            "redirect=127.0.0.1 must precede redirect_uri in the URL"
        );
    }

    #[tokio::test]
    async fn fetch_user_info_resolves_user_id_and_name() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/users"))
            .and(header("authorization", "Bearer test_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": [{"user_id": 99, "name": "kick_streamer"}]
            })))
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let user = fetch_user_info(&http, &format!("{}/users", server.uri()), "test_token")
            .await
            .unwrap();
        assert_eq!(user.user_id, 99);
        assert_eq!(user.name, "kick_streamer");
    }

    #[tokio::test]
    async fn fetch_user_info_returns_error_on_empty_data() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/users"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": []})))
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let err = fetch_user_info(&http, &format!("{}/users", server.uri()), "tok")
            .await
            .unwrap_err();
        assert!(
            matches!(err, PlatformError::Http { status: 200, .. }),
            "empty data array must yield an Http error"
        );
    }

    #[tokio::test]
    async fn fetch_user_info_propagates_4xx_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/users"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let err = fetch_user_info(&http, &format!("{}/users", server.uri()), "bad_tok")
            .await
            .unwrap_err();
        assert!(
            matches!(err, PlatformError::Http { status: 401, .. }),
            "401 from users endpoint must yield Http error"
        );
    }
}
