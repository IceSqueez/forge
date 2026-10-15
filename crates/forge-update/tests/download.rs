#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::path::Path;

use forge_update::{
    Artifact, CancelSignal, DownloadProgress, Sha256Digest, StagedArtifact, UpdateError,
    download_artifact, progress_channel,
};
use support::{Reply, SECRET, TestServer, client};

type ErrorCheck = fn(&UpdateError) -> bool;

const NAME: &str = "forge-0.6.2-linux-x64.AppImage";
const STAGED_NAME: &str = ".forge-update-forge-0.6.2-linux-x64.AppImage";

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
