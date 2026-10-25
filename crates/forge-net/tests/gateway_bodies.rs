#![allow(clippy::unwrap_used)]
mod gateway_support;

use std::collections::BTreeMap;

use forge_net::{FormBody, GatewayRequest, MultipartBody, RequestBody, RequestEncodeError, Secret};
use gateway_support::{Harness, platform, received};
use serde_json::json;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn sent(request: GatewayRequest) -> wiremock::Request {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Harness::new(&server.uri(), platform()).ok(request).await;
    received(&server).await.remove(0)
}

fn content_types(request: &wiremock::Request) -> Vec<String> {
    request
        .headers
        .get_all("content-type")
        .iter()
        .map(|value| value.to_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn json_body_is_sent_with_the_json_content_type() {
    let request =
        sent(GatewayRequest::post("items").body(RequestBody::json(&json!({"name": "a"})).unwrap()))
            .await;
    assert_eq!(content_types(&request), vec!["application/json"]);
    assert_eq!(request.body, br#"{"name":"a"}"#);
}

#[tokio::test]
async fn explicit_content_type_replaces_the_body_default_instead_of_duplicating_it() {
    let request = sent(
        GatewayRequest::patch("items/1")
            .header("Content-Type", "application/merge-patch+json")
            .body(RequestBody::json(&json!({"name": null})).unwrap()),
    )
    .await;
    assert_eq!(
        content_types(&request),
        vec!["application/merge-patch+json"]
    );
}

#[test]
fn json_body_that_cannot_be_encoded_is_an_encode_error() {
    let unencodable: BTreeMap<(u8, u8), u8> = BTreeMap::from([((1, 2), 3)]);
    assert_eq!(
        RequestBody::json(&unencodable).unwrap_err(),
        RequestEncodeError
    );
}

#[tokio::test]
async fn raw_body_is_sent_verbatim_with_its_content_type() {
    let request = sent(GatewayRequest::post("ssml").body(RequestBody::raw(
        "application/ssml+xml",
        b"<speak/>".to_vec(),
    )))
    .await;
    assert_eq!(content_types(&request), vec!["application/ssml+xml"]);
    assert_eq!(request.body, b"<speak/>");
}

#[tokio::test]
async fn form_body_sends_plain_and_secret_fields_url_encoded() {
    let request = sent(
        GatewayRequest::post("oauth/token").body(
            FormBody::new()
                .field("grant_type", "refresh token")
                .secret_field("refresh_token", Secret::new("r&t=1")),
        ),
    )
    .await;
    assert_eq!(
        content_types(&request),
        vec!["application/x-www-form-urlencoded"]
    );
    assert_eq!(
        String::from_utf8(request.body).unwrap(),
        "grant_type=refresh+token&refresh_token=r%26t%3D1"
    );
}

#[tokio::test]
async fn multipart_body_reaches_the_server_with_text_and_file_parts() {
    let request = sent(
        GatewayRequest::post("webhooks/1/t").body(
            MultipartBody::new()
                .text("payload_json", r#"{"content":"hi"}"#)
                .file("files[0]", "clip.wav", "audio/wav", b"RIFFdata".to_vec()),
        ),
    )
    .await;
    let content_type = content_types(&request).remove(0);
    assert!(
        content_type.starts_with("multipart/form-data; boundary="),
        "{content_type}"
    );
    let body = String::from_utf8(request.body).unwrap();
    for expected in [
        r#"name="payload_json""#,
        r#"{"content":"hi"}"#,
        r#"name="files[0]"; filename="clip.wav""#,
        "content-type: audio/wav",
        "RIFFdata",
    ] {
        assert!(
            body.to_ascii_lowercase()
                .contains(&expected.to_ascii_lowercase()),
            "missing {expected:?} in {body}"
        );
    }
}

#[tokio::test]
async fn credentials_reach_the_server_in_their_headers() {
    for (request, name, value) in [
        (
            GatewayRequest::get("me").bearer(Secret::new("tok-1")),
            "authorization",
            "Bearer tok-1",
        ),
        (
            GatewayRequest::get("me").authorization("OAuth", Secret::new("tok-2")),
            "authorization",
            "OAuth tok-2",
        ),
        (
            GatewayRequest::get("me").api_key("Client-Id", Secret::new("cid-1")),
            "client-id",
            "cid-1",
        ),
    ] {
        let request = sent(request).await;
        assert_eq!(request.headers.get(name).unwrap().to_str().unwrap(), value);
    }
}
