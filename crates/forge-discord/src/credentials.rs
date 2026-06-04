use serde::{Deserialize, Serialize};

pub(crate) const DISCORD_CRED_PREFIX: &str = "discord:";

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct WebhookCredential {
    pub(crate) name: String,
    pub(crate) url: String,
}

/// URL is redacted in the `Debug` output; never printed in logs.
impl std::fmt::Debug for WebhookCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebhookCredential")
            .field("name", &self.name)
            .field("url", &"***")
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn discord_cred_prefix_value() {
        assert_eq!(DISCORD_CRED_PREFIX, "discord:");
    }

    #[test]
    fn debug_redacts_url() {
        let cred = WebhookCredential {
            name: "alerts".to_owned(),
            url: "https://discord.com/api/webhooks/123/super-secret-token".to_owned(),
        };
        let s = format!("{cred:?}");
        assert!(
            !s.contains("super-secret-token"),
            "token must not appear in Debug output"
        );
        assert!(s.contains("***"));
        assert!(s.contains("alerts"));
    }

    #[test]
    fn serde_roundtrip() {
        let cred = WebhookCredential {
            name: "clips".to_owned(),
            url: "https://discord.com/api/webhooks/999/tok".to_owned(),
        };
        let json = serde_json::to_string(&cred).unwrap();
        let back: WebhookCredential = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, cred.name);
        assert_eq!(back.url, cred.url);
    }
}
