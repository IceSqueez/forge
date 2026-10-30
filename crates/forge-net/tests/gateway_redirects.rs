#![allow(clippy::panic)]
mod gateway_support;

use forge_net::{GatewayFailure, GatewayRequest, RequestBody, TransportKind};

use forge_types::Secret;
use gateway_support::{Harness, platform, received};
use serde_json::json;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

const MAX_REDIRECT_HOPS: usize = 10;

async fn redirect(server: &MockServer, from: &str, status: u16, location: &str) {
    Mock::given(path(from))
        .respond_with(ResponseTemplate::new(status).insert_header("Location", location))
        .mount(server)
        .await;
}

async fn land(server: &MockServer, at: &str) {
    Mock::given(path(at))
        .respond_with(ResponseTemplate::new(200).set_body_string("landed"))
        .mount(server)
        .await;
}

async fn request_to(server: &MockServer, at: &str) -> wiremock::Request {
    received(server)
        .await
        .into_iter()
        .find(|request| request.url.path() == at)
        .unwrap_or_else(|| panic!("no request reached {at}"))
}

fn transport_kind(failure: &GatewayFailure) -> TransportKind {
    match failure {
        GatewayFailure::Transport(transport) => transport.kind(),
        other => panic!("expected a transport failure, got {other:?}"),
    }
}

fn header<'a>(request: &'a wiremock::Request, name: &str) -> Option<&'a str> {
    request
        .headers
        .get(name)
        .and_then(|value| value.to_str().ok())
}

#[tokio::test]
async fn see_other_family_redirects_become_a_bodyless_get() {
    for status in [301, 302, 303] {
        let server = MockServer::start().await;
        redirect(&server, "/start", status, "/landing").await;
        land(&server, "/landing").await;
        let harness = Harness::new(&server.uri(), platform());
        let response = harness
            .ok(GatewayRequest::post("start").body(RequestBody::json(&json!({"a": 1})).unwrap()))
            .await;
        assert_eq!(response.body(), b"landed");
        let landing = request_to(&server, "/landing").await;
        assert_eq!(landing.method.as_str(), "GET", "status {status}");
        assert!(landing.body.is_empty(), "status {status}");
    }
}

