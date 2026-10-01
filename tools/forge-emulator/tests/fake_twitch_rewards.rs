#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use forge_emulator::fixture::TwitchAccount;
use forge_emulator::twitch::{FakeTwitch, FakeTwitchConfig, MAX_CUSTOM_REWARDS};
use forge_events::{Event, EventPublisher};
use forge_platform_core::{
    BuiltinCollections, CollectionFailure, CollectionId, CollectionItemAccess, CollectionItemId,
    PlatformEndpoints, QuickActionFieldValue, RateLimiter, RevisionWait, TokenBucketRateLimiter,
};
use forge_platform_twitch::credentials::{StoredCredential, store_credential};
use forge_platform_twitch::{
    BroadcasterTier, ChatSessionConfig, HELIX_BUDGET_CAPACITY, HELIX_BUDGET_WINDOW,
    SubscriptionTracker, TwitchChat, TwitchChatHandle, TwitchCredentialsManager,
    TwitchIntegrationBundle, TwitchLifecycle,
};
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use forge_types::OAuthToken;
use serde_json::{Value, json};
use time::OffsetDateTime;
use tokio::time::timeout;

const DEADLINE: Duration = Duration::from_secs(10);
const REWARDS_PATH: &str = "/helix/channel_points/custom_rewards";
const REWARD_ADD: &str = "channel.channel_points_custom_reward.add";
const DASHBOARD_TITLE: &str = "Dashboard hydrate";
const TIER_POLL: Duration = Duration::from_millis(10);

#[derive(Default)]
struct MemoryCredentials(Mutex<HashMap<String, String>>);

#[async_trait]
impl CredentialsRepo for MemoryCredentials {
    async fn store(&self, id: &CredentialId, bundle: &str) -> Result<(), StorageError> {
        self.0
            .lock()
            .unwrap()
            .insert(id.as_str().to_owned(), bundle.to_owned());
        Ok(())
    }

    async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
        Ok(self.0.lock().unwrap().get(id.as_str()).cloned())
    }

    async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
        Ok(self.0.lock().unwrap().remove(id.as_str()).is_some())
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(Vec::new())
    }

    async fn last_refresh(&self, _: &CredentialId) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

struct DiscardBus;

impl EventPublisher for DiscardBus {
    fn publish(&self, _: Event) {}
}

struct Forge {
    bundle: Arc<TwitchIntegrationBundle>,
    lifecycle: TwitchLifecycle,
    creds: Arc<dyn CredentialsRepo>,
    config: ChatSessionConfig,
}

impl Forge {
    async fn against(fake: &FakeTwitch, account: &TwitchAccount) -> Self {
        let creds: Arc<dyn CredentialsRepo> = Arc::new(MemoryCredentials::default());
        store_credential(
            creds.as_ref(),
            &StoredCredential {
                access_token: OAuthToken::new(account.access_token.clone()),
                refresh_token: None,
                user_id: account.user_id.clone(),
                login: account.login.clone(),
                expires_at: Some(SystemTime::now() + Duration::from_secs(3600)),
            },
        )
        .await
        .unwrap();
        let overrides: HashMap<_, _> = fake.endpoint_overrides().into_iter().collect();
        let endpoints =
            PlatformEndpoints::resolve(|variable| overrides.get(variable).map(OsString::from))
                .unwrap();
        let config = ChatSessionConfig {
            client_id: account.client_id.clone(),
            broadcaster_id: account.user_id.clone(),
            user_id: account.user_id.clone(),
            endpoints,
        };
        let lifecycle = TwitchLifecycle::new();
        let bus: Arc<dyn EventPublisher> = Arc::new(DiscardBus);
        let rate_limiter: Arc<dyn RateLimiter> = Arc::new(TokenBucketRateLimiter::new(
            HELIX_BUDGET_CAPACITY,
            HELIX_BUDGET_WINDOW,
        ));
        let bundle = TwitchIntegrationBundle::new(
            Some(account.login.clone()),
            config.clone(),
            bus,
            Arc::clone(&creds),
            Arc::new(TwitchCredentialsManager::new(
                Arc::clone(&creds),
                account.client_id.clone(),
            )),
            SubscriptionTracker::default(),
            rate_limiter,
            lifecycle.clone(),
        );
        Self {
            bundle,
            lifecycle,
            creds,
            config,
        }
    }

