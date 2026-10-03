#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use forge_monobank::{MonobankError, MonobankJar, MonobankProvider, MonobankRateLimits};
use forge_platform_core::{PlatformError, RateLimitOutcome};
use serde_json::json;
use support::{
    Bank, CLIENT_INFO, JAR, Limits, MemCreds, OTHER_JAR, OTHER_TOKEN, ScriptedLimiter, TOKEN,
    config,
};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn provider(base_url: &str, creds: Arc<MemCreds>, limits: MonobankRateLimits) -> MonobankProvider {
    MonobankProvider::new(config(base_url), creds, limits).unwrap()
}

async fn bank_server(bank: &Bank) -> MockServer {
    let server = MockServer::start().await;
    support::mount(&server, bank).await;
    server
}

async fn client_info_answering(response: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(CLIENT_INFO))
        .respond_with(response)
        .mount(&server)
        .await;
    server
}

async fn verify_against(response: ResponseTemplate) -> Result<Vec<MonobankJar>, MonobankError> {
    let server = client_info_answering(response).await;
    provider(&server.uri(), Arc::default(), Limits::open().shared())
        .verify_token(TOKEN)
        .await
}

async fn mute_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn verify_token_sends_the_token_header_and_lists_the_jars() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(CLIENT_INFO))
        .and(header("X-Token", TOKEN))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "clientId": "c",
            "name": "Streamer",
            "accounts": [{ "id": "acc-1", "balance": 1 }],
            "jars": [support::jar(JAR), { "id": "../escape", "currencyCode": 980 }],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let jars = provider(&server.uri(), Arc::default(), Limits::open().shared())
        .verify_token(&format!(" {TOKEN} "))
        .await
        .unwrap();

    assert_eq!(
        jars.iter().map(|jar| jar.id.as_str()).collect::<Vec<_>>(),
        [JAR]
    );
}

#[tokio::test]
async fn verify_token_maps_each_failure_answer_to_its_error() {
    for (response, expected) in [
        (ResponseTemplate::new(401), "unauthorized"),
        (
            ResponseTemplate::new(403)
                .set_body_json(json!({ "errorDescription": "Unknown 'X-Token'" })),
            "unauthorized",
        ),
        (ResponseTemplate::new(500), "http 500"),
        (
            ResponseTemplate::new(200).set_body_string("<html>"),
            "malformed",
        ),
    ] {
        let result = verify_against(response).await;
        let matched = match expected {
            "unauthorized" => matches!(result, Err(MonobankError::Unauthorized)),
            "http 500" => matches!(result, Err(MonobankError::Http { status: 500 })),
            _ => matches!(result, Err(MonobankError::MalformedResponse { .. })),
        };
        assert!(matched, "{expected}: {result:?}");
    }
}

#[tokio::test]
async fn rejected_token_asks_the_user_to_log_in_again() {
    let error = verify_against(ResponseTemplate::new(403))
        .await
        .unwrap_err();
    assert!(
        matches!(PlatformError::from(error), PlatformError::ReauthRequired { ref platform } if platform == "monobank")
    );
}

#[tokio::test]
async fn verify_token_rejects_a_blank_or_unsendable_token_without_a_request() {
    let server = mute_server().await;
    let provider = provider(&server.uri(), Arc::default(), Limits::open().shared());

    let blank = provider.verify_token("   ").await;
    let unsendable = provider.verify_token("tok\nen").await;

    assert!(
        matches!(blank, Err(MonobankError::MissingToken)),
        "{blank:?}"
    );
    assert!(
        matches!(unsendable, Err(MonobankError::MalformedToken)),
        "{unsendable:?}"
    );
}

#[tokio::test]
async fn too_many_requests_cools_the_endpoint_down_for_at_least_a_minute() {
    for (retry_after, secs) in [
        (Some("5"), 60),
        (Some("60"), 60),
        (Some("61"), 61),
        (Some("120"), 120),
        (None, 60),
        (Some("soon"), 60),
    ] {
        let mut response = ResponseTemplate::new(429);
        if let Some(value) = retry_after {
            response = response.insert_header("Retry-After", value);
        }
        let server = client_info_answering(response).await;
        let limits = Limits::open();

        let result = provider(&server.uri(), Arc::default(), limits.shared())
            .verify_token(TOKEN)
            .await;

        assert!(
            matches!(result, Err(MonobankError::RateLimited { retry_after_secs }) if retry_after_secs == secs),
            "{retry_after:?}: {result:?}"
        );
        assert_eq!(
            limits.client_info.throttles(),
            vec![Duration::from_secs(u64::from(secs))],
            "{retry_after:?}"
        );
    }
}