#[tokio::test]
async fn temporary_and_permanent_redirects_keep_the_method_and_body() {
    for status in [307, 308] {
        let server = MockServer::start().await;
        redirect(&server, "/start", status, "/landing").await;
        land(&server, "/landing").await;
        let harness = Harness::new(&server.uri(), platform());
        harness
            .ok(GatewayRequest::put("start").body(RequestBody::json(&json!({"a": 1})).unwrap()))
            .await;
        let landing = request_to(&server, "/landing").await;
        assert_eq!(landing.method.as_str(), "PUT", "status {status}");
        assert_eq!(landing.body, br#"{"a":1}"#, "status {status}");
    }
}

#[tokio::test]
async fn ten_redirect_hops_are_followed_and_the_eleventh_is_refused() {
    for (hops, refused) in [(MAX_REDIRECT_HOPS, false), (MAX_REDIRECT_HOPS + 1, true)] {
        let server = MockServer::start().await;
        for hop in 0..hops {
            redirect(&server, &format!("/h{hop}"), 302, &format!("/h{}", hop + 1)).await;
        }
        land(&server, &format!("/h{hops}")).await;
        let harness = Harness::new(&server.uri(), platform());
        let outcome = harness.gateway.send(GatewayRequest::get("h0")).await;
        if refused {
            assert_eq!(
                transport_kind(&outcome.unwrap_err()),
                TransportKind::Redirect
            );
        } else {
            assert_eq!(outcome.unwrap().body(), b"landed");
        }
    }
}

#[tokio::test]
async fn query_pairs_are_sent_on_the_first_hop_only() {
    let server = MockServer::start().await;
    redirect(&server, "/start", 302, "/next").await;
    land(&server, "/next").await;
    let harness = Harness::new(&server.uri(), platform());
    harness
        .ok(GatewayRequest::get("start")
            .query("page", "2")
            .secret_query("token", Secret::new("q-secret")))
        .await;
    assert_eq!(
        request_to(&server, "/start").await.url.query(),
        Some("page=2&token=q-secret")
    );
    assert_eq!(request_to(&server, "/next").await.url.query(), None);
}

#[tokio::test]
async fn same_origin_hop_keeps_request_headers_and_the_credential() {
    let server = MockServer::start().await;
    redirect(&server, "/start", 302, "/next").await;
    land(&server, "/next").await;
    let harness = Harness::new(&server.uri(), platform());
    harness
        .ok(GatewayRequest::get("start")
            .header("x-trace", "trace-1")
            .bearer(Secret::new("same-origin-token")))
        .await;
    let next = request_to(&server, "/next").await;
    assert_eq!(header(&next, "x-trace"), Some("trace-1"));
    assert_eq!(
        header(&next, "authorization"),
        Some("Bearer same-origin-token")
    );
}

#[tokio::test]
async fn cross_origin_hop_strips_headers_and_credential_for_the_rest_of_the_chain() {
    for request in [
        GatewayRequest::get("start")
            .header("x-trace", "trace-1")
            .bearer(Secret::new("bearer-token")),
        GatewayRequest::get("start")
            .header("x-trace", "trace-1")
            .api_key("x-api-key", Secret::new("api-key-value")),
    ] {
        let origin = MockServer::start().await;
        let other = MockServer::start().await;
        redirect(&origin, "/start", 302, &format!("{}/hop", other.uri())).await;
        redirect(&other, "/hop", 307, &format!("{}/final", origin.uri())).await;
        land(&origin, "/final").await;
        let harness = Harness::new(&origin.uri(), platform());
        harness.ok(request).await;
        let start = request_to(&origin, "/start").await;
        assert!(header(&start, "x-trace").is_some());
        for hop in [
            request_to(&other, "/hop").await,
            request_to(&origin, "/final").await,
        ] {
            for name in ["x-trace", "authorization", "x-api-key"] {
                assert_eq!(header(&hop, name), None, "{name} on {}", hop.url.path());
            }
        }
    }
}

#[tokio::test]
async fn platform_redirect_off_https_or_onto_userinfo_is_refused_without_following() {
    for location in [
        "http://192.0.2.1/plain-http".to_owned(),
        "ftp://192.0.2.1/file".to_owned(),
        "https://user:pw@192.0.2.1/".to_owned(),
        "http://[::1".to_owned(),
    ] {
        let server = MockServer::start().await;
        redirect(&server, "/start", 302, &location).await;
        let harness = Harness::new(&server.uri(), platform());
        let failure = harness.fail(GatewayRequest::get("start")).await;
        assert_eq!(
            transport_kind(&failure),
            TransportKind::Redirect,
            "{location}"
        );
        assert_eq!(received(&server).await.len(), 1);
    }
}

#[tokio::test]
async fn a_redirect_is_returned_unfollowed_as_an_unexpected_status_when_following_is_off() {
    for status in [301, 302, 303, 307, 308] {
        let server = MockServer::start().await;
        redirect(&server, "/start", status, "/landing").await;
        land(&server, "/landing").await;
        let harness = Harness::new(&server.uri(), platform());

        let failure = harness
            .fail(GatewayRequest::get("start").follow_redirects(false))
            .await;

        assert!(
            matches!(failure, GatewayFailure::UnexpectedStatus { status: got, .. } if got == status),
            "status {status}: {failure:?}"
        );
        let paths: Vec<String> = received(&server)
            .await
            .iter()
            .map(|request| request.url.path().to_owned())
            .collect();
        assert_eq!(paths, vec!["/start".to_owned()], "status {status}");
    }
}
