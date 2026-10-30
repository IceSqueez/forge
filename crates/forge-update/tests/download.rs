#![allow(clippy::unwrap_used)]
mod support;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_net::{RateLimitError, RateLimitOutcome, RateLimiter, STREAMING_READ_IDLE_BUDGET};
use tokio::sync::Notify;

use forge_update::UpdateClient;
use forge_update::{
    Artifact, CancelSignal, DownloadProgress, Sha256Digest, StagedArtifact, UpdateError,
    download_artifact, progress_channel,
};
use support::{Reply, SECRET, TestServer, client};

type ErrorCheck = fn(&UpdateError) -> bool;

const HANG_GUARD: Duration = Duration::from_secs(30);
const TIMER_GRANULARITY: Duration = Duration::from_millis(1);
const NAME: &str = "forge-0.6.2-linux-x64.AppImage";
const STAGED_NAME: &str = ".forge-update-forge-0.6.2-linux-x64.AppImage";
const PARTIAL_NAME: &str = ".forge-update-forge-0.6.2-linux-x64.AppImage.part";
const OFF_LOOPBACK_HTTP: &str = "http://192.0.2.1/forge-0.6.2-linux-x64.AppImage";

fn payload() -> Vec<u8> {
    (0..64 * 1024).map(|index| (index % 251) as u8).collect()
}

fn artifact(url: String, body: &[u8]) -> Artifact {
    Artifact {
        platform: "linux-x64".to_owned(),
        kind: "appimage".to_owned(),
        name: NAME.to_owned(),
        url,
        sha256: Sha256Digest::of(body),
        size: body.len() as u64,
    }
}

fn staging_entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

async fn serve(reply: Reply) -> TestServer {
    TestServer::start(vec![("/artifact", reply)]).await
}

async fn download(artifact: &Artifact, dir: &Path) -> Result<StagedArtifact, UpdateError> {
    let (progress, _receiver) = progress_channel();
    download_artifact(&client(), artifact, dir, &progress, &CancelSignal::new()).await
}

#[tokio::test]
async fn a_matching_artifact_is_staged_under_its_staging_name() {
    let body = payload();
    let server = serve(Reply::Full(body.clone())).await;
    let staging = tempfile::tempdir().unwrap();

    let staged = download(&artifact(server.url("/artifact"), &body), staging.path())
        .await
        .unwrap();

    assert_eq!(staged.path, staging.path().join(STAGED_NAME));
    assert_eq!(std::fs::read(&staged.path).unwrap(), body);
    assert_eq!(staging_entries(staging.path()), [STAGED_NAME]);
}

#[tokio::test]
async fn progress_ends_at_the_full_size_of_the_artifact() {
    let body = payload();
    let server = serve(Reply::CloseDelimited(body.clone())).await;
    let staging = tempfile::tempdir().unwrap();
    let (progress, receiver) = progress_channel();

    download_artifact(
        &client(),
        &artifact(server.url("/artifact"), &body),
        staging.path(),
        &progress,
        &CancelSignal::new(),
    )
    .await
    .unwrap();

    let total = body.len() as u64;
    assert_eq!(
        receiver.latest(),
        DownloadProgress {
            received: total,
            total
        }
    );
}

#[tokio::test]
async fn every_rejected_download_leaves_nothing_in_the_staging_dir() {
    let body = payload();
    let size = body.len() as u64;
    let mut longer = body.clone();
    longer.extend_from_slice(b"extra");
    let mut corrupted = body.clone();
    corrupted[100] ^= 0xff;
    let short = body[..body.len() - 1].to_vec();

    let cases: Vec<(&str, Reply, ErrorCheck)> = vec![
        ("hash mismatch", Reply::Full(corrupted), |error| {
            matches!(error, UpdateError::ArtifactHashMismatch)
        }),
        (
            "content-length over the manifest size",
            Reply::Declared {
                content_length: size + 1,
                body: longer.clone(),
            },
            |error| matches!(error, UpdateError::ArtifactOversized { .. }),
        ),
        (
            "stream longer than declared",
            Reply::CloseDelimited(longer),
            |error| matches!(error, UpdateError::ArtifactOversized { .. }),
        ),
        ("short body", Reply::Full(short.clone()), |error| {
            matches!(
                error,
                UpdateError::ArtifactSizeMismatch { expected, actual }
                    if *actual + 1 == *expected
            )
        }),
        (
            "connection closed before the declared length",
            Reply::Declared {
                content_length: size,
                body: short,
            },
            |error| matches!(error, UpdateError::Network(_)),
        ),
        ("server error", Reply::Status(500), |error| {
            matches!(error, UpdateError::Status(500))
        }),
        ("throttled", Reply::Status(429), |error| {
            matches!(error, UpdateError::Status(429))
        }),
        ("forbidden", Reply::Status(403), |error| {
            matches!(error, UpdateError::Status(403))
        }),
        (
            "redirect onto plain http off loopback",
            Reply::Redirect(OFF_LOOPBACK_HTTP.to_owned()),
            |error| matches!(error, UpdateError::Network(_)),
        ),
    ];
    for (label, reply, expected) in cases {
        let server = serve(reply).await;
        let staging = tempfile::tempdir().unwrap();

        let error = download(&artifact(server.url("/artifact"), &body), staging.path())
            .await
            .unwrap_err();

        assert!(expected(&error), "{label}: {error:?}");
        assert!(
            staging_entries(staging.path()).is_empty(),
            "{label}: left {:?}",
            staging_entries(staging.path()),
        );
    }
}