#[tokio::test]
async fn throttle_lands_only_on_the_endpoint_that_was_throttled() {
    let server = client_info_answering(ResponseTemplate::new(429)).await;
    let limits = Limits::open();

    let _ = provider(&server.uri(), Arc::default(), limits.shared())
        .verify_token(TOKEN)
        .await;

    assert!(limits.statement.throttles().is_empty());
}

#[tokio::test]
async fn local_cooldown_answers_without_a_request() {
    for (outcome, secs) in [
        (
            RateLimitOutcome::Throttled {
                wait_for: Duration::from_millis(41_200),
            },
            42,
        ),
        (RateLimitOutcome::Exhausted, 60),
    ] {
        let server = mute_server().await;
        let limits = MonobankRateLimits {
            client_info: ScriptedLimiter::answering(outcome),
            statement: ScriptedLimiter::open(),
        };

        let result = provider(&server.uri(), Arc::default(), limits)
            .verify_token(TOKEN)
            .await;

        assert!(
            matches!(result, Err(MonobankError::CoolingDown { retry_after_secs }) if retry_after_secs == secs),
            "{outcome:?}: {result:?}"
        );
    }
}

#[tokio::test]
async fn official_limits_allow_one_client_info_call_per_minute() {
    let bank = Bank::new(&[JAR]);
    let server = bank_server(&bank).await;
    let provider = provider(
        &server.uri(),
        Arc::default(),
        MonobankRateLimits::official(),
    );

    let first = provider.verify_token(TOKEN).await;
    let second = provider.verify_token(OTHER_TOKEN).await;

    assert!(first.is_ok(), "{first:?}");
    assert!(
        matches!(second, Err(MonobankError::CoolingDown { retry_after_secs }) if (59..=60).contains(&retry_after_secs)),
        "{second:?}"
    );
    assert_eq!(bank.client_info_calls(), 1);
}

#[tokio::test]
async fn saving_right_after_verifying_reuses_the_jar_list() {
    let bank = Bank::new(&[JAR]);
    let server = bank_server(&bank).await;
    let creds = Arc::new(MemCreds::default());
    let provider = provider(&server.uri(), creds.clone(), MonobankRateLimits::official());

    provider.verify_token(TOKEN).await.unwrap();
    let saved = provider.save_token(TOKEN, JAR).await;

    assert_eq!(saved.map(|jar| jar.id).ok().as_deref(), Some(JAR));
    assert_eq!(bank.client_info_calls(), 1);
}

#[tokio::test]
async fn jar_list_of_one_token_is_not_reused_for_another() {
    let bank = Bank::new(&[JAR]);
    let server = bank_server(&bank).await;
    let provider = provider(&server.uri(), Arc::default(), Limits::open().shared());

    provider.verify_token(TOKEN).await.unwrap();
    provider.verify_token(OTHER_TOKEN).await.unwrap();

    assert_eq!(bank.client_info_calls(), 2);
}

#[tokio::test]
async fn save_token_persists_the_token_and_the_selected_jar() {
    let bank = Bank::new(&[JAR, OTHER_JAR]);
    let server = bank_server(&bank).await;
    let creds = Arc::new(MemCreds::default());

    let saved = provider(&server.uri(), creds.clone(), Limits::open().shared())
        .save_token(&format!("{TOKEN}\n"), &format!(" {OTHER_JAR} "))
        .await;

    assert!(saved.is_ok(), "{saved:?}");
    assert_eq!(
        creds.stored(),
        Some(json!({ "token": TOKEN, "jar_id": OTHER_JAR }))
    );
}

#[tokio::test]
async fn save_token_stores_nothing_when_the_jar_is_not_among_the_tokens_jars() {
    let bank = Bank::new(&[JAR]);
    let server = bank_server(&bank).await;
    let creds = Arc::new(MemCreds::default());

    let result = provider(&server.uri(), creds.clone(), Limits::open().shared())
        .save_token(TOKEN, OTHER_JAR)
        .await;

    assert!(
        matches!(result, Err(MonobankError::JarNotFound)),
        "{result:?}"
    );
    assert_eq!(creds.stored(), None);
}

