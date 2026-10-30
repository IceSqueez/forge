use std::sync::Arc;
use std::time::Duration;

use crate::update_report::ROLLED_BACK_VERSION;
use forge_storage::{DataProvider, SettingsRepo, read_setting};
use forge_update::{
    GithubRepo, InstallKind, ManualReason, Platform, PublishedRelease, TrustedKeys, UpdateClient,
    UpdateOffer, check_for_update, detect_install, fetch_latest_release, is_newer, parse_version,
};
use gpui::{AsyncApp, Entity, Task};
use time::OffsetDateTime;
use tokio::runtime::Handle;

use crate::update_flow::{InstallTarget, UpdateCandidate, UpdateFlow};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPO_URL: &str = env!("CARGO_PKG_REPOSITORY");

const BOOT_SETTLE_DELAY: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatePlan {
    Download,
    Offer,
}

pub fn plan_update(
    current: &str,
    latest: &str,
    installable: bool,
    rolled_back: Option<&str>,
    reported_this_launch: Option<&str>,
) -> Option<UpdatePlan> {
    let current = parse_version(current)?;
    let latest = parse_version(latest)?;
    if !is_newer(&current, &latest) {
        return None;
    }
    let covers_latest = |version: Option<&str>| {
        version
            .and_then(parse_version)
            .is_some_and(|seen| seen >= latest)
    };
    if covers_latest(reported_this_launch) {
        return None;
    }
    Some(if installable && !covers_latest(rolled_back) {
        UpdatePlan::Download
    } else {
        UpdatePlan::Offer
    })
}

pub fn display_version(raw: &str) -> String {
    let trimmed = raw.trim();
    trimmed.strip_prefix('v').unwrap_or(trimmed).to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FoundUpdate {
    Signed(UpdateOffer),
    Unsigned(PublishedRelease),
}

impl FoundUpdate {
    pub fn version(&self) -> String {
        match self {
            FoundUpdate::Signed(offer) => offer.version.to_string(),
            FoundUpdate::Unsigned(release) => display_version(&release.tag),
        }
    }
}

pub fn update_candidate(
    found: FoundUpdate,
    install: &InstallKind,
    platform: Option<Platform>,
) -> UpdateCandidate {
    match found {
        FoundUpdate::Unsigned(release) => UpdateCandidate {
            version: display_version(&release.tag),
            release_page: release.page_url,
            install: None,
        },
        FoundUpdate::Signed(offer) => {
            let install = match (install, platform) {
                (InstallKind::Updatable(install), Some(platform)) => offer
                    .artifact_for(platform, install.artifact_kind())
                    .map(|artifact| InstallTarget {
                        install: install.clone(),
                        artifact: artifact.clone(),
                    }),
                _ => None,
            };
            UpdateCandidate {
                version: offer.version.to_string(),
                release_page: offer.release_page,
                install,
            }
        }
    }
}

enum SignedCheck {
    Offer(UpdateOffer),
    UpToDate,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    Found(FoundUpdate),
    UpToDate,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckResult {
    NothingNew { checked_at: OffsetDateTime },
    Found { version: String, plan: UpdatePlan },
    Failed,
}

struct Probe {
    rolled_back: Option<String>,
    rollback_unknown: bool,
    outcome: CheckOutcome,
    install: InstallKind,
    checked_at: OffsetDateTime,
}

pub fn start_update_check(
    cx: &mut AsyncApp,
    backend: Arc<dyn DataProvider>,
    rt_handle: Handle,
    boot_report: Task<Option<String>>,
    flow: Entity<UpdateFlow>,
) {
    cx.spawn(async move |cx| {
        let reported = boot_report.await;
        cx.background_executor().timer(BOOT_SETTLE_DELAY).await;
        run_update_check(cx, backend, &rt_handle, reported, &flow).await;
    })
    .detach();
}

pub async fn run_update_check(
    cx: &mut AsyncApp,
    backend: Arc<dyn DataProvider>,
    rt_handle: &Handle,
    reported_this_launch: Option<String>,
    flow: &Entity<UpdateFlow>,
) -> CheckResult {
    let (tx, rx) = tokio::sync::oneshot::channel();
    rt_handle.spawn(async move {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the view may close before the result arrives"
        )]
        let _ = tx.send(probe(backend).await);
    });
    let Ok(probe) = rx.await else {
        return CheckResult::Failed;
    };
    let found = match probe.outcome {
        CheckOutcome::Found(found) => found,
        CheckOutcome::UpToDate => {
            return CheckResult::NothingNew {
                checked_at: probe.checked_at,
            };
        }
        CheckOutcome::Failed => return CheckResult::Failed,
    };

    let version = found.version();
    let mut candidate = update_candidate(found, &probe.install, Platform::running());
    let Some(plan) = plan_update(
        CURRENT_VERSION,
        &version,
        candidate.install.is_some() && !probe.rollback_unknown,
        probe.rolled_back.as_deref(),
        reported_this_launch.as_deref(),
    ) else {
        return CheckResult::NothingNew {
            checked_at: probe.checked_at,
        };
    };

    match plan {
        UpdatePlan::Download => {
            flow.update(cx, |flow, cx| flow.download(candidate, cx));
        }
        UpdatePlan::Offer => {
            candidate.install = None;
            flow.update(cx, |flow, cx| flow.offer(candidate, cx));
        }
    }
    CheckResult::Found { version, plan }
}

