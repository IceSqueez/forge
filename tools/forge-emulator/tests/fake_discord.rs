#![allow(clippy::unwrap_used)]
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_discord::{DiscordClient, MentionPolicy};
use forge_emulator::EmulatorError;
use forge_emulator::fakes::discord::{FakeDiscord, UNKNOWN_WEBHOOK_CODE};
use forge_emulator::fixture::{DiscordWebhook, Fixture};
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use serde_json::{Value, json};
use time::OffsetDateTime;

mod support;
use support::QuietPublisher;

const ALERTS: &str = "alerts";

async fn fake() -> (FakeDiscord, String) {
    let fake = FakeDiscord::start(&[ALERTS.to_owned()]).await.unwrap();
    let url = loopback_webhook_url(&fake);
    (fake, url)
}

fn loopback_webhook_url(fake: &FakeDiscord) -> String {
    let saved = fake.webhook_url(ALERTS).unwrap();
    let (_, webhook_route) = saved.split_once("/api/").unwrap();
    let (_, api_base) = fake.endpoint_override();
    format!("{api_base}/{webhook_route}")
}

fn http() -> reqwest::Client {
    reqwest::Client::new()
}

#[tokio::test]
async fn post_with_wait_answers_the_created_message_and_records_the_payload() {
    let (fake, url) = fake().await;

    let response = http()
        .post(format!("{url}?wait=true"))
        .json(&json!({
            "content": "<@&42> live",
            "embeds": [{ "title": "Live" }],
            "allowed_mentions": { "parse": ["users", "roles"] }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let message: Value = response.json().await.unwrap();
    let posts = fake.posts();
    assert_eq!(posts.len(), 1);
    let post = &posts[0];
    assert_eq!(post.message_id.as_deref(), message["id"].as_str());
    assert_eq!(message["content"], "<@&42> live");
    assert_eq!(post.webhook.as_deref(), Some(ALERTS));
    assert_eq!(post.content.as_deref(), Some("<@&42> live"));
    assert_eq!(post.embeds, [json!({ "title": "Live" })]);
    assert_eq!(
        post.mention_parse(),
        Some(vec!["roles".to_owned(), "users".to_owned()])
    );
}

#[tokio::test]
async fn post_without_wait_answers_no_content_and_mints_distinct_message_ids() {
    let (fake, url) = fake().await;

    for _ in 0..2 {
        let status = http()
            .post(&url)
            .json(&json!({ "content": "hi" }))
            .send()
            .await
            .unwrap()
            .status();
        assert_eq!(status.as_u16(), 204);
    }

    let posts = fake.posts();
    assert!(
        posts
            .iter()
            .all(|post| !post.wait && post.allowed_mentions.is_none())
    );
    assert_ne!(posts[0].message_id, posts[1].message_id);
}

#[tokio::test]
async fn unknown_webhook_answers_not_found_with_the_discord_error_code() {
    let (fake, url) = fake().await;
    let wrong_token = format!("{}-revoked", url);

    let response = http()
        .post(format!("{wrong_token}?wait=true"))
        .json(&json!({ "content": "hi" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 404);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["code"], UNKNOWN_WEBHOOK_CODE);
    assert_eq!(fake.posts()[0].webhook, None);
}

#[tokio::test]
async fn scripted_rate_limit_answers_one_429_with_retry_after_then_accepts_again() {
    let (fake, url) = fake().await;
    fake.rate_limit_next(0.25);
    let send = || {
        http()
            .post(format!("{url}?wait=true"))
            .json(&json!({ "content": "hi" }))
            .send()
    };

    let limited = send().await.unwrap();
    assert_eq!(limited.status().as_u16(), 429);
    assert_eq!(limited.headers()["retry-after"], "0.25");
    let body: Value = limited.json().await.unwrap();
    assert_eq!(body["retry_after"], 0.25);

    assert_eq!(send().await.unwrap().status().as_u16(), 200);
    let statuses: Vec<u16> = fake.posts().iter().map(|post| post.status).collect();
    assert_eq!(statuses, [429, 200]);
}

#[tokio::test]
async fn edit_and_delete_address_the_message_under_the_webhook() {
    let (fake, url) = fake().await;

    let edited = http()
        .patch(format!("{url}/messages/777"))
        .json(&json!({ "content": "edited", "allowed_mentions": { "parse": ["users"] } }))
        .send()
        .await
        .unwrap();
    let deleted = http()
        .delete(format!("{url}/messages/777"))
        .send()
        .await
        .unwrap();

    assert_eq!(edited.status().as_u16(), 200);
    assert_eq!(deleted.status().as_u16(), 204);
    let recorded: Vec<(String, Option<String>)> = fake
        .posts()
        .into_iter()
        .map(|post| (post.method, post.message_id))
        .collect();
    assert_eq!(
        recorded,
        [
            ("PATCH".to_owned(), Some("777".to_owned())),
            ("DELETE".to_owned(), Some("777".to_owned())),
        ]
    );
}

#[tokio::test]
async fn method_the_webhook_does_not_model_is_refused() {
    let (_fake, url) = fake().await;

    let status = http().patch(&url).send().await.unwrap().status();

    assert_eq!(status.as_u16(), 405);
}

#[tokio::test]
async fn start_refuses_a_blank_or_repeated_webhook_name() {
    for names in [
        vec!["   ".to_owned()],
        vec![ALERTS.to_owned(), ALERTS.to_owned()],
    ] {
        let refused = FakeDiscord::start(&names).await;
        assert!(
            matches!(refused, Err(EmulatorError::InvalidFakeConfig { .. })),
            "{names:?}"
        );
    }
}

#[tokio::test]
async fn addressed_fixture_seeds_each_declared_webhook_with_a_url_forge_accepts() {
    let (fake, _) = fake().await;
    let fixture = Fixture {
        discord_webhooks: vec![DiscordWebhook {
            name: ALERTS.to_owned(),
            url: String::new(),
        }],
        ..Fixture::default()
    };

    let addressed = fake.addressed(&fixture);

    let seeded = &addressed.discord_webhooks[0].url;
    assert_eq!(Some(seeded), fake.webhook_url(ALERTS).as_ref());
    assert!(
        forge_discord::validate_webhook_url(seeded).is_ok(),
        "{seeded}"
    );
}

#[tokio::test]
async fn the_endpoint_override_names_the_discord_api_variable_and_a_loopback_api_base() {
    let (fake, _) = fake().await;

    let (variable, api_base) = fake.endpoint_override();
    let endpoints = forge_platform_core::PlatformEndpoints::resolve(|name| {
        (name == variable).then(|| api_base.clone().into())
    })
    .unwrap();

    assert_eq!(
        endpoints.base_url(forge_platform_core::EndpointSurface::DiscordApi),
        api_base
    );
}

struct MemoryCreds {
    store: Mutex<HashMap<String, String>>,
}

#[async_trait]
impl CredentialsRepo for MemoryCreds {
    async fn store(&self, id: &CredentialId, plaintext: &str) -> Result<(), StorageError> {
        self.store
            .lock()
            .unwrap()
            .insert(id.as_str().to_owned(), plaintext.to_owned());
        Ok(())
    }

    async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
        Ok(self.store.lock().unwrap().get(id.as_str()).cloned())
    }

    async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
        Ok(self.store.lock().unwrap().remove(id.as_str()).is_some())
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(self
            .store
            .lock()
            .unwrap()
            .keys()
            .map(|key| CredentialId::new(key.clone()))
            .collect())
    }

    async fn last_refresh(&self, _: &CredentialId) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

fn forge_client(fake: &FakeDiscord, saved: &str) -> Arc<DiscordClient> {
    let creds = MemoryCreds {
        store: Mutex::new(HashMap::from([(
            format!("discord:{ALERTS}"),
            json!({ "url": saved }).to_string(),
        )])),
    };
    let (variable, api_base) = fake.endpoint_override();
    let endpoints = forge_platform_core::PlatformEndpoints::resolve(|name| {
        (name == variable).then(|| api_base.clone().into())
    })
    .unwrap();
    DiscordClient::new(
        endpoints.base_url(forge_platform_core::EndpointSurface::DiscordApi),
        Arc::new(QuietPublisher),
        Arc::new(creds),
    )
    .unwrap()
}

fn saved_url(fake: &FakeDiscord) -> String {
    fake.webhook_url(ALERTS).unwrap()
}

#[tokio::test]
async fn forge_role_ping_post_reaches_the_fake_with_its_message_id_and_mentions() {
    let (fake, _) = fake().await;
    let client = forge_client(&fake, &saved_url(&fake));

    let message_id = client
        .post_text_with_mentions(
            ALERTS,
            "<@&42> we are live",
            MentionPolicy {
                allow_roles: true,
                allow_everyone: false,
            },
        )
        .await
        .unwrap();

    let post = &fake.posts()[0];
    assert_eq!(post.message_id.as_deref(), Some(message_id.as_str()));
    assert_eq!(
        post.mention_parse(),
        Some(vec!["roles".to_owned(), "users".to_owned()])
    );
}

#[tokio::test]
async fn forge_file_upload_reaches_the_fake_with_its_caption_and_file_name() {
    let (fake, _) = fake().await;
    let client = forge_client(&fake, &saved_url(&fake));

    client
        .send_file(
            ALERTS,
            Some("clip"),
            "clip.png",
            &[0, 13, 10, 255],
            MentionPolicy::default(),
        )
        .await
        .unwrap();

    let post = &fake.posts()[0];
    assert_eq!(
        (post.content.as_deref(), post.files.as_slice()),
        (Some("clip"), ["clip.png".to_owned()].as_slice())
    );
}

#[tokio::test]
async fn forge_retries_once_after_the_fake_rate_limits_its_post() {
    let (fake, _) = fake().await;
    fake.rate_limit_next(0.01);
    let client = forge_client(&fake, &saved_url(&fake));

    client
        .post_text_with_mentions(ALERTS, "hi", MentionPolicy::default())
        .await
        .unwrap();

    let statuses: Vec<u16> = fake.posts().iter().map(|post| post.status).collect();
    assert_eq!(statuses, [429, 200]);
}

#[tokio::test]
async fn a_body_that_does_not_decode_is_refused_and_recorded_as_malformed() {
    let multipart = "--edge\r\nContent-Disposition: form-data; name=\"payload_json\"\r\n\r\n{broken\r\n--edge--\r\n";
    for (content_type, body) in [
        ("application/json", "{broken"),
        ("multipart/form-data; boundary=edge", multipart),
    ] {
        let (fake, url) = fake().await;

        let response = http()
            .post(&url)
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(body)
            .send()
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "{content_type}"
        );
        let posts = fake.posts();
        assert_eq!(posts.len(), 1, "{content_type}");
        assert!(posts[0].body_error.is_some(), "{content_type}");
    }
}

#[tokio::test]
async fn forge_post_to_a_revoked_webhook_surfaces_the_404_without_the_token() {
    let (fake, _) = fake().await;
    let saved = saved_url(&fake);
    let (_, token) = saved.rsplit_once('/').unwrap();
    assert!(!token.is_empty());
    let revoked = format!("{saved}-revoked");
    let client = forge_client(&fake, &revoked);

    let err = client
        .post_text_with_mentions(ALERTS, "hi", MentionPolicy::default())
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        forge_discord::DiscordError::BadResponse { status: 404, .. }
    ));
    assert!(!err.to_string().contains(token), "{err}");
    assert!(!format!("{err:?}").contains(token), "{err:?}");
}
