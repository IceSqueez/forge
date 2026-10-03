#![allow(clippy::unwrap_used)]
mod support;
use forge_donatello::DonatelloProvider;
use std::sync::Arc;
use support::{ScriptedLimiter, TOKEN, config};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn token_is_not_forwarded_across_a_cross_origin_redirect() {
    let elsewhere = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .mount(&elsewhere)
        .await;
    let donatello = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", format!("{}/collect", elsewhere.uri())),
        )
        .mount(&donatello)
        .await;
    let provider = DonatelloProvider::new(
        config(&donatello.uri()),
        Arc::new(support::MemCreds::default()),
        ScriptedLimiter::open(),
    )
    .unwrap();
    let _ = provider.verify_token(TOKEN).await;
    let leaked = elsewhere
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| {
            r.headers
                .get("x-token")
                .is_some_and(|v| v.as_bytes() == TOKEN.as_bytes())
        })
        .count();
    assert_eq!(leaked, 0, "X-Token forwarded to another origin");
}
