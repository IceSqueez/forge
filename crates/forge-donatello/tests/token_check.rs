#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use forge_donatello::{
    DonatelloAccount, DonatelloError, DonatelloProvider, MAX_POLL_INTERVAL, MIN_POLL_INTERVAL,
    default_rate_limiter,
};
use forge_net::RateLimitOutcome;
use forge_platform_core::PlatformError;
use serde_json::json;
use support::{MemCreds, ScriptedLimiter, TOKEN};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

fn provider(
    base_url: &str,
    creds: Arc<MemCreds>,
    limiter: Arc<ScriptedLimiter>,
) -> DonatelloProvider {
    support::provider_on(base_url, creds, limiter)
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
    let server = MockServer::builder().start().await;
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
    let server = MockServer::builder().start().await;
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
    let result = provider(
        "http://127.0.0.1:0",
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

struct EchoToken {
    status: u16,
    retry_after: Option<&'static str>,
}

impl Respond for EchoToken {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let echoed = request
            .headers
            .get("x-token")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let mut response = ResponseTemplate::new(self.status)
            .insert_header("Location", format!("/moved?token={echoed}"))
            .insert_header("X-Echo", echoed.as_str())
            .set_body_string(format!(
                "{{\"message\": \"bad token {echoed}\", \"nickname\": {echoed}"
            ));
        if let Some(value) = self.retry_after {
            response = response.insert_header("Retry-After", value);
        }
        response
    }
}

#[tokio::test]
async fn no_failure_renders_a_token_the_server_echoes_back() {
    let mut errors = Vec::new();
    for (status, retry_after) in [
        (200, None),
        (302, None),
        (400, None),
        (401, None),
        (403, None),
        (404, None),
        (429, Some("3")),
        (500, None),
        (503, Some("4")),
        (503, None),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(EchoToken {
                status,
                retry_after,
            })
            .mount(&server)
            .await;
        let result = provider(&server.uri(), Arc::default(), ScriptedLimiter::open())
            .verify_token(TOKEN)
            .await;
        errors.push(result.unwrap_err());
    }
    errors.push(
        provider(
            "http://127.0.0.1:0",
            Arc::default(),
            ScriptedLimiter::open(),
        )
        .verify_token(TOKEN)
        .await
        .unwrap_err(),
    );

    for error in errors {
        let rendered_error = [format!("{error:?}"), error.to_string()];
        let platform = PlatformError::from(error);
        for rendered in rendered_error
            .into_iter()
            .chain([platform.to_string(), format!("{platform:?}")])
        {
            assert!(!rendered.contains(TOKEN), "token leaked: {rendered}");
        }
    }
}

#[tokio::test]
async fn service_unavailable_is_a_rate_limit_only_when_it_names_a_retry_after() {
    for (retry_after, expected) in [(Some("9"), "rate limited 9"), (None, "http 503")] {
        let mut response = ResponseTemplate::new(503);
        if let Some(value) = retry_after {
            response = response.insert_header("Retry-After", value);
        }
        let result = verify_against(response).await;
        let matched = match expected {
            "rate limited 9" => matches!(
                result,
                Err(DonatelloError::RateLimited {
                    retry_after_secs: 9
                })
            ),
            _ => matches!(result, Err(DonatelloError::Http { status: 503 })),
        };
        assert!(matched, "{expected}: {result:?}");
    }
}

#[tokio::test]
async fn a_throttled_client_budget_is_waited_out_before_the_request() {
    let server =
        me_answering(ResponseTemplate::new(200).set_body_json(json!({ "nickname": "s" }))).await;
    let limiter = ScriptedLimiter::throttled_once(Duration::from_millis(20));

    let result = provider(&server.uri(), Arc::default(), Arc::clone(&limiter))
        .verify_token(TOKEN)
        .await;

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(limiter.acquires(), 2);
}

#[tokio::test]
async fn the_default_budget_grants_twenty_calls_then_paces_one_call_per_three_seconds() {
    let limiter = default_rate_limiter();
    for call in 0..20 {
        assert_eq!(
            limiter.acquire(1).await,
            Ok(RateLimitOutcome::Granted),
            "call {call}"
        );
    }

    let twenty_first = limiter.acquire(1).await;

    let Ok(RateLimitOutcome::Throttled { wait_for }) = twenty_first else {
        panic!("expected the 21st call to be throttled, got {twenty_first:?}");
    };
    assert!(
        wait_for > Duration::from_secs(2) && wait_for <= Duration::from_secs(3),
        "{wait_for:?}"
    );
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
