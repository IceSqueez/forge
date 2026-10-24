#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod gateway_support;

use std::sync::Arc;

use forge_net::test_support::RecordingRateLimiter;
use forge_net::{
    Gateway, GatewayBuildError, GatewayFailure, GatewayProfile, GatewayRequest, HostRejection,
    InvalidRequest, Method, MultipartBody, RequestBody, Secret,
};
use gateway_support::{Harness, platform, received};
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn ok_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    server
}

fn build(raw: &str, profile: GatewayProfile) -> Result<Gateway, GatewayBuildError> {
    Gateway::new(
        raw,
        Arc::new(RecordingRateLimiter::granting()),
        profile,
        None,
    )
}

#[test]
fn base_url_rules_reject_each_unsafe_shape_with_its_own_error() {
    let guarded = GatewayProfile::user_directed(false);
    for (raw, profile, expected) in [
        ("not a url", platform(), GatewayBuildError::MalformedBase),
        (
            "/relative/path",
            platform(),
            GatewayBuildError::MalformedBase,
        ),
        (
            "mailto:ops@example.com",
            platform(),
            GatewayBuildError::MalformedBase,
        ),
        (
            "http://api.example.com",
            platform(),
            GatewayBuildError::InsecureBase,
        ),
        (
            "http://192.0.2.10",
            platform(),
            GatewayBuildError::InsecureBase,
        ),
        (
            "ftp://127.0.0.1",
            platform(),
            GatewayBuildError::InsecureBase,
        ),
        (
            "ws://localhost",
            platform(),
            GatewayBuildError::InsecureBase,
        ),
        (
            "https://user:pw@api.example.com",
            platform(),
            GatewayBuildError::EmbeddedCredentials,
        ),
        (
            "https://user@api.example.com",
            platform(),
            GatewayBuildError::EmbeddedCredentials,
        ),
        (
            "https://api.example.com/v1?key=1",
            platform(),
            GatewayBuildError::QueryOrFragment,
        ),
        (
            "https://api.example.com/v1#top",
            platform(),
            GatewayBuildError::QueryOrFragment,
        ),
        (
            "https://10.0.0.1",
            guarded,
            GatewayBuildError::AddressRefused(HostRejection::PrivateOrSpecial),
        ),
        (
            "http://127.0.0.1:8080",
            guarded,
            GatewayBuildError::AddressRefused(HostRejection::PrivateOrSpecial),
        ),
        (
            "https://[fd00::1]",
            guarded,
            GatewayBuildError::AddressRefused(HostRejection::PrivateOrSpecial),
        ),
    ] {
        assert_eq!(build(raw, profile).unwrap_err(), expected, "base {raw:?}");
    }
}

#[test]
fn base_url_accepts_https_and_plain_http_only_on_loopback_hosts() {
    for (raw, profile) in [
        ("https://api.example.com/v1", platform()),
        ("http://127.0.0.1:9000", platform()),
        ("http://127.8.0.1", platform()),
        ("http://localhost:9000/api", platform()),
        ("http://LOCALHOST", platform()),
        ("http://[::1]:9000", platform()),
        ("https://10.0.0.1", platform()),
        ("https://10.0.0.1", GatewayProfile::user_directed(true)),
        (
            "https://api.example.com",
            GatewayProfile::user_directed(false),
        ),
    ] {
        assert!(
            build(raw, profile).is_ok(),
            "base {raw:?} should be accepted"
        );
    }
}

#[tokio::test]
async fn request_path_is_appended_to_the_base_path_whatever_the_slashes() {
    for (base_path, path, expected) in [
        ("/v1", "users", "/v1/users"),
        ("/v1", "/users", "/v1/users"),
        ("/v1/", "users", "/v1/users"),
        ("/v1/", "//users", "/v1/users"),
        ("", "users/42", "/users/42"),
        ("/api/v2", "webhooks/1/abc", "/api/v2/webhooks/1/abc"),
    ] {
        let server = ok_server().await;
        let harness = Harness::new(&format!("{}{base_path}", server.uri()), platform());
        harness.ok(GatewayRequest::get(path)).await;
        let seen = received(&server).await;
        assert_eq!(seen.len(), 1);
        assert_eq!(
            seen[0].url.path(),
            expected,
            "base {base_path:?} + {path:?}"
        );
    }
}

#[tokio::test]
async fn query_pairs_are_encoded_onto_the_request() {
    let server = ok_server().await;
    let harness = Harness::new(&server.uri(), platform());
    harness
        .ok(GatewayRequest::get("search")
            .query("q", "a b&c")
            .secret_query("key", Secret::new("k=1")))
        .await;
    let seen = received(&server).await;
    let pairs: Vec<(String, String)> = seen[0]
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("q".to_owned(), "a b&c".to_owned()),
            ("key".to_owned(), "k=1".to_owned())
        ]
    );
}

