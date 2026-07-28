use forge_storage::CredentialId;

use crate::client::DiscordClient;
use crate::credentials::{DISCORD_CRED_PREFIX, WebhookCredential};
use crate::error::DiscordError;

const WEBHOOK_HOSTS: [&str; 2] = ["discord.com", "discordapp.com"];

/// Never embeds the candidate URL in the returned error - the URL is the credential.
pub fn validate_webhook_url(url: &str) -> Result<(), DiscordError> {
    let rejected = || DiscordError::Validation("not a discord webhook endpoint".to_owned());

    let rest = url.trim().strip_prefix("https://").ok_or_else(rejected)?;
    let (host, path) = rest.split_once('/').ok_or_else(rejected)?;
    let host_allowed = WEBHOOK_HOSTS
        .iter()
        .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")));

    let mut segments = path.split('/');
    let route_allowed = segments.next() == Some("api") && segments.next() == Some("webhooks");
    let has_id = segments.next().is_some_and(|segment| !segment.is_empty());
    let has_token = segments.next().is_some_and(|segment| !segment.is_empty());

    if host_allowed && route_allowed && has_id && has_token {
        Ok(())
    } else {
        Err(rejected())
    }
}

fn credential_id(name: &str) -> CredentialId {
    CredentialId::new(format!("{DISCORD_CRED_PREFIX}{name}"))
}

impl DiscordClient {
    /// Re-syncs the cached name list from storage, so a webhook added elsewhere becomes sendable without a restart.
    pub async fn list_webhooks(&self) -> Result<Vec<String>, DiscordError> {
        let ids = self
            .creds
            .list_ids()
            .await
            .map_err(|e| DiscordError::Credential(e.to_string()))?;

        let mut names: Vec<String> = ids
            .iter()
            .filter_map(|id| id.as_str().strip_prefix(DISCORD_CRED_PREFIX))
            .map(str::to_owned)
            .collect();
        names.sort_unstable();

        let mut snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
        snap.webhook_names.clone_from(&names);
        drop(snap);

        Ok(names)
    }

    pub async fn save_webhook(&self, name: &str, url: &str) -> Result<(), DiscordError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DiscordError::Validation("webhook name is empty".to_owned()));
        }
        let url = url.trim();
        validate_webhook_url(url)?;

        let payload = serde_json::to_string(&WebhookCredential {
            name: name.to_owned(),
            url: url.to_owned(),
        })?;
        self.creds
            .store(&credential_id(name), &payload)
            .await
            .map_err(|e| DiscordError::Credential(e.to_string()))?;

        let mut snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
        if !snap.webhook_names.iter().any(|known| known == name) {
            snap.webhook_names.push(name.to_owned());
            snap.webhook_names.sort_unstable();
        }

        Ok(())
    }

    pub async fn delete_webhook(&self, name: &str) -> Result<bool, DiscordError> {
        let removed = self
            .creds
            .delete(&credential_id(name))
            .await
            .map_err(|e| DiscordError::Credential(e.to_string()))?;

        let mut snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
        snap.webhook_names.retain(|known| known != name);
        snap.webhook_last_ok.remove(name);

        Ok(removed)
    }

    pub async fn webhook_url(&self, name: &str) -> Result<String, DiscordError> {
        Ok(self.load_webhook(name).await?.url)
    }

    /// Posts straight to `url` instead of resolving `webhook_name` from storage, so an unsaved endpoint can be verified before it is persisted.
    pub async fn post_test(
        &self,
        webhook_name: &str,
        url: &str,
        content: &str,
    ) -> Result<String, DiscordError> {
        let url = url.trim();
        validate_webhook_url(url)?;
        let body = serde_json::json!({ "content": content });
        self.execute_post(url, body, webhook_name, 0).await
    }
}
