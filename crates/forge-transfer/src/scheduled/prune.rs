use std::path::{Path, PathBuf};

use time::{Duration, OffsetDateTime, UtcOffset};

use crate::manifest::BundleOrigin;
use crate::naming::{
    BUNDLE_EXTENSION, CIPHERTEXT_EXTENSION, PARTIAL_EXTENSION, SCHEDULED_PREFIX, ciphertext_path,
};
use crate::reader::{BundleSummary, read_bundle_summary};
use crate::scheduled::retention::{BackupFile, RetentionPolicy, expired};

const STALE_PARTIAL_AGE: Duration = Duration::days(1);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PruneReport {
    pub removed: u32,
    pub failed: u32,
}

struct Listing {
    bundles: Vec<PathBuf>,
    partials: Vec<PathBuf>,
}

pub(crate) async fn prune(
    folder: &Path,
    key_fingerprint: &str,
    policy: RetentionPolicy,
    now: OffsetDateTime,
    offset: UtcOffset,
) -> PruneReport {
    let Some(listing) = list_candidates(folder).await else {
        return PruneReport {
            removed: 0,
            failed: 1,
        };
    };
    let mut doomed = Vec::new();
    let mut kept_candidates = Vec::new();
    for bundle in listing.bundles {
        if let Some(created_at) = created_at_if_ours(&bundle, key_fingerprint).await {
            kept_candidates.push(BackupFile {
                path: bundle,
                created_at,
            });
        }
    }
    for partial in listing.partials {
        let ours = created_at_if_ours(&partial, key_fingerprint)
            .await
            .is_some();
        if ours || is_stale(&partial, now).await {
            doomed.push(partial);
        }
    }
    let mut report = PruneReport::default();
    for path in expired(&kept_candidates, policy, now, offset)
        .into_iter()
        .chain(doomed)
    {
        let sibling = ciphertext_sibling(&path);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => report.removed += 1,
            Err(_) => report.failed += 1,
        }
        if let Some(sibling) = sibling {
            let _ = tokio::fs::remove_file(sibling).await;
        }
    }
    report
}

fn ciphertext_sibling(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(&format!(".{PARTIAL_EXTENSION}"))?;
    Some(ciphertext_path(&path.with_file_name(stem)))
}

async fn list_candidates(folder: &Path) -> Option<Listing> {
    let mut entries = tokio::fs::read_dir(folder).await.ok()?;
    let mut listing = Listing {
        bundles: Vec::new(),
        partials: Vec::new(),
    };
    let bundle_suffix = format!(".{BUNDLE_EXTENSION}");
    let partial_suffix = format!("{bundle_suffix}.{PARTIAL_EXTENSION}");
    let ciphertext_suffix = format!("{bundle_suffix}.{CIPHERTEXT_EXTENSION}");
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(kind) = entry.file_type().await else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if !kind.is_file() || !name.starts_with(SCHEDULED_PREFIX) {
            continue;
        }
        if name.ends_with(&partial_suffix) || name.ends_with(&ciphertext_suffix) {
            listing.partials.push(entry.path());
        } else if name.ends_with(&bundle_suffix) {
            listing.bundles.push(entry.path());
        }
    }
    Some(listing)
}

async fn is_stale(path: &Path, now: OffsetDateTime) -> bool {
    tokio::fs::metadata(path)
        .await
        .and_then(|metadata| metadata.modified())
        .map(OffsetDateTime::from)
        .is_ok_and(|modified| now - modified >= STALE_PARTIAL_AGE)
}

async fn created_at_if_ours(path: &Path, key_fingerprint: &str) -> Option<OffsetDateTime> {
    match read_bundle_summary(path.to_path_buf()).await.ok()? {
        BundleSummary::Encrypted(manifest)
            if manifest.origin == BundleOrigin::Scheduled
                && manifest.key_fingerprint.as_deref() == Some(key_fingerprint) =>
        {
            Some(manifest.created_at)
        }
        _ => None,
    }
}
