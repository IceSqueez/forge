use std::path::{Path, PathBuf};

use time::{Duration, OffsetDateTime, UtcOffset};

use crate::bundle::manifest::BundleOrigin;
use crate::bundle::naming::{
    BUNDLE_EXTENSION, CIPHERTEXT_EXTENSION, PARTIAL_EXTENSION, SCHEDULED_PREFIX, ciphertext_path,
};
use crate::bundle::reader::{BundleSummary, read_bundle_summary};
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
        if let Some(sibling) = sibling
            && let Err(error) = tokio::fs::remove_file(sibling).await
        {
            tracing::debug!(%error, "backup sibling file could not be removed");
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use flate2::Compression;
    use flate2::write::GzEncoder;
    use time::{Date, Duration, Month, OffsetDateTime, UtcOffset};

    use super::{PruneReport, prune};
    use crate::bundle::category::BackupCategory;
    use crate::bundle::manifest::{
        BUNDLE_FORMAT_VERSION, BundleKind, BundleOrigin, EncryptionScheme, MANIFEST_ENTRY_NAME,
        PublicManifest,
    };
    use crate::scheduled::retention::RetentionPolicy;

    const OUR_KEY: &str = "our-key-fingerprint";
    const OTHER_KEY: &str = "other-installation-fingerprint";
    const KEEP_ONE: RetentionPolicy = RetentionPolicy {
        keep_newest: 1,
        keep_daily_days: 0,
    };

    fn noon() -> OffsetDateTime {
        Date::from_calendar_date(2026, Month::October, 21)
            .unwrap()
            .with_hms(12, 0, 0)
            .unwrap()
            .assume_utc()
    }

    fn manifest(origin: BundleOrigin, key: &str, created_at: OffsetDateTime) -> PublicManifest {
        PublicManifest {
            format_version: BUNDLE_FORMAT_VERSION,
            kind: BundleKind::Full,
            encryption: EncryptionScheme::age_x25519_wrapped(),
            forge_version: "0.0.0".to_owned(),
            schema_version: 1,
            created_at,
            origin,
            categories: BackupCategory::all(),
            key_fingerprint: Some(key.to_owned()),
            wrapped_identity: Some("wrapped".to_owned()),
        }
    }

    fn plant(path: &Path, manifest: &PublicManifest) {
        let bytes = serde_json::to_vec(manifest).unwrap();
        let file = std::fs::File::create(path).unwrap();
        let mut builder = tar::Builder::new(GzEncoder::new(file, Compression::none()));
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        builder
            .append_data(&mut header, MANIFEST_ENTRY_NAME, bytes.as_slice())
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap();
    }

    fn ours(folder: &Path, name: &str, age: Duration) -> std::path::PathBuf {
        let path = folder.join(name);
        plant(
            &path,
            &manifest(BundleOrigin::Scheduled, OUR_KEY, noon() - age),
        );
        path
    }

    fn entries(folder: &Path) -> BTreeSet<String> {
        std::fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }

    async fn prune_keeping_one(folder: &Path) -> PruneReport {
        prune(folder, OUR_KEY, KEEP_ONE, noon(), UtcOffset::UTC).await
    }

    #[tokio::test]
    async fn prune_removes_this_keys_expired_scheduled_bundles() {
        let folder = tempfile::tempdir().unwrap();
        ours(
            folder.path(),
            "forge-scheduled-a.forgebundle",
            Duration::hours(1),
        );
        ours(
            folder.path(),
            "forge-scheduled-b.forgebundle",
            Duration::hours(2),
        );
        ours(
            folder.path(),
            "forge-scheduled-c.forgebundle",
            Duration::hours(3),
        );

        let report = prune_keeping_one(folder.path()).await;

        assert_eq!(
            (report, entries(folder.path())),
            (
                PruneReport {
                    removed: 2,
                    failed: 0
                },
                BTreeSet::from(["forge-scheduled-a.forgebundle".to_owned()])
            )
        );
    }

    #[tokio::test]
    async fn prune_leaves_every_file_that_is_not_provably_this_keys_scheduled_bundle() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        let old = noon() - Duration::days(30);
        ours(
            root,
            "forge-scheduled-newest.forgebundle",
            Duration::minutes(1),
        );
        plant(
            &root.join("forge-scheduled-foreign.forgebundle"),
            &manifest(BundleOrigin::Scheduled, OTHER_KEY, old),
        );
        plant(
            &root.join("forge-scheduled-manual.forgebundle"),
            &manifest(BundleOrigin::Manual, OUR_KEY, old),
        );
        plant(
            &root.join("forge-backup-20260101-000000Z-abcdef.forgebundle"),
            &manifest(BundleOrigin::Scheduled, OUR_KEY, old),
        );
        std::fs::write(root.join("forge-scheduled-notes.txt"), b"user notes").unwrap();
        std::fs::write(
            root.join("forge-scheduled-garbage.forgebundle"),
            b"not a bundle",
        )
        .unwrap();
        std::fs::create_dir(root.join("forge-scheduled-dir.forgebundle")).unwrap();
        let survivors = entries(root);

        prune_keeping_one(root).await;

        assert_eq!(entries(root), survivors);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn prune_never_follows_a_symlink_to_an_expired_bundle() {
        let folder = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        ours(
            folder.path(),
            "forge-scheduled-newest.forgebundle",
            Duration::minutes(1),
        );
        let target = ours(
            elsewhere.path(),
            "forge-scheduled-old.forgebundle",
            Duration::days(30),
        );
        let link = folder.path().join("forge-scheduled-link.forgebundle");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        prune_keeping_one(folder.path()).await;

        assert_eq!(
            (link.symlink_metadata().is_ok(), target.exists()),
            (true, true)
        );
    }

    #[tokio::test]
    async fn prune_sweeps_this_keys_partial_and_its_ciphertext_sibling() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        ours(
            root,
            "forge-scheduled-kept.forgebundle",
            Duration::minutes(1),
        );
        ours(
            root,
            "forge-scheduled-crashed.forgebundle.partial",
            Duration::minutes(5),
        );
        std::fs::write(
            root.join("forge-scheduled-crashed.forgebundle.payload.partial"),
            b"ciphertext",
        )
        .unwrap();

        prune_keeping_one(root).await;

        assert_eq!(
            entries(root),
            BTreeSet::from(["forge-scheduled-kept.forgebundle".to_owned()])
        );
    }

    #[tokio::test]
    async fn prune_keeps_a_partial_it_cannot_prove_is_its_own() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        plant(
            &root.join("forge-scheduled-foreign.forgebundle.partial"),
            &manifest(BundleOrigin::Scheduled, OTHER_KEY, noon()),
        );
        std::fs::write(
            root.join("forge-scheduled-torn.forgebundle.partial"),
            b"truncated",
        )
        .unwrap();
        let survivors = entries(root);

        prune_keeping_one(root).await;

        assert_eq!(entries(root), survivors);
    }

    #[tokio::test]
    async fn prune_of_a_missing_folder_reports_one_failure() {
        let folder = tempfile::tempdir().unwrap();

        let report = prune_keeping_one(&folder.path().join("gone")).await;

        assert_eq!(
            report,
            PruneReport {
                removed: 0,
                failed: 1
            }
        );
    }

    fn aged(path: &Path, age: Duration) {
        let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        let modified = std::time::SystemTime::from(noon() - age);
        file.set_modified(modified).unwrap();
    }

    #[tokio::test]
    async fn prune_sweeps_scheduled_leftovers_older_than_a_day_and_keeps_younger_or_manual_ones() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        for (name, age) in [
            (
                "forge-scheduled-torn.forgebundle.partial",
                Duration::days(2),
            ),
            (
                "forge-scheduled-lone.forgebundle.payload.partial",
                Duration::days(2),
            ),
            (
                "forge-scheduled-young.forgebundle.partial",
                Duration::hours(23),
            ),
            ("forge-backup-manual.forgebundle.partial", Duration::days(2)),
        ] {
            let path = root.join(name);
            std::fs::write(&path, b"truncated").unwrap();
            aged(&path, age);
        }

        prune_keeping_one(root).await;

        assert_eq!(
            entries(root),
            BTreeSet::from([
                "forge-backup-manual.forgebundle.partial".to_owned(),
                "forge-scheduled-young.forgebundle.partial".to_owned(),
            ])
        );
    }
}
