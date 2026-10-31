#![allow(clippy::unwrap_used)]
use forge_platform_twitch::vocabulary::topics;
use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use forge_emulator::fakes::twitch::{FakeTwitch, FakeTwitchConfig, MAX_CUSTOM_REWARDS};
use forge_emulator::fixture::TwitchAccount;
use forge_events::EventPublisher;
use forge_platform_core::{
    BuiltinCollections, ChatPlatform, CollectionFailure, CollectionId, CollectionItemAccess,
    CollectionItemId, PlatformEndpoints, QuickActionFieldValue, RevisionWait,
};
use forge_platform_twitch::credentials::{StoredCredential, store_credential};
use forge_platform_twitch::{BroadcasterTier, TwitchAssembly, TwitchIntegrationBundle};
use forge_registry::SubActionRegistry;
use forge_storage::CredentialsRepo;
use forge_types::OAuthToken;
use serde_json::{Value, json};
use tokio::time::timeout;

mod support;

use forge_storage::test_support::MemoryCredentials;

const DEADLINE: Duration = Duration::from_secs(30);
const REWARDS_PATH: &str = "/helix/channel_points/custom_rewards";
const REWARD_ADD: &str = topics::CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_ADD;
const DASHBOARD_TITLE: &str = "Dashboard hydrate";

struct Forge {
    bundle: Arc<TwitchIntegrationBundle>,
    platform: Arc<dyn ChatPlatform>,
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
        let bus: Arc<dyn EventPublisher> = Arc::new(support::QuietPublisher);
        let assembly = TwitchAssembly::new(
            &mut SubActionRegistry::new(),
            creds,
            &bus,
            &endpoints,
            account.client_id.clone(),
        )
        .unwrap();
        let session = assembly.open_session().await.unwrap();
        let platform = session.chat_platform();
        let bundle = session.into_bundle().unwrap();
        Self { bundle, platform }
    }

    async fn connect(&self) {
        self.platform.connect().await.unwrap();
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
    let mut tiers = forge.bundle.tier_changes();
    timeout(
        DEADLINE,
        tiers.wait_for(|tier| *tier != BroadcasterTier::Standard),
    )
    .await
    .expect("forge resolved the affiliate tier from the fake")
    .expect("the bundle outlives the wait");

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
    forge.connect().await;
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
