#![allow(clippy::unwrap_used)]
mod support;

use std::sync::Arc;

use async_trait::async_trait;
use forge_net::{RateLimitError, RateLimitOutcome, RateLimiter};
use forge_update::{
    ManifestSource, TrustedKeys, UpdateClient, UpdateError, check_for_update, parse_version,
};
use support::{Reply, SECRET, TestServer, client};

const TEST_KEY: &str = "RWS0+7RuIDjM/BDQuhL8FdT+3ic+WhaoX5wl1/vemvOb9gMZb5FU6dmr";
const MANIFEST: &[u8] = include_bytes!("fixtures/manifest.json");
const SIGNATURE: &[u8] = include_bytes!("fixtures/manifest.json.minisig");
const MANIFEST_LIMIT: u64 = 256 * 1024;
const SIGNATURE_LIMIT: u64 = 4 * 1024;

fn test_keys() -> TrustedKeys {
    TrustedKeys::from_base64(&[TEST_KEY]).unwrap()
}

async fn serve(manifest: Reply, signature: Reply) -> (TestServer, ManifestSource) {
    let server = TestServer::start(vec![
        ("/forge-update.json", manifest),
        ("/forge-update.json.minisig", signature),
    ])
    .await;
    let source = ManifestSource::new(
        server.url("/forge-update.json"),
        server.url("/forge-update.json.minisig"),
    );
    (server, source)
}

async fn check(source: &ManifestSource, running: &str) -> Result<Option<String>, UpdateError> {
    check_for_update(
        &client(),
        source,
        &test_keys(),
        &parse_version(running).unwrap(),
    )
    .await
    .map(|offer| offer.map(|offer| offer.version.to_string()))
}

#[tokio::test]
async fn a_signed_manifest_is_offered_only_when_newer_than_the_running_build() {
    let (_server, source) = serve(
        Reply::Full(MANIFEST.to_vec()),
        Reply::Full(SIGNATURE.to_vec()),
    )
    .await;
    for (running, offered) in [
        ("0.6.1", Some("0.6.2")),
        ("0.6.2-beta.3", Some("0.6.2")),
        ("0.6.2", None),
        ("0.6.3", None),
        ("1.0.0", None),
    ] {
        let result = check(&source, running).await.unwrap();

        assert_eq!(result.as_deref(), offered, "running {running}");
    }
}

#[tokio::test]
async fn a_manifest_signed_by_an_untrusted_key_is_never_offered() {
    let (_server, source) = serve(
        Reply::Full(MANIFEST.to_vec()),
        Reply::Full(include_bytes!("fixtures/manifest.other-key.minisig").to_vec()),
    )
    .await;

    let result = check(&source, "0.0.1").await;

    assert!(matches!(result, Err(UpdateError::UntrustedSignature)));
}

#[tokio::test]
async fn a_manifest_over_the_size_cap_is_refused_whether_declared_or_streamed() {
    let oversized = vec![b' '; MANIFEST_LIMIT as usize + 1];
    for manifest in [
        Reply::Full(oversized.clone()),
        Reply::CloseDelimited(oversized),
    ] {
        let (_server, source) = serve(manifest, Reply::Full(SIGNATURE.to_vec())).await;

        let result = check(&source, "0.0.1").await;

        assert!(
            matches!(result, Err(UpdateError::ResponseTooLarge { limit }) if limit == MANIFEST_LIMIT),
            "{result:?}",
        );
    }
}

#[tokio::test]
async fn a_manifest_exactly_at_the_size_cap_is_read_and_checked_against_its_signature() {
    let mut at_cap = MANIFEST.to_vec();
    at_cap.resize(MANIFEST_LIMIT as usize, b' ');
    for manifest in [Reply::Full(at_cap.clone()), Reply::CloseDelimited(at_cap)] {
        let (_server, source) = serve(manifest, Reply::Full(SIGNATURE.to_vec())).await;

        let result = check(&source, "0.0.1").await;

        assert!(
            matches!(result, Err(UpdateError::UntrustedSignature)),
            "{result:?}",
        );
    }
}

