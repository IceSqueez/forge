use std::sync::Arc;
use std::time::Duration;

use forge_events::Event;
use forge_platform_core::{BuiltinHealth, HealthMetric, HealthValue};
use forge_runtime::{EventBus, EventSubscription};
use forge_storage::{DataProvider, SettingsRepo, get_bool_setting, reserved_keys};
use forge_update::{
    GithubRepo, InstallKind, ManualReason, Platform, PublishedRelease, TrustedKeys, UpdateClient,
    UpdateOffer, check_for_update, detect_install, fetch_latest_release, is_newer, parse_version,
};
use gpui::{AsyncApp, Entity};
use tokio::runtime::Handle;

use crate::async_bridge::{EventBatch, detached, recv_event_batch};
use crate::integrations::ObsInstallSeed;
use crate::update_flow::{InstallTarget, UpdateCandidate, UpdateFlow};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPO_URL: &str = env!("CARGO_PKG_REPOSITORY");

const STREAM_STOPPED_KIND: &str = "obs.streaming.stopped";
const RECORD_STOPPED_KIND: &str = "obs.recording.stopped";

pub const NOTIFY_DEFAULT: bool = true;

const BOOT_SETTLE_DELAY: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateNotice {
    Raise,
    DeferUntilObsIdle,
}

