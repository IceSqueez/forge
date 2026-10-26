mod support;
use forge_donatello::DonatelloError;
use std::sync::Arc;
use support::{ScriptedLimiter, TOKEN, provider_on};
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
    let provider = provider_on(
        &donatello.uri(),
        Arc::new(support::MemCreds::default()),
        ScriptedLimiter::open(),
    );
    assert!(provider.verify_token(TOKEN).await.is_err());
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

#[tokio::test]
async fn a_redirect_answer_is_an_http_error_and_is_never_followed() {
    for status in [301, 302, 307, 308] {
        let donatello = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/me"))
            .respond_with(ResponseTemplate::new(status).insert_header("Location", "/moved"))
            .mount(&donatello)
            .await;
        Mock::given(method("GET"))
            .and(path("/moved"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&donatello)
            .await;
        let provider = provider_on(
            &donatello.uri(),
            Arc::new(support::MemCreds::default()),
            ScriptedLimiter::open(),
        );

        let result = provider.verify_token(TOKEN).await;

        assert!(
            matches!(result, Err(DonatelloError::Http { status: got }) if got == status),
            "status {status}: {result:?}"
        );
        let paths: Vec<String> = donatello
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|request| request.url.path().to_owned())
            .collect();
        assert_eq!(paths, vec!["/me".to_owned()], "status {status}");
    }
}