    fn start_chat(&self) -> TwitchChatHandle {
        TwitchChat::new(
            Arc::new(TwitchCredentialsManager::new(
                Arc::clone(&self.creds),
                self.config.client_id.clone(),
            )),
            self.config.clone(),
            Arc::new(DiscardBus),
            SubscriptionTracker::default(),
            self.lifecycle.clone(),
        )
        .start()
    }
}

async fn start(dashboard_rewards: usize) -> (FakeTwitch, Forge) {
    let account = TwitchAccount::default();
    let fake = FakeTwitch::start(FakeTwitchConfig::for_account(&account))
        .await
        .unwrap();
    for index in 0..dashboard_rewards {
        let title = if index == 0 {
            DASHBOARD_TITLE.to_owned()
        } else {
            format!("Dashboard {index}")
        };
        fake.seed_dashboard_reward(&title);
    }
    let forge = Forge::against(&fake, &account).await;
    (fake, forge)
}

fn rewards() -> CollectionId {
    CollectionId::new("rewards")
}

fn draft(title: &str) -> BTreeMap<String, QuickActionFieldValue> {
    BTreeMap::from([
        (
            "title".to_owned(),
            QuickActionFieldValue::Text(title.to_owned()),
        ),
        ("cost".to_owned(), QuickActionFieldValue::Int(300)),
        (
            "prompt".to_owned(),
            QuickActionFieldValue::Text(String::new()),
        ),
        ("max_per_stream".to_owned(), QuickActionFieldValue::Int(2)),
    ])
}

fn authorized(method: reqwest::Method, fake: &FakeTwitch, query: &str) -> reqwest::RequestBuilder {
    let account = TwitchAccount::default();
    reqwest::Client::new()
        .request(
            method,
            format!("{}{REWARDS_PATH}?{query}", fake.api_base_url()),
        )
        .header("Authorization", format!("Bearer {}", account.access_token))
        .header("Client-Id", account.client_id)
}

#[tokio::test]
async fn forge_lists_a_dashboard_reward_read_only_beside_its_own() {
    let (_fake, forge) = start(1).await;
    let own = forge
        .bundle
        .create(&rewards(), &draft("Stretch"))
        .await
        .unwrap();

    let listed = forge.bundle.list(&rewards()).await.unwrap();

    let access: Vec<(String, bool)> = listed
        .iter()
        .map(|item| {
            (
                item.title.clone(),
                item.access == CollectionItemAccess::Manageable,
            )
        })
        .collect();
    assert_eq!(
        access,
        vec![
            (DASHBOARD_TITLE.to_owned(), false),
            (own.title.clone(), true)
        ]
    );
}