#[tokio::test]
async fn invalid_requests_fail_before_the_limiter_or_the_network() {
    let server = ok_server().await;
    let uri = server.uri();
    for (request, expected) in [
        (
            GatewayRequest::get("users?id=1"),
            InvalidRequest::QueryInPath,
        ),
        (
            GatewayRequest::get("users#frag"),
            InvalidRequest::QueryInPath,
        ),
        (
            GatewayRequest::absolute(Method::Get, format!("{uri}/users")),
            InvalidRequest::AbsoluteUrlNotAllowed,
        ),
        (
            GatewayRequest::get("users").header("bad name", "v"),
            InvalidRequest::InvalidHeader,
        ),
        (
            GatewayRequest::get("users").header("x-trace", "line\nbreak"),
            InvalidRequest::InvalidHeader,
        ),
        (
            GatewayRequest::get("users").api_key("bad header", Secret::new("k")),
            InvalidRequest::InvalidHeader,
        ),
        (
            GatewayRequest::get("users").bearer(Secret::new("tok\nen")),
            InvalidRequest::InvalidHeader,
        ),
        (
            GatewayRequest::post("users").body(RequestBody::raw("text/plain\nx", b"x".to_vec())),
            InvalidRequest::InvalidContentType,
        ),
        (
            GatewayRequest::post("users").body(MultipartBody::new().file(
                "file",
                "a.bin",
                "not a mime",
                b"x".to_vec(),
            )),
            InvalidRequest::InvalidContentType,
        ),
    ] {
        let harness = Harness::new(&uri, platform());
        let failure = harness.fail(request).await;
        assert_eq!(failure, GatewayFailure::InvalidRequest(expected));
        assert_eq!(harness.limiter.acquire_count(), 0, "{expected:?}");
    }
    assert!(received(&server).await.is_empty());
}

#[tokio::test]
async fn credential_headers_are_refused_as_plain_headers_in_any_case() {
    let server = ok_server().await;
    for name in [
        "Authorization",
        "authorization",
        "Proxy-Authorization",
        "proxy-authorization",
        "Cookie",
        "COOKIE",
    ] {
        let harness = Harness::new(&server.uri(), platform());
        let failure = harness
            .fail(GatewayRequest::get("users").header(name, "secret"))
            .await;
        assert_eq!(
            failure,
            GatewayFailure::InvalidRequest(InvalidRequest::SensitiveHeader),
            "{name}"
        );
    }
    assert!(received(&server).await.is_empty());
}

#[tokio::test]
async fn request_weight_reaches_the_limiter_and_defaults_to_one() {
    let server = ok_server().await;
    for (request, expected) in [
        (GatewayRequest::get("a"), vec![1]),
        (GatewayRequest::get("a").weight(7), vec![7]),
    ] {
        let harness = Harness::new(&server.uri(), platform());
        harness.ok(request).await;
        assert_eq!(harness.limiter.acquired_weights(), expected);
    }
}

#[tokio::test]
async fn exhausted_limiter_fails_with_budget_exhausted_and_sends_nothing() {
    let server = ok_server().await;
    for limiter in [
        RecordingRateLimiter::exhausted(),
        RecordingRateLimiter::failing(),
    ] {
        let harness = Harness::with_limiter(&server.uri(), platform(), limiter);
        let failure = harness.fail(GatewayRequest::get("a")).await;
        assert_eq!(failure, GatewayFailure::BudgetExhausted);
    }
    assert!(received(&server).await.is_empty());
}

#[tokio::test]
async fn user_directed_absolute_url_reaches_its_target_without_the_fragment() {
    let server = ok_server().await;
    let harness = Harness::new(&server.uri(), GatewayProfile::user_directed(true));
    let response = harness
        .ok(GatewayRequest::absolute(
            Method::Get,
            format!("{}/hook/path#section", server.uri()),
        ))
        .await;
    assert_eq!(response.status(), 200);
    let seen = received(&server).await;
    assert_eq!(seen[0].url.path(), "/hook/path");
}

#[tokio::test]
async fn user_directed_absolute_url_rejects_bad_scheme_userinfo_and_garbage() {
    let server = ok_server().await;
    let authority = server.address().to_string();
    for (raw, expected) in [
        (
            format!("ftp://{authority}/x"),
            InvalidRequest::UnsupportedScheme,
        ),
        (
            "file:///etc/passwd".to_owned(),
            InvalidRequest::MalformedUrl,
        ),
        (
            format!("http://user:pw@{authority}/x"),
            InvalidRequest::EmbeddedCredentials,
        ),
        (
            format!("http://user@{authority}/x"),
            InvalidRequest::EmbeddedCredentials,
        ),
        ("not a url".to_owned(), InvalidRequest::MalformedUrl),
    ] {
        let harness = Harness::new(&server.uri(), GatewayProfile::user_directed(true));
        let failure = harness
            .fail(GatewayRequest::absolute(Method::Get, raw.clone()))
            .await;
        assert_eq!(failure, GatewayFailure::InvalidRequest(expected), "{raw}");
        assert_eq!(harness.limiter.acquire_count(), 0);
    }
    assert!(received(&server).await.is_empty());
}

#[tokio::test]
async fn guarded_user_directed_refuses_private_literals_before_the_limiter() {
    for raw in [
        "http://10.0.0.1/x",
        "https://192.168.1.1/x",
        "http://[::1]:80/x",
    ] {
        let harness = Harness::new(
            "https://example.invalid",
            GatewayProfile::user_directed(false),
        );
        let failure = harness
            .fail(GatewayRequest::absolute(Method::Get, raw))
            .await;
        assert_eq!(
            failure,
            GatewayFailure::AddressRefused(HostRejection::PrivateOrSpecial),
            "{raw}"
        );
        assert_eq!(harness.limiter.acquire_count(), 0);
    }
}