#[tokio::test]
async fn a_signature_is_read_at_its_size_cap_and_refused_one_byte_over() {
    for (length, refused) in [
        (SIGNATURE_LIMIT as usize, false),
        (SIGNATURE_LIMIT as usize + 1, true),
    ] {
        let (_server, source) = serve(
            Reply::Full(MANIFEST.to_vec()),
            Reply::CloseDelimited(vec![b'A'; length]),
        )
        .await;

        let result = check(&source, "0.0.1").await;

        if refused {
            assert!(
                matches!(result, Err(UpdateError::ResponseTooLarge { limit }) if limit == SIGNATURE_LIMIT),
                "{length}: {result:?}",
            );
        } else {
            assert!(
                matches!(result, Err(UpdateError::MalformedSignature)),
                "{length}: {result:?}",
            );
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_release_file_declaring_more_than_its_limit_is_refused_without_reading_the_body() {
    let declared_over = |limit: u64| Reply::Stall {
        content_length: limit + 1,
        prefix: Vec::new(),
    };
    for (label, manifest, signature, limit) in [
        (
            "manifest",
            declared_over(MANIFEST_LIMIT),
            Reply::Full(SIGNATURE.to_vec()),
            MANIFEST_LIMIT,
        ),
        (
            "signature",
            Reply::Full(MANIFEST.to_vec()),
            declared_over(SIGNATURE_LIMIT),
            SIGNATURE_LIMIT,
        ),
    ] {
        let (_server, source) = serve(manifest, signature).await;
        let started = tokio::time::Instant::now();

        let result = check(&source, "0.0.1").await;

        assert!(
            matches!(result, Err(UpdateError::ResponseTooLarge { limit: refused }) if refused == limit),
            "{label}: {result:?}",
        );
        assert_eq!(started.elapsed(), std::time::Duration::ZERO, "{label}");
    }
}

#[tokio::test]
async fn a_signature_that_is_not_utf8_is_malformed() {
    let (_server, source) = serve(
        Reply::Full(MANIFEST.to_vec()),
        Reply::Full(vec![0xff, 0xfe, 0xfd]),
    )
    .await;

    let result = check(&source, "0.0.1").await;

    assert!(matches!(result, Err(UpdateError::MalformedSignature)));
}

#[tokio::test]
async fn a_missing_release_asset_reports_its_status_without_the_url() {
    let (_server, source) = serve(Reply::Status(404), Reply::Full(SIGNATURE.to_vec())).await;

    let error = check(&source, "0.0.1").await.unwrap_err();

    assert!(matches!(error, UpdateError::Status(404)), "{error:?}");
    assert!(!error.to_string().contains(SECRET));
}

#[tokio::test]
async fn an_unreachable_update_server_reports_a_network_error_without_the_url() {
    let url = support::refused_url().await;
    let source = ManifestSource::new(url.clone(), url.clone());

    let error = check(&source, "0.0.1").await.unwrap_err();

    assert!(matches!(error, UpdateError::Network(_)), "{error:?}");
    let shown = error.to_string();
    assert!(
        !shown.contains(SECRET) && !shown.contains("127.0.0.1"),
        "{shown}"
    );
}

struct ExhaustedLimiter;

#[async_trait]
impl RateLimiter for ExhaustedLimiter {
    async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, RateLimitError> {
        Ok(RateLimitOutcome::Exhausted)
    }

    async fn observe_remote_throttle(&self, _retry_after: std::time::Duration) {}
}

#[tokio::test]
async fn an_exhausted_rate_limiter_stops_the_check_before_any_request() {
    let (server, source) = serve(
        Reply::Full(MANIFEST.to_vec()),
        Reply::Full(SIGNATURE.to_vec()),
    )
    .await;
    let limited = UpdateClient::new(Arc::new(ExhaustedLimiter)).unwrap();

    let result = check_for_update(
        &limited,
        &source,
        &test_keys(),
        &parse_version("0.0.1").unwrap(),
    )
    .await;

    assert!(matches!(result, Err(UpdateError::RateLimited)));
    assert_eq!(server.hits(), 0);
}
