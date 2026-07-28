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
    fn stored_blob_exposes_the_url_under_the_key_the_loader_reads() {
        let cred = WebhookCredential {
            name: "clips".to_owned(),
            url: "https://discord.com/api/webhooks/999/tok".to_owned(),
        };

        let blob: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&cred).unwrap()).unwrap();

        assert_eq!(blob["name"].as_str(), Some("clips"));
        assert_eq!(blob["url"].as_str(), Some(cred.url.as_str()));
    }
}