#[tokio::test]
async fn save_token_stores_nothing_when_monobank_rejects_the_token() {
    let server = client_info_answering(ResponseTemplate::new(403)).await;
    let creds = Arc::new(MemCreds::default());

    let result = provider(&server.uri(), creds.clone(), Limits::open().shared())
        .save_token(TOKEN, JAR)
        .await;

    assert!(
        matches!(result, Err(MonobankError::Unauthorized)),
        "{result:?}"
    );
    assert_eq!(creds.stored(), None);
}

#[tokio::test]
async fn save_token_refuses_an_unsafe_jar_id_without_a_request() {
    let server = mute_server().await;

    for jar in ["../client-info", "a/b", "a b", ""] {
        let result = provider(&server.uri(), Arc::default(), Limits::open().shared())
            .save_token(TOKEN, jar)
            .await;
        assert!(
            matches!(result, Err(MonobankError::InvalidJarId)),
            "{jar:?}: {result:?}"
        );
    }
}

#[tokio::test]
async fn select_jar_needs_a_stored_token() {
    let server = mute_server().await;

    let result = provider(&server.uri(), Arc::default(), Limits::open().shared())
        .select_jar(JAR)
        .await;

    assert!(
        matches!(result, Err(MonobankError::MissingToken)),
        "{result:?}"
    );
}

#[tokio::test]
async fn select_jar_keeps_the_stored_token_and_switches_the_jar() {
    let bank = Bank::new(&[JAR, OTHER_JAR]);
    let server = bank_server(&bank).await;
    let creds = MemCreds::with(TOKEN, Some(JAR));

    let selected = provider(&server.uri(), creds.clone(), Limits::open().shared())
        .select_jar(OTHER_JAR)
        .await;

    assert!(selected.is_ok(), "{selected:?}");
    assert_eq!(
        creds.stored(),
        Some(json!({ "token": TOKEN, "jar_id": OTHER_JAR }))
    );
}

#[tokio::test]
async fn token_and_jar_problems_need_the_user_while_transient_failures_do_not() {
    let needs_user = [
        MonobankError::MissingToken,
        MonobankError::MalformedToken,
        MonobankError::Unauthorized,
        MonobankError::MissingJar,
        MonobankError::JarNotFound,
        MonobankError::InvalidJarId,
    ];
    let transient = [
        MonobankError::CoolingDown {
            retry_after_secs: 1,
        },
        MonobankError::RateLimited {
            retry_after_secs: 60,
        },
        MonobankError::Http { status: 503 },
        MonobankError::Network {
            reason: String::new(),
        },
        MonobankError::MalformedResponse {
            reason: String::new(),
        },
        MonobankError::InvalidDonation {
            donation_id: String::new(),
            reason: String::new(),
        },
    ];
    assert!(needs_user.iter().all(MonobankError::requires_user_action));
    assert!(!transient.iter().any(MonobankError::requires_user_action));
}

#[tokio::test]
async fn unreachable_service_is_a_network_error() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };

    let result = provider(
        &format!("http://127.0.0.1:{port}"),
        Arc::default(),
        Limits::open().shared(),
    )
    .verify_token(TOKEN)
    .await;

    assert!(
        matches!(result, Err(MonobankError::Network { .. })),
        "{result:?}"
    );
}

#[tokio::test]
async fn no_failure_renders_the_token() {
    let closed_port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let mut errors = Vec::new();
    for response in [
        ResponseTemplate::new(401),
        ResponseTemplate::new(403),
        ResponseTemplate::new(429).insert_header("Retry-After", "3"),
        ResponseTemplate::new(503),
        ResponseTemplate::new(200).set_body_string(format!("{{\"jars\": \"{TOKEN}")),
    ] {
        errors.push(verify_against(response).await.unwrap_err());
    }
    errors.push(
        provider(
            &format!("http://127.0.0.1:{closed_port}"),
            Arc::default(),
            Limits::open().shared(),
        )
        .verify_token(TOKEN)
        .await
        .unwrap_err(),
    );

    for error in errors {
        let debug = format!("{error:?}");
        let platform = PlatformError::from(error);
        for rendered in [debug, platform.to_string(), format!("{platform:?}")] {
            assert!(!rendered.contains(TOKEN), "token leaked: {rendered}");
        }
    }
}
