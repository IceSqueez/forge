use forge_storage::CredentialId;
use forge_types::Secret;

use crate::client::{DiscordClient, SendRecord};
use crate::credentials::{DISCORD_CRED_PREFIX, WebhookCredential};
use crate::error::{DiscordError, ValidationFault};
use crate::route::WebhookRoute;

pub fn validate_webhook_url(url: &str) -> Result<(), DiscordError> {
    WebhookRoute::parse(url).map(|_| ())
}

fn credential_id(name: &str) -> CredentialId {
    CredentialId::new(format!("{DISCORD_CRED_PREFIX}{name}"))
}

impl DiscordClient {
    pub async fn list_webhooks(&self) -> Result<Vec<String>, DiscordError> {
        let _serial = self.webhook_registry_serial.lock().await;
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
            return Err(DiscordError::Validation(ValidationFault::WebhookNameEmpty));
        }
        let url = url.trim();
        validate_webhook_url(url)?;

        let _serial = self.webhook_registry_serial.lock().await;
        let payload = serde_json::to_string(&WebhookCredential {
            name: name.to_owned(),
            url: Secret::new(url),
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
        let _serial = self.webhook_registry_serial.lock().await;
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
        Ok(self.load_webhook(name).await?.url.expose().to_owned())
    }

    pub async fn post_test(
        &self,
        webhook_name: &str,
        url: &str,
        content: &str,
    ) -> Result<String, DiscordError> {
        let route = WebhookRoute::parse(url)?;
        let body = serde_json::json!({ "content": content });
        let record = SendRecord {
            embed_count: 0,
            register_name: false,
        };
        self.execute_post(&route, &body, webhook_name, record).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::client::tests::MockCreds;
    use crate::content::record_send;
    use forge_storage::{CredentialsRepo, StorageError};

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

    const ACCEPTED_HOSTS: [&str; 6] = [
        "discord.com",
        "discordapp.com",
        "ptb.discord.com",
        "canary.discord.com",
        "canary.discordapp.com",
        "a.b.discord.com",
    ];

    const REFUSED_URLS: [&str; 29] = [
        "http://discord.com/api/webhooks/1/tok",
        "ftp://discord.com/api/webhooks/1/tok",
        "discord.com/api/webhooks/1/tok",
        "https://evildiscord.com/api/webhooks/1/tok",
        "https://evildiscordapp.com/api/webhooks/1/tok",
        "https://evil-discord.com/api/webhooks/1/tok",
        "https://discord.community/api/webhooks/1/tok",
        "https://discord.com.attacker.io/api/webhooks/1/tok",
        "https://discord.com@attacker.io/api/webhooks/1/tok",
        "https://discord.com:8443/api/webhooks/1/tok",
        "https://attacker.io/api/webhooks/1/tok",
        "https://discord.com",
        "https://discord.com/api/webhooks/1",
        "https://discord.com/api/webhooks/1/",
        "https://discord.com/api/webhooks//tok",
        "https://discord.com/v1/webhooks/1/tok",
        "https://discord.com/api/channels/1/tok",
        "https://discord.com/api/webhooks/12a/tok",
        "https://discord.com/api/webhooks/+1/tok",
        "https://discord.com/api/webhooks/-1/tok",
        "https://discord.com/api/webhooks/18446744073709551616/tok",
        "https://discord.com/api/webhooks/\u{0661}/tok",
        "https://discord.com/api/webhooks/1/..",
        "https://discord.com/api/webhooks/1/to..ken",
        "https://discord.com/api/webhooks/1/%2e%2e",
        "https://discord.com/api/webhooks/1/tok%2Fen",
        "https://discord.com/api/webhooks/1/tok%20en",
        "",
        "not a url at all",
    ];

    #[test]
    fn validate_webhook_url_accepts_discord_and_discordapp_hosts_and_their_subdomains() {
        for host in ACCEPTED_HOSTS {
            let url = format!("https://{host}/api/webhooks/1234567890/token-value");
            assert!(
                validate_webhook_url(&url).is_ok(),
                "expected {host} to be accepted"
            );
        }
    }

    #[test]
    fn validate_webhook_url_rejects_urls_outside_the_webhook_contract() {
        for bad in REFUSED_URLS {
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
    async fn save_webhook_refuses_every_url_a_send_would_refuse_without_touching_storage() {
        let creds = MockCreds::new();
        let client = client_with(&creds);

        for bad in REFUSED_URLS {
            let err = client.save_webhook("alerts", bad).await.unwrap_err();

            assert!(
                matches!(err, DiscordError::Validation(_)),
                "{bad:?}: {err:?}"
            );
        }
        assert!(creds.keys().is_empty());
        assert!(client.webhook_names().is_empty());
    }

    #[tokio::test]
    async fn save_webhook_accepts_discord_and_discordapp_subdomains() {
        let creds = MockCreds::new();
        let client = client_with(&creds);

        for (index, host) in ACCEPTED_HOSTS.into_iter().enumerate() {
            let url = format!("https://{host}/api/webhooks/1234567890/token-value");
            client
                .save_webhook(&format!("hook-{index}"), &url)
                .await
                .unwrap();

            assert_eq!(
                stored_url(&creds, &format!("discord:hook-{index}")),
                Some(url)
            );
        }
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

    const INTERLEAVING_YIELDS: usize = 16;

    struct StalledListing {
        inner: Arc<MockCreds>,
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
    }

    impl StalledListing {
        fn over(inner: &Arc<MockCreds>) -> Arc<Self> {
            Arc::new(Self {
                inner: Arc::clone(inner),
                entered: tokio::sync::Notify::new(),
                release: tokio::sync::Notify::new(),
            })
        }
    }

    #[async_trait::async_trait]
    impl CredentialsRepo for StalledListing {
        async fn store(&self, id: &CredentialId, bundle: &str) -> Result<(), StorageError> {
            self.inner.store(id, bundle).await
        }

        async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
            self.inner.load(id).await
        }

        async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
            self.inner.delete(id).await
        }

        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            let snapshot = self.inner.list_ids().await;
            self.entered.notify_one();
            self.release.notified().await;
            snapshot
        }

        async fn last_refresh(
            &self,
            id: &CredentialId,
        ) -> Result<Option<time::OffsetDateTime>, StorageError> {
            self.inner.last_refresh(id).await
        }

        async fn mark_refreshed(&self, id: &CredentialId) -> Result<(), StorageError> {
            self.inner.mark_refreshed(id).await
        }
    }

    async fn while_a_listing_is_stalled<F, Fut, T>(
        creds: &Arc<MockCreds>,
        edit: F,
    ) -> (Arc<DiscordClient>, T)
    where
        F: FnOnce(Arc<DiscordClient>) -> Fut,
        Fut: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let stalled = StalledListing::over(creds);
        let client = DiscordClient::new_for_test_with_creds(
            Arc::clone(&stalled) as Arc<dyn CredentialsRepo>
        );
        let listing = tokio::spawn({
            let client = Arc::clone(&client);
            async move { client.list_webhooks().await }
        });
        stalled.entered.notified().await;
        let edited = tokio::spawn(edit(Arc::clone(&client)));
        for _ in 0..INTERLEAVING_YIELDS {
            tokio::task::yield_now().await;
        }
        stalled.release.notify_one();
        listing.await.unwrap().unwrap();
        (client, edited.await.unwrap())
    }

    #[tokio::test]
    async fn a_webhook_deleted_during_the_startup_listing_stays_deleted() {
        let creds = MockCreds::new();
        creds.insert(
            "discord:alerts",
            &serde_json::json!({ "url": secret_url() }).to_string(),
        );

        let (client, removed) = while_a_listing_is_stalled(&creds, |client| async move {
            client.delete_webhook("alerts").await.unwrap()
        })
        .await;

        assert!(removed);
        assert!(
            client.webhook_names().is_empty(),
            "the stale listing resurrected {:?}",
            client.webhook_names()
        );
    }

    #[tokio::test]
    async fn a_webhook_saved_during_the_startup_listing_stays_listed() {
        let creds = MockCreds::new();

        let (client, ()) = while_a_listing_is_stalled(&creds, |client| async move {
            client.save_webhook("alerts", &secret_url()).await.unwrap()
        })
        .await;

        assert_eq!(client.webhook_names(), ["alerts"]);
    }
}