pub fn should_notify(
    current: &str,
    latest: &str,
    skipped: Option<&str>,
    enabled: bool,
    obs_live: bool,
) -> Option<UpdateNotice> {
    if !enabled {
        return None;
    }
    let current = parse_version(current)?;
    let latest = parse_version(latest)?;
    if !is_newer(&current, &latest) {
        return None;
    }
    if skipped
        .and_then(parse_version)
        .is_some_and(|seen| seen >= latest)
    {
        return None;
    }
    Some(if obs_live {
        UpdateNotice::DeferUntilObsIdle
    } else {
        UpdateNotice::Raise
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

struct Probe {
    enabled: bool,
    skipped: Option<String>,
    found: Option<FoundUpdate>,
    install: InstallKind,
}

pub fn start_update_check(
    cx: &mut AsyncApp,
    backend: Arc<dyn DataProvider>,
    rt_handle: Handle,
    obs: ObsInstallSeed,
    bus: Arc<EventBus>,
    flow: Entity<UpdateFlow>,
) {
    cx.spawn(async move |cx| {
        cx.background_executor().timer(BOOT_SETTLE_DELAY).await;

        let (tx, rx) = tokio::sync::oneshot::channel();
        rt_handle.spawn(async move {
            let _ = tx.send(probe(backend).await);
        });
        let Ok(probe) = rx.await else {
            return;
        };
        let Some(found) = probe.found else {
            return;
        };

        let subscription = bus.subscribe();
        let Some(notice) = should_notify(
            CURRENT_VERSION,
            &found.version(),
            probe.skipped.as_deref(),
            probe.enabled,
            obs_busy(&obs),
        ) else {
            return;
        };

        if notice == UpdateNotice::DeferUntilObsIdle && !wait_for_obs_idle(subscription, &obs).await
        {
            return;
        }

        let candidate = update_candidate(found, &probe.install, Platform::running());
        flow.update(cx, |flow, cx| flow.offer(candidate, cx));
    })
    .detach();
}

async fn probe(backend: Arc<dyn DataProvider>) -> Probe {
    let repo = backend as Arc<dyn SettingsRepo>;
    let enabled =
        get_bool_setting(repo.as_ref(), reserved_keys::UPDATES_NOTIFY, NOTIFY_DEFAULT).await;
    if !enabled {
        return Probe {
            enabled,
            skipped: None,
            found: None,
            install: InstallKind::ManualOnly(ManualReason::NotReleaseBuild),
        };
    }
    let skipped = repo
        .get_string(reserved_keys::UPDATES_SKIPPED_VERSION)
        .await
        .ok()
        .flatten();
    let found = find_update().await;
    let install = tokio::task::spawn_blocking(detect_install)
        .await
        .unwrap_or(InstallKind::ManualOnly(ManualReason::UnknownExecutable));
    Probe {
        enabled,
        skipped,
        found,
        install,
    }
}

async fn find_update() -> Option<FoundUpdate> {
    let repo = GithubRepo::parse(REPO_URL)?;
    let client = match UpdateClient::for_github() {
        Ok(client) => client,
        Err(error) => {
            tracing::debug!(error = %error, "update check could not start");
            return None;
        }
    };
    match check_signed(&client, &repo).await {
        SignedCheck::Offer(offer) => Some(FoundUpdate::Signed(offer)),
        SignedCheck::UpToDate => None,
        SignedCheck::Unavailable => latest_release(&client, &repo)
            .await
            .map(FoundUpdate::Unsigned),
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

async fn latest_release(client: &UpdateClient, repo: &GithubRepo) -> Option<PublishedRelease> {
    match fetch_latest_release(client, repo).await {
        Ok(release) => release,
        Err(error) => {
            tracing::debug!(error = %error, "latest release lookup failed");
            None
        }
    }
}

fn obs_busy(obs: &ObsInstallSeed) -> bool {
    obs.live()
        .is_some_and(|client| any_output_active(&client.metrics()))
}

fn any_output_active(metrics: &[HealthMetric]) -> bool {
    metrics
        .iter()
        .any(|metric| matches!(metric.value, HealthValue::Status { active: true, .. }))
}

async fn wait_for_obs_idle(mut subscription: EventSubscription, obs: &ObsInstallSeed) -> bool {
    loop {
        match recv_event_batch(&mut subscription).await {
            EventBatch::Ready(batch) => {
                if batch.iter().any(output_stopped) && !obs_busy(obs) {
                    return true;
                }
            }
            EventBatch::Closed => return false,
        }
    }
}

fn output_stopped(event: &Event) -> bool {
    event.kind == STREAM_STOPPED_KIND || event.kind == RECORD_STOPPED_KIND
}

pub fn remember_skipped(tag: String, backend: Arc<dyn DataProvider>, rt_handle: &Handle) {
    detached(rt_handle, "remember the skipped version", async move {
        let repo = backend as Arc<dyn SettingsRepo>;
        repo.set_string(reserved_keys::UPDATES_SKIPPED_VERSION, &tag)
            .await
    });
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use forge_platform_core::{HealthMetric, HealthValue};

    use std::path::PathBuf;

    use forge_update::{
        Artifact, ArtifactKind, GithubRepo, InstallKind, InstallLayout, ManualReason, Platform,
        PublishedRelease, Sha256Digest, UpdatableInstall, UpdateOffer,
    };

    use super::{
        CURRENT_VERSION, FoundUpdate, REPO_URL, UpdateNotice, any_output_active, display_version,
        parse_version, should_notify, update_candidate,
    };

    fn status(active: bool) -> HealthMetric {
        HealthMetric {
            label: "Stream".to_owned(),
            value: HealthValue::Status {
                label: if active { "Live" } else { "Off" }.to_owned(),
                active,
                detail: None,
            },
        }
    }

    fn text() -> HealthMetric {
        HealthMetric {
            label: "Scene".to_owned(),
            value: HealthValue::Text {
                primary: "Starting soon".to_owned(),
                secondary: None,
            },
        }
    }

    fn ratio() -> HealthMetric {
        HealthMetric {
            label: "Buffer".to_owned(),
            value: HealthValue::Ratio {
                used: 3,
                total: 4,
                reset_hint: None,
            },
        }
    }

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
    fn should_notify_raises_for_a_newer_release_the_user_has_not_skipped() {
        for (label, skipped) in [
            ("nothing skipped", None),
            ("skipped the running version", Some("0.5.2")),
            ("skipped the running version under a v tag", Some("v0.5.2")),
            ("skipped one patch below the latest", Some("v0.5.3")),
            ("skipped a prerelease of the latest", Some("0.5.4-beta.1")),
            ("unparseable skip", Some("not a version")),
        ] {
            assert_eq!(
                should_notify("0.5.2", "v0.5.4", skipped, true, false),
                Some(UpdateNotice::Raise),
                "{label}",
            );
        }
    }

    #[test]
    fn should_notify_defers_an_unskipped_newer_release_while_obs_is_live() {
        for skipped in [None, Some("0.5.3")] {
            assert_eq!(
                should_notify("0.5.2", "0.5.4", skipped, true, true),
                Some(UpdateNotice::DeferUntilObsIdle),
                "for skipped {skipped:?}",
            );
        }
    }

    #[test]
    fn should_notify_stays_silent_when_disabled_skipped_or_not_newer() {
        for (label, current, latest, skipped, enabled, obs_live) in [
            ("disabled", "0.5.2", "0.5.3", None, false, false),
            (
                "disabled while obs is live",
                "0.5.2",
                "0.5.3",
                None,
                false,
                true,
            ),
            ("same version", "0.5.3", "0.5.3", None, true, false),
            ("older release", "0.5.3", "0.5.2", None, true, false),
            (
                "prerelease for a release build",
                "0.5.2",
                "0.5.3-beta.1",
                None,
                true,
                false,
            ),
            (
                "skipped exactly the latest",
                "0.5.2",
                "0.5.3",
                Some("0.5.3"),
                true,
                false,
            ),
            (
                "skipped the latest under a v tag",
                "0.5.2",
                "v0.5.3",
                Some("v0.5.3"),
                true,
                false,
            ),
            (
                "skipped a v tag against a bare latest",
                "0.5.2",
                "0.5.3",
                Some("v0.5.3"),
                true,
                false,
            ),
            (
                "skipped a later version",
                "0.5.2",
                "0.5.3",
                Some("0.6.0"),
                true,
                false,
            ),
            ("unparseable current", "nightly", "0.5.3", None, true, false),
            ("unparseable latest", "0.5.2", "latest", None, true, false),
            (
                "skipped the latest while obs is live",
                "0.5.2",
                "0.5.3",
                Some("0.5.3"),
                true,
                true,
            ),
        ] {
            assert_eq!(
                should_notify(current, latest, skipped, enabled, obs_live),
                None,
                "{label}",
            );
        }
    }

    #[test]
    fn any_output_active_holds_the_update_back_only_for_a_running_status_metric() {
        for (label, metrics, expected) in [
            ("no metrics at all", Vec::new(), false),
            (
                "every status idle",
                vec![status(false), status(false)],
                false,
            ),
            (
                "one running status among idle ones",
                vec![status(false), status(true), status(false)],
                true,
            ),
            (
                "a running status is the last metric",
                vec![status(false), status(true)],
                true,
            ),
            ("no status metric at all", vec![text(), ratio()], false),
            (
                "a running status beside non-status metrics",
                vec![text(), status(true), ratio()],
                true,
            ),
            (
                "only idle statuses beside non-status metrics",
                vec![text(), status(false), ratio()],
                false,
            ),
        ] {
            assert_eq!(any_output_active(&metrics), expected, "{label}");
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
