use serde::{Deserialize, Serialize};

pub mod local_callback;
pub use local_callback::{CallbackCode, LocalCallbackDriver};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthFlow {
    DeviceCode {
        user_code_endpoint: String,
        token_endpoint: String,
        scopes: Vec<String>,
    },
    LocalCallback {
        authorize_url: String,
        token_endpoint: String,
        redirect_path: String,
        scopes: Vec<String>,
    },
    None {
        reason: String,
    },
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn auth_flow_serde_roundtrip_preserves_kind_per_variant() {
        for (flow, expected_kind) in [
            (
                AuthFlow::DeviceCode {
                    user_code_endpoint: "https://id.twitch.tv/oauth2/device".to_owned(),
                    token_endpoint: "https://id.twitch.tv/oauth2/token".to_owned(),
                    scopes: vec!["chat:read".to_owned(), "chat:edit".to_owned()],
                },
                "device_code",
            ),
            (
                AuthFlow::LocalCallback {
                    authorize_url: "https://id.twitch.tv/oauth2/authorize".to_owned(),
                    token_endpoint: "https://id.twitch.tv/oauth2/token".to_owned(),
                    redirect_path: "/oauth/callback".to_owned(),
                    scopes: vec!["chat:read".to_owned()],
                },
                "local_callback",
            ),
            (
                AuthFlow::None {
                    reason: "no public OAuth API as of 2024; chat-only via unofficial WS"
                        .to_owned(),
                },
                "none",
            ),
        ] {
            let json = serde_json::to_string(&flow).unwrap();
            assert!(
                json.contains(&format!(r#""kind":"{expected_kind}""#)),
                "kind tag for {expected_kind}: {json}"
            );
            let decoded: AuthFlow = serde_json::from_str(&json).unwrap();
            assert_eq!(flow, decoded);
        }
    }
}