#[tokio::test]
async fn download_errors_never_show_the_artifact_url() {
    let body = payload();
    let server = serve(Reply::Declared {
        content_length: body.len() as u64,
        body: body[..10].to_vec(),
    })
    .await;
    let staging = tempfile::tempdir().unwrap();
    let mut errors = Vec::new();
    for url in [server.url("/artifact"), support::refused_url().await] {
        errors.push(
            download(&artifact(url, &body), staging.path())
                .await
                .unwrap_err(),
        );
    }

    for error in errors {
        let shown = error.to_string();
        assert!(matches!(error, UpdateError::Network(_)), "{error:?}");
        assert!(
            !shown.contains(SECRET) && !shown.contains("127.0.0.1"),
            "{shown}"
        );
    }
}

#[tokio::test]
async fn cancelling_mid_stream_stops_the_download_and_removes_the_partial_file() {
    let body = payload();
    let server = serve(Reply::Stall {
        content_length: body.len() as u64,
        prefix: body[..4096].to_vec(),
    })
    .await;
    let staging = tempfile::tempdir().unwrap();
    let (progress, mut receiver) = progress_channel();
    let cancel = CancelSignal::new();
    let artifact = artifact(server.url("/artifact"), &body);

    let client = client();
    let download = download_artifact(&client, &artifact, staging.path(), &progress, &cancel);
    let canceller = async {
        while let Some(step) = receiver.changed().await {
            if step.received > 0 {
                assert!(
                    !staging_entries(staging.path()).is_empty(),
                    "no partial file while streaming",
                );
                cancel.cancel();
                return;
            }
        }
    };
    let (result, ()) = tokio::join!(download, canceller);

    assert!(matches!(result, Err(UpdateError::Cancelled)), "{result:?}");
    assert!(staging_entries(staging.path()).is_empty());
}

#[tokio::test]
async fn a_download_cancelled_before_it_starts_makes_no_request() {
    let body = payload();
    let server = serve(Reply::Full(body.clone())).await;
    let staging = tempfile::tempdir().unwrap();
    let (progress, _receiver) = progress_channel();
    let cancel = CancelSignal::new();
    cancel.cancel();

    let result = download_artifact(
        &client(),
        &artifact(server.url("/artifact"), &body),
        staging.path(),
        &progress,
        &cancel,
    )
    .await;

    assert!(matches!(result, Err(UpdateError::Cancelled)), "{result:?}");
    assert_eq!(server.hits(), 0);
    assert!(staging_entries(staging.path()).is_empty());
}

#[tokio::test]
async fn dropping_the_download_mid_stream_removes_the_partial_file() {
    let body = payload();
    let server = serve(Reply::Stall {
        content_length: body.len() as u64,
        prefix: body[..4096].to_vec(),
    })
    .await;
    let staging = tempfile::tempdir().unwrap();
    let (progress, mut receiver) = progress_channel();
    let cancel = CancelSignal::new();
    let artifact = artifact(server.url("/artifact"), &body);

    {
        let client = client();
        let download = download_artifact(&client, &artifact, staging.path(), &progress, &cancel);
        tokio::pin!(download);
        tokio::select! {
            result = &mut download => panic!("the stalled download finished: {result:?}"),
            step = async {
                loop {
                    match receiver.changed().await {
                        Some(step) if step.received > 0 => return step,
                        Some(_) => continue,
                        None => panic!("progress closed"),
                    }
                }
            } => assert!(step.received > 0),
        }
        assert!(!staging_entries(staging.path()).is_empty());
    }

    assert!(staging_entries(staging.path()).is_empty());
}

