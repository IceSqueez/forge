#![allow(clippy::unwrap_used)]
mod gateway_streaming_support;
mod gateway_support;

use std::sync::Arc;
use std::time::Duration;

use forge_net::test_support::RecordingRateLimiter;
use forge_net::{
    CallClass, FailureObserver, Gateway, GatewayBuildError, GatewayFailure, GatewayProfile,
    GatewayRequest, InvalidRequest, Method, Secret, ThrottleHintFamily, TransportKind,
};
use gateway_streaming_support::{
    StubServer, UNLIMITED, redirect_to, sized_body, stream, transport_kind,
};
use gateway_support::{RecordingObserver, platform, received};
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

const UNROUTABLE_HTTPS: &str = "https://192.0.2.1/release/asset.bin";
const UNROUTABLE_HTTP: &str = "http://192.0.2.1/release/asset.bin";

fn trusted() -> GatewayProfile {
    GatewayProfile::absolute_https(
        CallClass::Media,
        ThrottleHintFamily::RetryAfter,
        Duration::from_secs(60),
    )
}

struct BaseLess {
    gateway: Gateway,
    limiter: Arc<RecordingRateLimiter>,
    observer: Arc<RecordingObserver>,
}

fn base_less(profile: GatewayProfile) -> BaseLess {
    let limiter = Arc::new(RecordingRateLimiter::granting());
    let observer = Arc::new(RecordingObserver::default());
    let gateway = Gateway::without_base(
        limiter.clone(),
        profile,
        Some(observer.clone() as Arc<dyn FailureObserver>),
    )
    .unwrap();
    BaseLess {
        gateway,
        limiter,
        observer,
    }
}

async fn ok_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_string("asset"))
        .mount(&server)
        .await;
    server
}

#[tokio::test(start_paused = true)]
async fn trusted_absolute_sends_https_to_a_non_loopback_host() {
    let trusted = base_less(trusted());

    let streamed = stream(
        &trusted.gateway,
        GatewayRequest::absolute(Method::Get, UNROUTABLE_HTTPS),
        UNLIMITED,
    )
    .await;

    let failure = streamed.failure();
    assert!(
        matches!(failure, GatewayFailure::Transport(_)),
        "{failure:?}"
    );
    assert_eq!(trusted.limiter.acquire_count(), 1);
}

#[tokio::test]
async fn trusted_absolute_accepts_plain_http_only_on_a_loopback_host() {
    let server = ok_server().await;
    let port = server.address().port();
    for target in [
        format!("http://127.0.0.1:{port}/asset"),
        format!("http://localhost:{port}/asset"),
    ] {
        let trusted = base_less(trusted());
        let streamed = stream(
            &trusted.gateway,
            GatewayRequest::absolute(Method::Get, target.clone()),
            UNLIMITED,
        )
        .await;
        assert_eq!(streamed.sink.bytes(), b"asset", "{target}");
    }
}

#[tokio::test]
async fn trusted_absolute_refuses_plain_http_or_other_schemes_off_loopback_before_the_limiter() {
    for target in [UNROUTABLE_HTTP, "ftp://192.0.2.1/asset.bin"] {
        let trusted = base_less(trusted());
        let streamed = stream(
            &trusted.gateway,
            GatewayRequest::absolute(Method::Get, target),
            UNLIMITED,
        )
        .await;
        assert_eq!(
            streamed.failure(),
            GatewayFailure::InvalidRequest(InvalidRequest::UnsupportedScheme),
            "{target}"
        );
        assert_eq!(trusted.limiter.acquire_count(), 0, "{target}");
    }
}

#[derive(Debug, PartialEq, Eq)]
enum HopOutcome {
    Landed,
    Refused,
    FollowedOffHost,
}

#[tokio::test(start_paused = true)]
async fn trusted_absolute_redirects_follow_the_same_scheme_rule_as_the_first_hop() {
    let landing = StubServer::serving(sized_body(&[b"asset"])).await;
    for (location, expected) in [
        (UNROUTABLE_HTTP.to_owned(), HopOutcome::Refused),
        (UNROUTABLE_HTTPS.to_owned(), HopOutcome::FollowedOffHost),
        (format!("{}/asset", landing.uri()), HopOutcome::Landed),
    ] {
        let start = StubServer::serving(redirect_to(&location)).await;
        let trusted = base_less(trusted());

        let streamed = stream(
            &trusted.gateway,
            GatewayRequest::absolute(Method::Get, format!("{}/latest", start.uri())),
            UNLIMITED,
        )
        .await;

        let outcome = match &streamed.outcome {
            Ok(_) if streamed.sink.bytes() == b"asset" => HopOutcome::Landed,
            Ok(_) => panic!("{location}: unexpected body {:?}", streamed.sink.bytes()),
            Err(_) if transport_kind(&streamed.failure()) == TransportKind::Redirect => {
                HopOutcome::Refused
            }
            Err(_) => HopOutcome::FollowedOffHost,
        };
        assert_eq!(outcome, expected, "{location}");
    }
}

