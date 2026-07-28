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

    /// Posts straight to `url` instead of resolving `webhook_name` from storage, so an unsaved endpoint can be verified before it is persisted. Never registers `webhook_name` in the saved-webhook list.
    pub async fn post_test(
        &self,
        webhook_name: &str,
        url: &str,
        content: &str,
    ) -> Result<String, DiscordError> {
        let url = url.trim();
        validate_webhook_url(url)?;
        let body = serde_json::json!({ "content": content });
        self.execute_post(url, body, webhook_name, 0, false).await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::client::tests::MockCreds;
    use crate::content::record_send;

    const SECRET: &str = "S3CRET-WEBHOOK-TOKEN";

    fn secret_url() -> String {
        format!("https://discord.com/api/webhooks/1234567890/{SECRET}")
    }

    fn client_with(creds: &Arc<MockCreds>) -> Arc<DiscordClient> {
        DiscordClient::new_for_test_with_creds(creds.creds())
    }

    fn stored_url(creds: &Arc<MockCreds>, key: &str) -> Option<String> {
        let raw = creds.peek(key)?;
        let blob: serde_json::Value = serde_json::from_str(&raw).unwrap();
        Some(blob["url"].as_str().unwrap().to_owned())
    }

    #[test]
    fn validate_webhook_url_accepts_official_and_staging_hosts() {
        for host in [
            "discord.com",
            "discordapp.com",
            "ptb.discord.com",
            "canary.discord.com",
        ] {
            let url = format!("https://{host}/api/webhooks/1234567890/token-value");
            assert!(
                validate_webhook_url(&url).is_ok(),
                "expected {host} to be accepted"
            );
        }
    }

    #[test]
    fn validate_webhook_url_rejects_urls_outside_the_webhook_contract() {
        for bad in [
            "http://discord.com/api/webhooks/1/tok",
            "ftp://discord.com/api/webhooks/1/tok",
            "discord.com/api/webhooks/1/tok",
            "https://evildiscord.com/api/webhooks/1/tok",
            "https://discord.com.attacker.io/api/webhooks/1/tok",
            "https://discord.com@attacker.io/api/webhooks/1/tok",
            "https://discord.com",
            "https://discord.com/api/webhooks/1",
            "https://discord.com/api/webhooks/1/",
            "https://discord.com/api/webhooks//tok",
            "https://discord.com/v1/webhooks/1/tok",
            "https://discord.com/api/channels/1/tok",
            "",
            "not a url at all",
        ] {
            assert!(
                matches!(validate_webhook_url(bad), Err(DiscordError::Validation(_))),
                "expected rejection for {bad:?}"
            );
        }
    }

    #[tokio::test]
    async fn no_webhook_api_error_ever_echoes_the_endpoint_secret() {
        let creds = MockCreds::new();
        creds.insert(
            "discord:broken",
            &serde_json::json!({ "url": 7 }).to_string(),
        );
        let client = client_with(&creds);
        let rejected = format!("https://attacker.io/api/webhooks/1/{SECRET}");

        let errors = vec![
            validate_webhook_url(&rejected).unwrap_err(),
            client.save_webhook("alerts", &rejected).await.unwrap_err(),
            client
                .post_test("alerts", &rejected, "hello")
                .await
                .unwrap_err(),
            client.webhook_url("broken").await.unwrap_err(),
            client.webhook_url("absent").await.unwrap_err(),
        ];

        for err in errors {
            let shown = err.to_string();
            assert!(!shown.contains(SECRET), "secret leaked in Display: {shown}");
            assert!(
                !shown.contains("attacker.io"),
                "endpoint leaked in Display: {shown}"
            );
        }
    }

    #[tokio::test]
    async fn post_test_rejects_a_non_discord_endpoint_before_any_request() {
        let creds = MockCreds::new();
        let client = client_with(&creds);

        let err = client
            .post_test("alerts", "https://192.0.2.1/api/webhooks/1/tok", "ping")
            .await
            .unwrap_err();

        assert!(matches!(err, DiscordError::Validation(_)));
    }

    #[tokio::test]
    async fn list_webhooks_returns_sorted_names_and_ignores_foreign_credentials() {
        let creds = MockCreds::new();
        creds.insert(
            "discord:zulu",
            &serde_json::json!({ "url": "u" }).to_string(),
        );
        creds.insert(
            "discord:alpha",
            &serde_json::json!({ "url": "u" }).to_string(),
        );
        creds.insert("twitch:broadcaster", "{}");
        creds.insert("obs:default", "{}");
        let client = client_with(&creds);

        assert_eq!(client.list_webhooks().await.unwrap(), ["alpha", "zulu"]);
    }

    #[tokio::test]
    async fn list_webhooks_drops_cached_names_that_storage_no_longer_has() {
        let creds = MockCreds::new();
        let client = client_with(&creds);
        {
            let mut snap = client.content_state.lock().unwrap();
            record_send(&mut snap, "ghost", None, false, true);
        }

        client.list_webhooks().await.unwrap();

        assert!(client.webhook_names().is_empty());
    }

    #[tokio::test]
    async fn save_webhook_persists_the_credential_and_extends_the_cache() {
        let creds = MockCreds::new();
        let client = client_with(&creds);

        client.save_webhook("alerts", &secret_url()).await.unwrap();

        assert_eq!(stored_url(&creds, "discord:alerts"), Some(secret_url()));
        assert_eq!(client.webhook_names(), ["alerts"]);
    }

    #[tokio::test]
    async fn save_webhook_rejects_a_blank_name_without_touching_storage() {
        let creds = MockCreds::new();
        let client = client_with(&creds);

        let err = client.save_webhook("   ", &secret_url()).await.unwrap_err();

        assert!(matches!(err, DiscordError::Validation(_)));
        assert!(creds.keys().is_empty());
    }

    #[tokio::test]
    async fn save_webhook_rejects_a_non_discord_url_without_touching_storage() {
        let creds = MockCreds::new();
        let client = client_with(&creds);

        let err = client
            .save_webhook("alerts", "https://attacker.io/api/webhooks/1/tok")
            .await
            .unwrap_err();

        assert!(matches!(err, DiscordError::Validation(_)));
        assert!(creds.keys().is_empty());
        assert!(client.webhook_names().is_empty());
    }

    #[tokio::test]
    async fn save_webhook_replaces_the_endpoint_of_an_existing_name_without_duplicating_it() {
        let creds = MockCreds::new();
        let client = client_with(&creds);
        let replacement = "https://discord.com/api/webhooks/999/replacement-token";
        client.save_webhook("alerts", &secret_url()).await.unwrap();

        client.save_webhook("alerts", replacement).await.unwrap();

        assert_eq!(
            stored_url(&creds, "discord:alerts"),
            Some(replacement.to_owned())
        );
        assert_eq!(client.webhook_names(), ["alerts"]);
    }

    #[tokio::test]
    async fn delete_webhook_removes_the_credential_and_the_cache_entry() {
        let creds = MockCreds::new();
        let client = client_with(&creds);
        client.save_webhook("alerts", &secret_url()).await.unwrap();
        client.save_webhook("clips", &secret_url()).await.unwrap();

        assert!(client.delete_webhook("alerts").await.unwrap());

        assert_eq!(creds.keys(), ["discord:clips"]);
        assert_eq!(client.webhook_names(), ["clips"]);
    }

    #[tokio::test]
    async fn delete_webhook_reports_false_for_an_unknown_name() {
        let creds = MockCreds::new();
        let client = client_with(&creds);

        assert!(!client.delete_webhook("absent").await.unwrap());
    }

    #[tokio::test]
    async fn webhook_url_returns_the_stored_endpoint() {
        let creds = MockCreds::new();
        let client = client_with(&creds);
        client.save_webhook("alerts", &secret_url()).await.unwrap();

        assert_eq!(client.webhook_url("alerts").await.unwrap(), secret_url());
    }

    #[tokio::test]
    async fn webhook_url_for_an_unknown_name_reports_webhook_not_found() {
        let creds = MockCreds::new();
        let client = client_with(&creds);

        let err = client.webhook_url("absent").await.unwrap_err();

        assert!(matches!(err, DiscordError::WebhookNotFound { .. }));
    }
}