#[tokio::test]
async fn an_artifact_name_that_is_not_a_plain_file_name_is_refused_before_any_request() {
    let body = payload();
    let server = serve(Reply::Full(body.clone())).await;
    let outer = tempfile::tempdir().unwrap();
    let staging = outer.path().join("staging");
    std::fs::create_dir(&staging).unwrap();
    for name in ["../escape.AppImage", "", ".hidden", "dir/forge.AppImage"] {
        let mut artifact = artifact(server.url("/artifact"), &body);
        artifact.name = name.to_owned();

        let result = download(&artifact, &staging).await;

        assert!(
            matches!(result, Err(UpdateError::InvalidArtifactName)),
            "{name:?}: {result:?}",
        );
    }
    assert_eq!(server.hits(), 0);
    assert_eq!(staging_entries(outer.path()), ["staging"]);
}

#[tokio::test]
async fn a_download_whose_response_headers_never_arrive_fails_after_the_read_idle_budget() {
    let body = payload();
    let server = serve(Reply::Silent).await;
    let staging = tempfile::tempdir().unwrap();
    let silent_artifact = artifact(server.url("/artifact"), &body);
    let staging_dir = staging.path().to_owned();
    let started = tokio::time::Instant::now();
    let downloading = tokio::spawn(async move { download(&silent_artifact, &staging_dir).await });
    tokio::time::timeout(HANG_GUARD, async {
        while server.heads().is_empty() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    let request_arrived_after = started.elapsed();
    tokio::time::pause();

    let error = downloading.await.unwrap().unwrap_err();

    let waited = started.elapsed();
    assert!(matches!(error, UpdateError::Network(_)), "{error:?}");
    assert!(
        waited >= STREAMING_READ_IDLE_BUDGET
            && waited <= request_arrived_after + STREAMING_READ_IDLE_BUDGET + TIMER_GRANULARITY,
        "waited {waited:?}"
    );
    assert!(staging_entries(staging.path()).is_empty());
}

#[derive(Default)]
struct StuckLimiter {
    entered: Notify,
}

#[async_trait]
impl RateLimiter for StuckLimiter {
    async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, RateLimitError> {
        self.entered.notify_one();
        std::future::pending().await
    }

    async fn observe_remote_throttle(&self, _retry_after: Duration) {}
}

#[tokio::test]
async fn cancelling_a_download_waiting_on_the_rate_limiter_sends_nothing_and_removes_the_partial_file()
 {
    let body = payload();
    let server = serve(Reply::Full(body.clone())).await;
    let staging = tempfile::tempdir().unwrap();
    let (progress, _receiver) = progress_channel();
    let cancel = CancelSignal::new();
    let limiter = Arc::new(StuckLimiter::default());
    let client = UpdateClient::new(limiter.clone()).unwrap();
    let artifact = artifact(server.url("/artifact"), &body);

    let download = download_artifact(&client, &artifact, staging.path(), &progress, &cancel);
    let canceller = async {
        limiter.entered.notified().await;
        assert_eq!(staging_entries(staging.path()), [PARTIAL_NAME]);
        cancel.cancel();
    };
    let (result, ()) = tokio::join!(download, canceller);

    assert!(matches!(result, Err(UpdateError::Cancelled)), "{result:?}");
    assert_eq!(server.hits(), 0);
    assert!(staging_entries(staging.path()).is_empty());
}

#[tokio::test]
async fn a_release_redirect_chain_onto_a_cdn_host_stages_the_artifact_without_adding_headers() {
    let body = payload();
    let cdn = TestServer::start(vec![("/objects/artifact", Reply::Full(body.clone()))]).await;
    let release = TestServer::start(vec![
        (
            "/releases/latest/download/artifact",
            Reply::Redirect("/releases/download/v0.6.2/artifact".to_owned()),
        ),
        (
            "/releases/download/v0.6.2/artifact",
            Reply::Redirect(cdn.plain_url("localhost", "/objects/artifact")),
        ),
    ])
    .await;
    let staging = tempfile::tempdir().unwrap();

    let staged = download(
        &artifact(release.url("/releases/latest/download/artifact"), &body),
        staging.path(),
    )
    .await
    .unwrap();

    assert_eq!(std::fs::read(&staged.path).unwrap(), body);
    let first_hop_headers = header_names(&release.heads()[0]);
    let cdn_heads = cdn.heads();
    assert_eq!(cdn_heads.len(), 1);
    let extra: Vec<String> = header_names(&cdn_heads[0])
        .into_iter()
        .filter(|name| !first_hop_headers.contains(name))
        .collect();
    assert!(extra.is_empty(), "cdn hop gained {extra:?}");
}

fn header_names(head: &str) -> Vec<String> {
    head.lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, _)| name.trim().to_ascii_lowercase())
        .collect()
}