async fn probe(backend: Arc<dyn DataProvider>) -> Probe {
    let repo = backend as Arc<dyn SettingsRepo>;
    let stored_rollback = read_setting(repo.as_ref(), &ROLLED_BACK_VERSION).await;
    let rollback_unknown = stored_rollback.is_err();
    if let Err(error) = &stored_rollback {
        tracing::warn!(error = %error, "rolled back version unreadable; offering the update");
    }
    let rolled_back = stored_rollback.ok().flatten();
    let outcome = find_update().await;
    let install = tokio::task::spawn_blocking(detect_install)
        .await
        .unwrap_or(InstallKind::ManualOnly(ManualReason::UnknownExecutable));
    Probe {
        rolled_back,
        rollback_unknown,
        outcome,
        install,
        checked_at: OffsetDateTime::now_utc(),
    }
}

async fn find_update() -> CheckOutcome {
    let Some(repo) = GithubRepo::parse(REPO_URL) else {
        return CheckOutcome::Failed;
    };
    let client = match UpdateClient::for_github() {
        Ok(client) => client,
        Err(error) => {
            tracing::debug!(error = %error, "update check could not start");
            return CheckOutcome::Failed;
        }
    };
    match check_signed(&client, &repo).await {
        SignedCheck::Offer(offer) => CheckOutcome::Found(FoundUpdate::Signed(offer)),
        SignedCheck::UpToDate => CheckOutcome::UpToDate,
        SignedCheck::Unavailable => latest_release(&client, &repo).await,
    }
}

async fn check_signed(client: &UpdateClient, repo: &GithubRepo) -> SignedCheck {
    let Some(running) = parse_version(CURRENT_VERSION) else {
        return SignedCheck::Unavailable;
    };
    let keys = match TrustedKeys::production() {
        Ok(keys) => keys,
        Err(error) => {
            tracing::debug!(error = %error, "signed update check could not start");
            return SignedCheck::Unavailable;
        }
    };
    let source = repo.latest_manifest_source();
    match check_for_update(client, &source, &keys, &running).await {
        Ok(Some(offer)) => SignedCheck::Offer(offer),
        Ok(None) => SignedCheck::UpToDate,
        Err(error) => {
            tracing::debug!(error = %error, "signed update manifest unavailable; falling back");
            SignedCheck::Unavailable
        }
    }
}

