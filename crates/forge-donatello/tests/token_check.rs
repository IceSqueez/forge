#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use forge_donatello::{
    DonatelloAccount, DonatelloError, DonatelloProvider, MAX_POLL_INTERVAL, MIN_POLL_INTERVAL,
};
use forge_platform_core::PlatformError;
use serde_json::json;
use support::{MemCreds, ScriptedLimiter, TOKEN, config};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn provider(
    base_url: &str,
    creds: Arc<MemCreds>,
    limiter: Arc<ScriptedLimiter>,
) -> DonatelloProvider {
    DonatelloProvider::new(config(base_url), creds, limiter).unwrap()
}

async fn me_answering(response: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me"))
        .respond_with(response)
        .mount(&server)
        .await;
    server
}

async fn verify_against(response: ResponseTemplate) -> Result<DonatelloAccount, DonatelloError> {
    let server = me_answering(response).await;
    provider(&server.uri(), Arc::default(), ScriptedLimiter::open())
        .verify_token(TOKEN)
        .await
}

#[tokio::test]
async fn verify_token_sends_the_token_header_and_reads_the_account() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me"))
        .and(header("X-Token", TOKEN))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "nickname": "streamer",
            "page": "",
            "isActive": true,
        })))
        .expect(1)
        .mount(&server)
        .await;

    let account = provider(&server.uri(), Arc::default(), ScriptedLimiter::open())
        .verify_token(&format!(" {TOKEN} "))
        .await
        .unwrap();

    assert_eq!(
        account,
        DonatelloAccount {
            nickname: Some("streamer".to_owned()),
            page: None,
        }
    );
}

#[tokio::test]
async fn verify_token_maps_each_failure_answer_to_its_error() {
    for (response, expected) in [
        (
            ResponseTemplate::new(401)
                .set_body_json(json!({ "success": false, "message": "Помилка авторизації" })),
            "unauthorized",
        ),
        (ResponseTemplate::new(404), "profile incomplete"),
        (ResponseTemplate::new(500), "http 500"),
        (
            ResponseTemplate::new(200).set_body_string("<html>"),
            "malformed",
        ),
    ] {
        let result = verify_against(response).await;
        let matched = match expected {
            "unauthorized" => matches!(result, Err(DonatelloError::Unauthorized)),
            "profile incomplete" => matches!(result, Err(DonatelloError::ProfileIncomplete)),
            "http 500" => matches!(result, Err(DonatelloError::Http { status: 500 })),
            _ => matches!(result, Err(DonatelloError::MalformedResponse { .. })),
        };
        assert!(matched, "{expected}: {result:?}");
    }
}

#[tokio::test]
async fn verify_token_rejects_a_blank_or_unsendable_token_without_a_request() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let provider = provider(&server.uri(), Arc::default(), ScriptedLimiter::open());

    let blank = provider.verify_token("   ").await;
    let unsendable = provider.verify_token("tok\nen").await;

    assert!(
        matches!(blank, Err(DonatelloError::MissingToken)),
        "{blank:?}"
    );
    assert!(
        matches!(unsendable, Err(DonatelloError::MalformedToken)),
        "{unsendable:?}"
    );
}

#[tokio::test]
async fn token_problems_require_the_user_while_transient_failures_do_not() {
    let needs_user = [
        DonatelloError::MissingToken,
        DonatelloError::MalformedToken,
        DonatelloError::Unauthorized,
        DonatelloError::ProfileIncomplete,
    ];
    let transient = [
        DonatelloError::RateLimited {
            retry_after_secs: 1,
        },
        DonatelloError::RateLimitExhausted,
        DonatelloError::Http { status: 503 },
        DonatelloError::Network {
            reason: String::new(),
        },
        DonatelloError::MalformedResponse {
            reason: String::new(),
        },
        DonatelloError::InvalidDonation {
            donation_id: String::new(),
            reason: String::new(),
        },
    ];
    assert!(needs_user.iter().all(DonatelloError::requires_user_action));
    assert!(!transient.iter().any(DonatelloError::requires_user_action));
}

