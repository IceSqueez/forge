#![expect(clippy::unwrap_used, reason = "test fixtures assert directly")]
use std::ffi::OsString;
use std::sync::Arc;

use forge_events::EventPublisher;
use forge_events::test_support::NullPublisher;
use forge_platform_core::PlatformEndpoints;
use forge_platform_twitch::credentials::{StoredCredential, store_credential};
use forge_platform_twitch::{TWITCH_CREDENTIAL_ID, TwitchAssembly, endpoint_surfaces};
use forge_registry::{RunContext, SubActionRegistry};
use forge_storage::test_support::MemoryCredentials;
use forge_storage::{CredentialId, CredentialsRepo};
use forge_types::{ArgStack, EventId, SubActionConfig, SubActionOutcome};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CLEAR_CHAT: &str = "twitch.chat.clear";
const FIRST_ACCOUNT: &str = "1001";
const SECOND_ACCOUNT: &str = "2002";

struct Rig {
    server: MockServer,
    repo: Arc<MemoryCredentials>,
    registry: SubActionRegistry,
    assembly: TwitchAssembly,
}

impl Rig {
    async fn start() -> Self {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/helix/moderation/chat"))
            .respond_with(ResponseTemplate::new(reqwest::StatusCode::NO_CONTENT))
            .mount(&server)
            .await;
        let api_base = server.uri();
        let endpoints = PlatformEndpoints::resolve(endpoint_surfaces::ENDPOINT_SURFACES, |name| {
            (name == endpoint_surfaces::TWITCH_API.env_var()).then(|| OsString::from(&api_base))
        })
        .unwrap();
        let repo = Arc::new(MemoryCredentials::default());
        let bus: Arc<dyn EventPublisher> = Arc::new(NullPublisher);
        let mut registry = SubActionRegistry::new();
        let assembly = TwitchAssembly::new(
            &mut registry,
            Arc::clone(&repo) as Arc<dyn CredentialsRepo>,
            &bus,
            &endpoints,
            "identity-test-client".to_owned(),
        )
        .unwrap();
        Self {
            server,
            repo,
            registry,
            assembly,
        }
    }

    async fn sign_in_as(&self, user_id: &str) {
        store_credential(
            self.repo.as_ref(),
            &StoredCredential {
                access_token: forge_types::OAuthToken::new("access"),
                refresh_token: Some(forge_types::OAuthToken::new("refresh")),
                user_id: user_id.to_owned(),
                login: format!("login{user_id}"),
                expires_at: None,
            },
        )
        .await
        .unwrap();
    }

    async fn sign_out(&self) {
        self.repo
            .delete(&CredentialId::new(TWITCH_CREDENTIAL_ID.to_owned()))
            .await
            .unwrap();
    }

    async fn run_clear_chat(&self) -> SubActionOutcome {
        let runner = self.registry.get(CLEAR_CHAT).unwrap();
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        runner
            .execute(&SubActionConfig::new(), &ctx)
            .await
            .0
            .outcome
    }

    async fn acted_on(&self) -> Vec<(String, String)> {
        self.server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|request| {
                let query = |key: &str| {
                    request
                        .url
                        .query_pairs()
                        .find(|(name, _)| name == key)
                        .map(|(_, value)| value.into_owned())
                        .unwrap_or_default()
                };
                (query("broadcaster_id"), query("moderator_id"))
            })
            .collect()
    }
}

fn account(user_id: &str) -> (String, String) {
    (user_id.to_owned(), user_id.to_owned())
}

#[tokio::test]
async fn a_runner_acts_on_the_account_resolved_when_the_session_opened() {
    let rig = Rig::start().await;
    rig.sign_in_as(FIRST_ACCOUNT).await;
    rig.assembly.open_session().await.unwrap();

    let outcome = rig.run_clear_chat().await;

    assert_eq!(outcome, SubActionOutcome::Success);
    assert_eq!(rig.acted_on().await, vec![account(FIRST_ACCOUNT)]);
}

#[tokio::test]
async fn a_storage_change_alone_does_not_move_a_runner_to_another_account() {
    let rig = Rig::start().await;
    rig.sign_in_as(FIRST_ACCOUNT).await;
    rig.assembly.open_session().await.unwrap();

    rig.sign_in_as(SECOND_ACCOUNT).await;
    rig.run_clear_chat().await;

    assert_eq!(rig.acted_on().await, vec![account(FIRST_ACCOUNT)]);
}

#[tokio::test]
async fn reopening_the_session_after_an_account_switch_moves_runners_to_the_new_account() {
    let rig = Rig::start().await;
    rig.sign_in_as(FIRST_ACCOUNT).await;
    rig.assembly.open_session().await.unwrap();
    rig.run_clear_chat().await;

    rig.sign_in_as(SECOND_ACCOUNT).await;
    rig.assembly.open_session().await.unwrap();
    rig.run_clear_chat().await;

    assert_eq!(
        rig.acted_on().await,
        vec![account(FIRST_ACCOUNT), account(SECOND_ACCOUNT)]
    );
}

#[tokio::test]
async fn reopening_the_session_after_sign_out_stops_runners_without_calling_helix() {
    let rig = Rig::start().await;
    rig.sign_in_as(FIRST_ACCOUNT).await;
    rig.assembly.open_session().await.unwrap();

    rig.sign_out().await;
    rig.assembly.open_session().await.unwrap();
    let outcome = rig.run_clear_chat().await;

    assert!(matches!(outcome, SubActionOutcome::Failed(_)));
    assert!(rig.acted_on().await.is_empty());
}

#[tokio::test]
async fn a_runner_before_any_session_opened_fails_without_calling_helix() {
    let rig = Rig::start().await;
    rig.sign_in_as(FIRST_ACCOUNT).await;

    let outcome = rig.run_clear_chat().await;

    assert!(matches!(outcome, SubActionOutcome::Failed(_)));
    assert!(rig.acted_on().await.is_empty());
}