#[tokio::test]
async fn forge_puts_the_duplicate_title_refusal_on_the_title_field() {
    let (_fake, forge) = start(1).await;

    let outcome = forge
        .bundle
        .create(&rewards(), &draft(&DASHBOARD_TITLE.to_uppercase()))
        .await;

    assert!(
        matches!(
            &outcome,
            Err(CollectionFailure::InvalidInput { field: Some(field), .. }) if field == "title"
        ),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn forge_reports_capacity_when_the_channel_already_has_fifty_rewards() {
    let (fake, forge) = start(MAX_CUSTOM_REWARDS).await;

    let outcome = forge
        .bundle
        .create(&rewards(), &draft("One too many"))
        .await;

    assert_eq!(
        (outcome.map(|_| ()), fake.rewards().len()),
        (Err(CollectionFailure::CapacityReached), MAX_CUSTOM_REWARDS)
    );
}

#[tokio::test]
async fn forge_reports_not_owned_when_touching_a_dashboard_reward() {
    let (fake, forge) = start(1).await;
    let dashboard = CollectionItemId::new(fake.rewards()[0].id.clone());
    timeout(DEADLINE, async {
        while forge.bundle.tier() == BroadcasterTier::Standard {
            tokio::time::sleep(TIER_POLL).await;
        }
    })
    .await
    .expect("forge resolved the affiliate tier from the fake");

    let toggle = forge
        .bundle
        .set_toggle(&rewards(), &dashboard, "paused", true)
        .await;
    let delete = forge.bundle.delete(&rewards(), &dashboard).await;

    assert_eq!(
        (toggle.map(|_| ()), delete),
        (
            Err(CollectionFailure::NotOwned),
            Err(CollectionFailure::NotOwned)
        )
    );
}

#[tokio::test]
async fn forge_edits_toggles_and_deletes_its_own_reward_on_the_fake() {
    let (fake, forge) = start(0).await;
    let created = forge
        .bundle
        .create(&rewards(), &draft("Hydrate"))
        .await
        .unwrap();

    forge
        .bundle
        .update(&rewards(), &created.id, &draft("Hydrate now"))
        .await
        .unwrap();
    forge
        .bundle
        .set_toggle(&rewards(), &created.id, "paused", true)
        .await
        .unwrap();
    let after_edit = fake.rewards();
    forge.bundle.delete(&rewards(), &created.id).await.unwrap();

    assert_eq!(
        (
            after_edit[0].title.as_str(),
            after_edit[0].is_paused,
            after_edit[0].max_per_stream,
            fake.rewards().len()
        ),
        ("Hydrate now", true, Some(2), 0)
    );
}

#[tokio::test]
async fn forge_hears_its_own_reward_write_echo_back_as_a_revision_bump() {
    let (fake, forge) = start(0).await;
    let _chat = forge.start_chat();
    fake.wait_for("forge's reward-add subscription", DEADLINE, |ledger| {
        ledger
            .subscriptions
            .iter()
            .any(|subscription| subscription.subscription_type == REWARD_ADD)
            .then_some(())
    })
    .await
    .unwrap();
    let mut revisions = forge.bundle.revisions();

    forge
        .bundle
        .create(&rewards(), &draft("Hydrate"))
        .await
        .unwrap();

    assert_eq!(
        timeout(DEADLINE, revisions.changed()).await,
        Ok(RevisionWait::Changed)
    );
}

#[tokio::test]
async fn fake_deletes_answer_no_content() {
    let account = TwitchAccount::default();
    let fake = FakeTwitch::start(FakeTwitchConfig::for_account(&account))
        .await
        .unwrap();
    let created: Value = authorized(
        reqwest::Method::POST,
        &fake,
        &format!("broadcaster_id={}", account.user_id),
    )
    .json(&json!({ "title": "Hydrate", "cost": 1 }))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    let id = created["data"][0]["id"].as_str().unwrap();

    let response = authorized(
        reqwest::Method::DELETE,
        &fake,
        &format!("broadcaster_id={}&id={id}", account.user_id),
    )
    .send()
    .await
    .unwrap();

    assert_eq!(
        (
            response.status(),
            response.bytes().await.unwrap().is_empty()
        ),
        (reqwest::StatusCode::NO_CONTENT, true)
    );
}

#[tokio::test]
async fn fake_only_manageable_listing_hides_rewards_another_app_owns() {
    let account = TwitchAccount::default();
    let fake = FakeTwitch::start(FakeTwitchConfig::for_account(&account))
        .await
        .unwrap();
    fake.seed_dashboard_reward(DASHBOARD_TITLE);

    let listed: Value = authorized(
        reqwest::Method::GET,
        &fake,
        &format!(
            "broadcaster_id={}&only_manageable_rewards=true",
            account.user_id
        ),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();

    assert_eq!(listed["data"], json!([]));
}