async fn latest_release(client: &UpdateClient, repo: &GithubRepo) -> CheckOutcome {
    match fetch_latest_release(client, repo).await {
        Ok(Some(release)) => CheckOutcome::Found(FoundUpdate::Unsigned(release)),
        Ok(None) => CheckOutcome::UpToDate,
        Err(error) => {
            tracing::debug!(error = %error, "latest release lookup failed");
            CheckOutcome::Failed
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use forge_update::{
        Artifact, ArtifactKind, GithubRepo, InstallKind, InstallLayout, ManualReason, Platform,
        PublishedRelease, Sha256Digest, UpdatableInstall, UpdateOffer,
    };

    use super::{
        CURRENT_VERSION, FoundUpdate, REPO_URL, UpdatePlan, display_version, parse_version,
        plan_update, update_candidate,
    };

    #[test]
    fn the_manifest_version_and_repository_both_feed_the_check() {
        assert!(
            parse_version(CURRENT_VERSION).is_some(),
            "unparseable {CURRENT_VERSION:?}",
        );
        assert!(
            GithubRepo::parse(REPO_URL)
                .and_then(|repo| repo.release_page_url(CURRENT_VERSION))
                .is_some(),
            "no release page url from {REPO_URL:?}",
        );
    }

    #[test]
    fn plan_update_stays_silent_for_a_release_that_is_not_newer_or_was_already_reported() {
        for (label, current, latest, installable, rolled_back, reported) in [
            ("same version", "0.5.4", "0.5.4", true, None, None),
            ("older release", "0.5.4", "0.5.2", true, None, None),
            ("unparseable current", "nightly", "0.5.4", true, None, None),
            ("unparseable latest", "0.5.2", "latest", true, None, None),
            (
                "reported the latest",
                "0.5.2",
                "v0.5.4",
                true,
                None,
                Some("0.5.4"),
            ),
            (
                "reported a newer one",
                "0.5.2",
                "0.5.4",
                true,
                None,
                Some("v0.6.0"),
            ),
            (
                "reported the latest while not installable",
                "0.5.2",
                "0.5.4",
                false,
                None,
                Some("0.5.4"),
            ),
            (
                "reported the latest that also rolled back",
                "0.5.2",
                "0.5.4",
                true,
                Some("0.5.4"),
                Some("0.5.4"),
            ),
        ] {
            assert_eq!(
                plan_update(current, latest, installable, rolled_back, reported),
                None,
                "{label}",
            );
        }
    }

    #[test]
    fn plan_update_downloads_only_an_installable_release_that_has_not_rolled_back() {
        for (label, installable, rolled_back, reported, expected) in [
            ("installable", true, None, None, UpdatePlan::Download),
            ("not installable", false, None, None, UpdatePlan::Offer),
            (
                "rolled back the latest",
                true,
                Some("0.5.4"),
                None,
                UpdatePlan::Offer,
            ),
            (
                "rolled back the latest under a v tag",
                true,
                Some("v0.5.4"),
                None,
                UpdatePlan::Offer,
            ),
            (
                "rolled back a newer one",
                true,
                Some("0.6.0"),
                None,
                UpdatePlan::Offer,
            ),
            (
                "rolled back an older one",
                true,
                Some("0.5.3"),
                None,
                UpdatePlan::Download,
            ),
            (
                "unparseable rolled back version",
                true,
                Some("broken"),
                None,
                UpdatePlan::Download,
            ),
            (
                "rolled back an older one while not installable",
                false,
                Some("0.5.3"),
                None,
                UpdatePlan::Offer,
            ),
            (
                "reported an older one",
                true,
                None,
                Some("0.5.3"),
                UpdatePlan::Download,
            ),
            (
                "reported an older one while not installable",
                false,
                None,
                Some("0.5.3"),
                UpdatePlan::Offer,
            ),
        ] {
            assert_eq!(
                plan_update("0.5.2", "0.5.4", installable, rolled_back, reported),
                Some(expected),
                "{label}",
            );
        }
    }

    const OFFER_PAGE: &str = "https://example.invalid/releases/v0.9.0";

    fn artifact(platform: Platform, kind: ArtifactKind) -> Artifact {
        let kind = kind.id();
        Artifact {
            platform: platform.id().to_owned(),
            kind: kind.to_owned(),
            name: format!("forge-{}-{kind}", platform.id()),
            url: format!("https://example.invalid/{}-{kind}", platform.id()),
            sha256: Sha256Digest::of(kind.as_bytes()),
            size: 100,
        }
    }

    fn signed(artifacts: Vec<Artifact>) -> FoundUpdate {
        FoundUpdate::Signed(UpdateOffer {
            version: parse_version("0.9.0").expect("version"),
            published_at: "2026-10-15T00:00:00Z".to_owned(),
            release_page: OFFER_PAGE.to_owned(),
            artifacts,
        })
    }

    fn appimage_install() -> InstallKind {
        InstallKind::Updatable(UpdatableInstall {
            layout: InstallLayout::LinuxAppImage,
            target: PathBuf::from("/opt/forge/forge.AppImage"),
        })
    }

    fn linux_offer() -> Vec<Artifact> {
        vec![
            artifact(Platform::WindowsX64, ArtifactKind::Msi),
            artifact(Platform::LinuxX64, ArtifactKind::AppImage),
            artifact(Platform::MacosUniversal, ArtifactKind::AppBundleArchive),
        ]
    }

    #[test]
    fn a_signed_offer_for_an_updatable_install_targets_the_matching_artifact() {
        let candidate = update_candidate(
            signed(linux_offer()),
            &appimage_install(),
            Some(Platform::LinuxX64),
        );

        let target = candidate.install.map(|target| target.artifact);
        assert_eq!(
            target,
            Some(artifact(Platform::LinuxX64, ArtifactKind::AppImage))
        );
    }

    #[test]
    fn every_other_combination_falls_back_to_a_download_link() {
        let unsigned = FoundUpdate::Unsigned(PublishedRelease {
            tag: "v0.9.0".to_owned(),
            page_url: OFFER_PAGE.to_owned(),
        });
        for (label, found, install, platform) in [
            (
                "unsigned release",
                unsigned,
                appimage_install(),
                Some(Platform::LinuxX64),
            ),
            (
                "manual-only install",
                signed(linux_offer()),
                InstallKind::ManualOnly(ManualReason::LinuxPackageManaged),
                Some(Platform::LinuxX64),
            ),
            (
                "unknown platform",
                signed(linux_offer()),
                appimage_install(),
                None,
            ),
            (
                "no artifact for this platform",
                signed(vec![artifact(Platform::WindowsX64, ArtifactKind::AppImage)]),
                appimage_install(),
                Some(Platform::LinuxX64),
            ),
            (
                "no artifact of this install kind",
                signed(vec![artifact(Platform::LinuxX64, ArtifactKind::Msi)]),
                appimage_install(),
                Some(Platform::LinuxX64),
            ),
            (
                "empty manifest",
                signed(Vec::new()),
                appimage_install(),
                Some(Platform::LinuxX64),
            ),
        ] {
            let candidate = update_candidate(found, &install, platform);
            assert_eq!(candidate.install, None, "{label}");
            assert_eq!(candidate.release_page, OFFER_PAGE, "{label}");
        }
    }

    #[test]
    fn display_version_drops_the_tag_prefix_and_surrounding_space() {
        for raw in ["v0.9.0", " 0.9.0 ", "0.9.0", " v0.9.0\n"] {
            assert_eq!(display_version(raw), "0.9.0", "{raw:?}");
        }
    }

    #[test]
    fn signed_and_unsigned_offers_of_one_release_show_the_same_version() {
        let unsigned = FoundUpdate::Unsigned(PublishedRelease {
            tag: "v0.9.0".to_owned(),
            page_url: OFFER_PAGE.to_owned(),
        });
        let signed = signed(linux_offer());

        assert_eq!(unsigned.version(), signed.version());
        let install = appimage_install();
        assert_eq!(
            update_candidate(unsigned, &install, None).version,
            update_candidate(signed, &install, None).version,
        );
    }
}
