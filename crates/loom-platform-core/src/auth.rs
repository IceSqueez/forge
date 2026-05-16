use serde::{Deserialize, Serialize};

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
mod tests {
    use super::*;

    #[allow(clippy::unwrap_used)]
    #[test]
    fn device_code_roundtrip_preserves_discriminant() {
        let flow = AuthFlow::DeviceCode {
            user_code_endpoint: "https://id.twitch.tv/oauth2/device".to_owned(),
            token_endpoint: "https://id.twitch.tv/oauth2/token".to_owned(),
            scopes: vec!["chat:read".to_owned(), "chat:edit".to_owned()],
        };
        let json = serde_json::to_string(&flow).unwrap();
        assert!(json.contains(r#""kind":"device_code""#));
        let decoded: AuthFlow = serde_json::from_str(&json).unwrap();
        assert_eq!(flow, decoded);
    }

    #[allow(clippy::unwrap_used)]
    #[test]
    fn local_callback_roundtrip_preserves_discriminant() {
        let flow = AuthFlow::LocalCallback {
            authorize_url: "https://open.trovo.live/page/login.html".to_owned(),
            token_endpoint: "https://open-api.trovo.live/openplatform/exchangetoken".to_owned(),
            redirect_path: "/oauth/callback".to_owned(),
            scopes: vec!["channel:read".to_owned()],
        };
        let json = serde_json::to_string(&flow).unwrap();
        assert!(json.contains(r#""kind":"local_callback""#));
        let decoded: AuthFlow = serde_json::from_str(&json).unwrap();
        assert_eq!(flow, decoded);
    }

    #[allow(clippy::unwrap_used)]
    #[test]
    fn none_roundtrip_preserves_discriminant() {
        let flow = AuthFlow::None {
            reason: "no public OAuth API as of 2024; chat-only via unofficial WS".to_owned(),
        };
        let json = serde_json::to_string(&flow).unwrap();
        assert!(json.contains(r#""kind":"none""#));
        let decoded: AuthFlow = serde_json::from_str(&json).unwrap();
        assert_eq!(flow, decoded);
    }
}
