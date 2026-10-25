mod support;

use std::sync::Arc;

use forge_monobank::MonobankError;
use support::{CLIENT_INFO, Limits, TOKEN, provider_on};
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
    let provider = provider_on(
        &bank.uri(),
        Arc::new(support::MemCreds::default()),
        Limits::open().shared(),
    );

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

#[tokio::test]
async fn a_redirect_answer_is_an_http_error_and_is_never_followed() {
    for status in [301, 302, 307, 308] {
        let bank = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(CLIENT_INFO))
            .respond_with(ResponseTemplate::new(status).insert_header("Location", "/moved"))
            .mount(&bank)
            .await;
        Mock::given(method("GET"))
            .and(path("/moved"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{\"jars\": []}"))
            .mount(&bank)
            .await;
        let provider = provider_on(
            &bank.uri(),
            Arc::new(support::MemCreds::default()),
            Limits::open().shared(),
        );

        let result = provider.verify_token(TOKEN).await;

        assert!(
            matches!(result, Err(MonobankError::Http { status: got }) if got == status),
            "status {status}: {result:?}"
        );
        let paths: Vec<String> = bank
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|request| request.url.path().to_owned())
            .collect();
        assert_eq!(paths, vec![CLIENT_INFO.to_owned()], "status {status}");
    }
}
