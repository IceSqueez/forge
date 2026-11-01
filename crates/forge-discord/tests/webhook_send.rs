#![expect(clippy::unwrap_used, reason = "test fixtures")]

use std::sync::Arc;

use forge_discord::{DISCORD_CRED_PREFIX, DiscordClient, DiscordError, MentionPolicy};
use forge_events::EventPublisher;
use forge_events::test_support::CapturingPublisher;
use forge_storage::test_support::MemoryCredentials;
use forge_storage::{CredentialId, CredentialsRepo};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const WEBHOOK: &str = "alerts";
const HOOK_ID: &str = "1234567890";
const HOOK_TOKEN: &str = "S3CRET-WEBHOOK-TOKEN";
const RETRY_AFTER_SECS: f64 = 0.05;
const POSTED_KIND: &str = "discord.webhook.posted";
const RATE_LIMITED_KIND: &str = "discord.webhook.rate_limited";
const FAILED_KIND: &str = "discord.webhook.failed";

struct Fixture {
    server: MockServer,
    publisher: Arc<CapturingPublisher>,
    client: Arc<DiscordClient>,
}

impl Fixture {
    async fn with_saved_webhook() -> Self {
        let server = MockServer::start().await;
        let publisher = Arc::new(CapturingPublisher::default());
        let client = DiscordClient::new(
            &server.uri(),
            Arc::clone(&publisher) as Arc<dyn EventPublisher>,
            Arc::new(MemoryCredentials::default()),
        )
        .unwrap();
        client.save_webhook(WEBHOOK, &webhook_url()).await.unwrap();
        Self {
            server,
            publisher,
            client,
        }
    }

    fn kinds(&self) -> Vec<String> {
        self.publisher
            .events()
            .iter()
            .map(|event| event.kind.to_string())
            .collect()
    }
}

fn webhook_url() -> String {
    format!("https://discord.com/api/webhooks/{HOOK_ID}/{HOOK_TOKEN}")
}

fn hook_path() -> String {
    format!("/webhooks/{HOOK_ID}/{HOOK_TOKEN}")
}

fn accepted(message_id: &str) -> ResponseTemplate {
    ResponseTemplate::new(reqwest::StatusCode::OK)
        .set_body_json(serde_json::json!({ "id": message_id }))
}

fn rate_limited() -> ResponseTemplate {
    ResponseTemplate::new(reqwest::StatusCode::TOO_MANY_REQUESTS)
        .set_body_json(serde_json::json!({ "retry_after": RETRY_AFTER_SECS }))
        .insert_header("retry-after", RETRY_AFTER_SECS.to_string().as_str())
}

#[tokio::test]
async fn a_saved_webhook_posts_and_reports_the_event_and_a_healthy_send() {
    let fixture = Fixture::with_saved_webhook().await;
    Mock::given(method("POST"))
        .and(path(hook_path()))
        .respond_with(accepted("msg-1"))
        .mount(&fixture.server)
        .await;

    let message_id = fixture
        .client
        .post_text(WEBHOOK, "hello", MentionPolicy::default())
        .await
        .unwrap();

    assert_eq!(message_id, "msg-1");
    assert_eq!(fixture.kinds(), [POSTED_KIND]);
    let health = fixture.client.send_health();
    assert_eq!(
        (
            health.last_send_ok,
            health.latency_samples,
            health.errors_last_hour
        ),
        (Some(true), 1, 0)
    );
}

#[tokio::test]
async fn a_rate_limited_post_is_retried_and_recovers() {
    let fixture = Fixture::with_saved_webhook().await;
    Mock::given(method("POST"))
        .and(path(hook_path()))
        .respond_with(rate_limited())
        .up_to_n_times(1)
        .mount(&fixture.server)
        .await;
    Mock::given(method("POST"))
        .and(path(hook_path()))
        .respond_with(accepted("msg-2"))
        .mount(&fixture.server)
        .await;

    let message_id = fixture
        .client
        .post_text(WEBHOOK, "again", MentionPolicy::default())
        .await
        .unwrap();

    assert_eq!(message_id, "msg-2");
    assert_eq!(fixture.kinds(), [RATE_LIMITED_KIND, POSTED_KIND]);
    assert_eq!(fixture.client.send_health().last_send_ok, Some(true));
}

#[tokio::test]
async fn a_rejected_post_reports_a_failure_without_the_webhook_token() {
    let fixture = Fixture::with_saved_webhook().await;
    Mock::given(method("POST"))
        .and(path(hook_path()))
        .respond_with(ResponseTemplate::new(reqwest::StatusCode::BAD_REQUEST))
        .mount(&fixture.server)
        .await;

    let error = fixture
        .client
        .post_text(WEBHOOK, "rejected", MentionPolicy::default())
        .await
        .unwrap_err();

    assert!(
        matches!(error, DiscordError::BadResponse { .. }),
        "{error:?}"
    );
    assert!(!error.to_string().contains(HOOK_TOKEN));
    assert_eq!(fixture.kinds(), [FAILED_KIND]);
    let failure = fixture.publisher.events().remove(0);
    assert!(!failure.payload.to_string().contains(HOOK_TOKEN));
    assert_eq!(fixture.client.send_health().last_send_ok, Some(false));
}

#[tokio::test]
async fn a_webhook_stored_before_boot_is_listed_and_a_deleted_one_can_no_longer_post() {
    let server = MockServer::start().await;
    let creds = Arc::new(MemoryCredentials::default());
    let credential = CredentialId::new(format!("{DISCORD_CRED_PREFIX}{WEBHOOK}"));
    let stored = serde_json::json!({ "name": WEBHOOK, "url": webhook_url() });
    creds.store(&credential, &stored.to_string()).await.unwrap();
    let client = DiscordClient::new(
        &server.uri(),
        Arc::new(CapturingPublisher::default()),
        Arc::clone(&creds) as Arc<dyn CredentialsRepo>,
    )
    .unwrap();

    assert_eq!(client.list_webhooks().await.unwrap(), [WEBHOOK]);
    assert!(client.delete_webhook(WEBHOOK).await.unwrap());

    let after_delete = client
        .post_text(WEBHOOK, "gone", MentionPolicy::default())
        .await;
    assert!(
        matches!(after_delete, Err(DiscordError::WebhookNotFound { .. })),
        "{after_delete:?}"
    );
}
