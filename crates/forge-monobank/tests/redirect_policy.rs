#![allow(clippy::unwrap_used)]

mod support;

use std::sync::Arc;

use forge_monobank::{MonobankError, MonobankProvider};
use support::{CLIENT_INFO, Limits, TOKEN, config};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn token_is_not_forwarded_across_a_cross_origin_redirect() {
    let elsewhere = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"jars\": []}"))
        .mount(&elsewhere)
        .await;
    let bank = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(CLIENT_INFO))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", format!("{}/collect", elsewhere.uri())),
        )
        .mount(&bank)
        .await;
    let provider = MonobankProvider::new(
        config(&bank.uri()),
        Arc::new(support::MemCreds::default()),
        Limits::open().shared(),
    )
    .unwrap();

    let result = provider.verify_token(TOKEN).await;

    let leaked = elsewhere
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.headers.get("x-token").is_some())
        .count();
    assert_eq!(leaked, 0, "X-Token forwarded to another origin");
    assert!(
        matches!(result, Err(MonobankError::Http { status: 302 })),
        "{result:?}"
    );
}
