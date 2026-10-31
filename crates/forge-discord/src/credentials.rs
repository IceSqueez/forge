use forge_types::Secret;
use serde::{Deserialize, Serialize};

pub const DISCORD_CRED_PREFIX: &str = "discord:";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WebhookCredential {
    pub(crate) name: String,
    #[serde(serialize_with = "Secret::serialize_exposed")]
    pub(crate) url: Secret,
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEBHOOK_URL: &str = "https://discord.com/api/webhooks/123/super-secret-token";
    const SAVED_BUNDLE: &str =
        r#"{"name":"alerts","url":"https://discord.com/api/webhooks/123/super-secret-token"}"#;

    #[test]
    fn a_saved_webhook_loads_its_url_and_saves_back_byte_identical() {
        let loaded: WebhookCredential = serde_json::from_str(SAVED_BUNDLE).unwrap();

        assert_eq!(loaded.url.expose(), WEBHOOK_URL);
        assert_eq!(serde_json::to_string(&loaded).unwrap(), SAVED_BUNDLE);
    }

    #[test]
    fn debug_shows_the_webhook_name_but_never_its_url() {
        let loaded: WebhookCredential = serde_json::from_str(SAVED_BUNDLE).unwrap();

        let rendered = format!("{loaded:?}");

        assert!(!rendered.contains("super-secret-token"), "{rendered}");
        assert!(rendered.contains("alerts"), "{rendered}");
    }
}
