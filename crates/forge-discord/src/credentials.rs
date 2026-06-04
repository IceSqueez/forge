use forge_storage::{CredentialId, CredentialsRepo};
use serde::{Deserialize, Serialize};

use crate::error::DiscordError;

#[allow(dead_code)]
pub(crate) const DISCORD_CRED_PREFIX: &str = "discord:";

#[allow(dead_code)]
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

#[allow(dead_code)]
pub(crate) async fn load_all_webhooks(
    creds: &dyn CredentialsRepo,
) -> Result<Vec<WebhookCredential>, DiscordError> {
    let ids = creds
        .list_ids()
        .await
        .map_err(|e| DiscordError::Credential(e.to_string()))?;

    let mut out = Vec::new();
    for id in ids {
        if !id.as_str().starts_with(DISCORD_CRED_PREFIX) {
            continue;
        }
        let name = id.as_str()[DISCORD_CRED_PREFIX.len()..].to_owned();
        let Some(json) = creds
            .load(&id)
            .await
            .map_err(|e| DiscordError::Credential(e.to_string()))?
        else {
            continue;
        };
        let blob: serde_json::Value = serde_json::from_str(&json).map_err(DiscordError::Serde)?;
        let url = blob["url"]
            .as_str()
            .ok_or_else(|| DiscordError::Credential(format!("missing url in {id}")))?
            .to_owned();
        out.push(WebhookCredential { name, url });
    }
    Ok(out)
}

#[allow(dead_code)]
pub(crate) async fn store_webhook(
    creds: &dyn CredentialsRepo,
    name: &str,
    url: &str,
) -> Result<(), DiscordError> {
    let id = CredentialId::new(format!("{DISCORD_CRED_PREFIX}{name}"));
    let blob = serde_json::json!({ "url": url }).to_string();
    creds
        .store(&id, &blob)
        .await
        .map_err(|e| DiscordError::Credential(e.to_string()))
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