#[tokio::test]
async fn too_many_requests_answer_feeds_the_retry_after_into_the_shared_limiter() {
    for (retry_after, secs) in [(Some("7"), 7), (None, 30), (Some("soon"), 30)] {
        let mut response = ResponseTemplate::new(429);
        if let Some(value) = retry_after {
            response = response.insert_header("Retry-After", value);
        }
        let server = me_answering(response).await;
        let limiter = ScriptedLimiter::open();

        let result = provider(&server.uri(), Arc::default(), Arc::clone(&limiter))
            .verify_token(TOKEN)
            .await;

        assert!(
            matches!(result, Err(DonatelloError::RateLimited { retry_after_secs }) if retry_after_secs == secs),
            "{retry_after:?}: {result:?}"
        );
        assert_eq!(
            limiter.throttles(),
            vec![Duration::from_secs(u64::from(secs))],
            "{retry_after:?}"
        );
    }
}

#[tokio::test]
async fn exhausted_client_budget_sends_no_request() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let result = provider(&server.uri(), Arc::default(), ScriptedLimiter::exhausted())
        .verify_token(TOKEN)
        .await;

    assert!(
        matches!(result, Err(DonatelloError::RateLimitExhausted)),
        "{result:?}"
    );
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
        ScriptedLimiter::open(),
    )
    .verify_token(TOKEN)
    .await;

    assert!(
        matches!(result, Err(DonatelloError::Network { .. })),
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
        ResponseTemplate::new(404),
        ResponseTemplate::new(429).insert_header("Retry-After", "3"),
        ResponseTemplate::new(503),
        ResponseTemplate::new(200).set_body_string(format!("{{\"nickname\": \"{TOKEN}")),
    ] {
        errors.push(verify_against(response).await.unwrap_err());
    }
    errors.push(
        provider(
            &format!("http://127.0.0.1:{closed_port}"),
            Arc::default(),
            ScriptedLimiter::open(),
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

#[tokio::test]
async fn save_token_stores_nothing_when_donatello_rejects_it() {
    let server = me_answering(ResponseTemplate::new(401)).await;
    let creds = Arc::new(MemCreds::default());

    let result = provider(&server.uri(), Arc::clone(&creds), ScriptedLimiter::open())
        .save_token(TOKEN)
        .await;

    assert!(
        matches!(result, Err(DonatelloError::Unauthorized)),
        "{result:?}"
    );
    assert_eq!(creds.stored(), None);
}

#[tokio::test]
async fn save_token_persists_a_verified_token() {
    let server =
        me_answering(ResponseTemplate::new(200).set_body_json(json!({ "nickname": "s" }))).await;
    let creds = Arc::new(MemCreds::default());

    let result = provider(&server.uri(), Arc::clone(&creds), ScriptedLimiter::open())
        .save_token(&format!("{TOKEN}\n"))
        .await;

    assert!(result.is_ok(), "{result:?}");
    assert!(
        creds
            .stored()
            .is_some_and(|bundle| bundle.contains(TOKEN) && !bundle.contains('\n'))
    );
}

#[tokio::test]
async fn poll_interval_is_clamped_to_the_supported_range() {
    let provider = provider(
        "http://127.0.0.1:9",
        Arc::default(),
        ScriptedLimiter::open(),
    );
    let second = Duration::from_secs(1);

    for (requested, applied) in [
        (Duration::ZERO, MIN_POLL_INTERVAL),
        (MIN_POLL_INTERVAL - second, MIN_POLL_INTERVAL),
        (MIN_POLL_INTERVAL, MIN_POLL_INTERVAL),
        (MIN_POLL_INTERVAL + second, MIN_POLL_INTERVAL + second),
        (MAX_POLL_INTERVAL, MAX_POLL_INTERVAL),
        (MAX_POLL_INTERVAL + second, MAX_POLL_INTERVAL),
    ] {
        assert_eq!(
            provider.set_poll_interval(requested),
            applied,
            "{requested:?}"
        );
    }
}
