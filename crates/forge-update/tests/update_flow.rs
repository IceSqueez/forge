#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use forge_update::{
    ArtifactKind, CancelSignal, ManifestSource, Platform, Sha256Digest, TrustedKeys,
    check_for_update, download_artifact, parse_version, progress_channel,
};
use support::{Reply, TestServer, client};

const SERVING_KEY: &str = "RWTMXcVJnjbcubZpF9O5lOFlL11f06T1GDt/EaBQbWom3bcAByIxd5OO";
const MANIFEST: &[u8] = include_bytes!("fixtures/manifest-served-payload.json");
const SIGNATURE: &[u8] = include_bytes!("fixtures/manifest-served-payload.json.minisig");

fn payload() -> Vec<u8> {
    (0..64 * 1024).map(|index| (index % 251) as u8).collect()
}

#[tokio::test]
async fn a_signed_offer_is_downloaded_digest_checked_and_staged_through_one_client() {
    let body = payload();
    let server = TestServer::start(vec![
        ("/forge-update.json", Reply::Full(MANIFEST.to_vec())),
        (
            "/forge-update.json.minisig",
            Reply::Full(SIGNATURE.to_vec()),
        ),
        ("/artifact", Reply::Full(body.clone())),
    ])
    .await;
    let source = ManifestSource::new(
        server.url("/forge-update.json"),
        server.url("/forge-update.json.minisig"),
    );
    let client = client();
    let keys = TrustedKeys::from_base64(&[SERVING_KEY]).unwrap();

    let offer = check_for_update(&client, &source, &keys, &parse_version("0.6.1").unwrap())
        .await
        .unwrap()
        .expect("0.6.2 is offered to 0.6.1");
    let mut artifact = offer
        .artifact_for(Platform::LinuxX64, ArtifactKind::AppImage)
        .expect("signed linux appimage")
        .clone();
    artifact.url = server.url("/artifact");
    let staging = tempfile::tempdir().unwrap();
    let (progress, _receiver) = progress_channel();
    let staged = download_artifact(
        &client,
        &artifact,
        staging.path(),
        &progress,
        &CancelSignal::new(),
    )
    .await
    .unwrap();

    let written = std::fs::read(&staged.path).unwrap();
    assert_eq!(Sha256Digest::of(&written), artifact.sha256);
    assert_eq!(staged.size, artifact.size);
    assert_eq!(written, body);
}