#[tokio::test]
async fn non_platform_profiles_refuse_any_credential_before_the_limiter_on_both_send_paths() {
    let server = ok_server().await;
    let target = format!("{}/asset", server.uri());
    for (profile, streaming) in [
        (trusted(), false),
        (trusted(), true),
        (GatewayProfile::user_directed(true), false),
        (GatewayProfile::user_directed(true), true),
    ] {
        for request in [
            GatewayRequest::absolute(Method::Get, target.clone()).bearer(Secret::new("t")),
            GatewayRequest::absolute(Method::Get, target.clone())
                .api_key("x-api-key", Secret::new("k")),
            GatewayRequest::absolute(Method::Get, target.clone())
                .authorization("token", Secret::new("t")),
        ] {
            let gateway = base_less(profile);
            let failure = if streaming {
                stream(&gateway.gateway, request, UNLIMITED).await.failure()
            } else {
                gateway.gateway.send(request).await.unwrap_err()
            };
            assert_eq!(
                failure,
                GatewayFailure::InvalidRequest(InvalidRequest::CredentialNotAllowed),
                "{profile:?} streaming={streaming}"
            );
            assert_eq!(gateway.limiter.acquire_count(), 0);
            assert_eq!(gateway.observer.seen().len(), 1);
        }
    }
    assert!(received(&server).await.is_empty());
}

#[test]
fn a_base_less_gateway_needs_a_profile_that_accepts_absolute_urls() {
    for (profile, allowed) in [
        (platform(), false),
        (GatewayProfile::user_directed(false), true),
        (GatewayProfile::user_directed(true), true),
        (trusted(), true),
    ] {
        let built =
            Gateway::without_base(Arc::new(RecordingRateLimiter::granting()), profile, None);
        match built {
            Ok(_) => assert!(allowed, "{profile:?}"),
            Err(error) => {
                assert!(!allowed, "{profile:?}");
                assert_eq!(error, GatewayBuildError::BaseRequired);
            }
        }
    }
}

#[tokio::test]
async fn a_base_less_gateway_refuses_path_targets_before_the_limiter() {
    for profile in [trusted(), GatewayProfile::user_directed(true)] {
        let gateway = base_less(profile);
        let buffered = gateway
            .gateway
            .send(GatewayRequest::get("asset"))
            .await
            .unwrap_err();
        let streamed = stream(&gateway.gateway, GatewayRequest::get("asset"), UNLIMITED)
            .await
            .failure();
        let expected = GatewayFailure::InvalidRequest(InvalidRequest::PathWithoutBase);
        assert_eq!((buffered, streamed), (expected.clone(), expected));
        assert_eq!(gateway.limiter.acquire_count(), 0);
    }
}

#[test]
fn resolve_path_on_a_base_less_gateway_is_path_without_base() {
    let gateway = base_less(trusted());
    assert_eq!(
        gateway.gateway.resolve_path("asset").unwrap_err(),
        GatewayFailure::InvalidRequest(InvalidRequest::PathWithoutBase)
    );
}

#[tokio::test]
async fn a_trusted_absolute_gateway_with_a_base_serves_path_and_absolute_targets() {
    let server = ok_server().await;
    let gateway = Gateway::new(
        &server.uri(),
        Arc::new(RecordingRateLimiter::granting()),
        trusted(),
        None,
    )
    .unwrap();
    for request in [
        GatewayRequest::get("by-path"),
        GatewayRequest::absolute(Method::Get, format!("{}/by-url", server.uri())),
    ] {
        let streamed = stream(&gateway, request, UNLIMITED).await;
        assert_eq!(streamed.sink.bytes(), b"asset");
    }
    let paths: Vec<String> = received(&server)
        .await
        .iter()
        .map(|request| request.url.path().to_owned())
        .collect();
    assert_eq!(paths, vec!["/by-path".to_owned(), "/by-url".to_owned()]);
}

#[tokio::test(start_paused = true)]
async fn user_directed_still_sends_plain_http_to_a_non_loopback_host() {
    let directed = base_less(GatewayProfile::user_directed(true));

    let failure = directed
        .gateway
        .send(GatewayRequest::absolute(Method::Post, UNROUTABLE_HTTP))
        .await
        .unwrap_err();

    assert!(
        matches!(failure, GatewayFailure::Transport(_)),
        "{failure:?}"
    );
    assert_eq!(directed.limiter.acquire_count(), 1);
}
