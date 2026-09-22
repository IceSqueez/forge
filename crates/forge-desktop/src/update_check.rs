use std::cmp::Ordering;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use forge_components::{Icon, ToastAction, ToastKind, tr};
use forge_events::Event;
use forge_platform_core::{
    BuiltinHealth, HealthValue, RateLimiter, TokenBucketRateLimiter, acquire_or_wait,
};
use forge_runtime::{EventBus, EventSubscription};
use forge_storage::{DataProvider, SettingsRepo, get_bool_setting, reserved_keys};
use gpui::{App, AsyncApp};
use tokio::runtime::Handle;

use crate::async_bridge::{EventBatch, detached, open_path, recv_event_batch};
use crate::integrations::ObsInstallSeed;
use crate::toasts::PushToast;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO_URL: &str = env!("CARGO_PKG_REPOSITORY");
const USER_AGENT_VALUE: &str = concat!("forge/", env!("CARGO_PKG_VERSION"));

const GITHUB_WEB_PREFIX: &str = "https://github.com/";
const GITHUB_API_REPOS: &str = "https://api.github.com/repos/";
const LATEST_RELEASE_PATH: &str = "/releases/latest";
const GIT_SUFFIX: &str = ".git";

const TAG_FIELD: &str = "tag_name";
const HTML_URL_FIELD: &str = "html_url";
const DRAFT_FIELD: &str = "draft";

const STREAM_STOPPED_KIND: &str = "obs.streaming.stopped";
const RECORD_STOPPED_KIND: &str = "obs.recording.stopped";

pub const NOTIFY_DEFAULT: bool = true;

const BOOT_SETTLE_DELAY: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const UPDATE_TOAST_DURATION: Duration = Duration::from_secs(12);

const GITHUB_ANON_REQUEST_BUDGET: u32 = 60;
const GITHUB_ANON_WINDOW: Duration = Duration::from_secs(60 * 60);
const CHECK_WEIGHT: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    core: (u64, u64, u64),
    pre: Vec<String>,
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core
            .cmp(&other.core)
            .then_with(|| compare_prerelease(&self.pre, &other.pre))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn parse_version(raw: &str) -> Option<Version> {
    let raw = raw.trim();
    let raw = raw.strip_prefix('v').unwrap_or(raw);
    let raw = raw.split('+').next().unwrap_or(raw);
    let (core, pre) = match raw.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (raw, None),
    };

    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }

    let pre = match pre {
        None => Vec::new(),
        Some(pre) => {
            let ids: Vec<String> = pre.split('.').map(str::to_owned).collect();
            if ids.iter().any(String::is_empty) {
                return None;
            }
            ids
        }
    };

    Some(Version {
        core: (major, minor, patch),
        pre,
    })
}

fn compare_prerelease(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (left, right) in a.iter().zip(b) {
                match compare_identifier(left, right) {
                    Ordering::Equal => continue,
                    other => return other,
                }
            }
            a.len().cmp(&b.len())
        }
    }
}

fn compare_identifier(a: &str, b: &str) -> Ordering {
    match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        (Ok(_), Err(_)) => Ordering::Less,
        (Err(_), Ok(_)) => Ordering::Greater,
        (Err(_), Err(_)) => a.cmp(b),
    }
}

/// A prerelease never counts as newer than a release build, so a beta cut for the next version
/// leaves a stable install alone.
pub fn is_newer(current: &Version, latest: &Version) -> bool {
    if current.pre.is_empty() && !latest.pre.is_empty() {
        return false;
    }
    latest > current
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub url: String,
}

pub fn release_from_json(body: &str) -> Option<Release> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    if value
        .get(DRAFT_FIELD)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }
    let tag = value.get(TAG_FIELD)?.as_str()?.trim().to_owned();
    let url = value.get(HTML_URL_FIELD)?.as_str()?.trim().to_owned();
    if tag.is_empty() || url.is_empty() {
        return None;
    }
    Some(Release { tag, url })
}

pub fn latest_release_url(repo_url: &str) -> Option<String> {
    let slug = repo_url
        .trim()
        .strip_prefix(GITHUB_WEB_PREFIX)?
        .trim_end_matches('/');
    let slug = slug.strip_suffix(GIT_SUFFIX).unwrap_or(slug);
    if slug.is_empty() {
        return None;
    }
    Some(format!("{GITHUB_API_REPOS}{slug}{LATEST_RELEASE_PATH}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateNotice {
    Raise,
    DeferUntilObsIdle,
}

pub fn should_notify(
    current: &str,
    latest: &str,
    dismissed_for: Option<&str>,
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
    if dismissed_for
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

struct Probe {
    enabled: bool,
    dismissed: Option<String>,
    release: Option<Release>,
}

pub fn start_update_check(
    cx: &mut AsyncApp,
    backend: Arc<dyn DataProvider>,
    rt_handle: Handle,
    obs: ObsInstallSeed,
    bus: Arc<EventBus>,
) {
    let backend_for_probe = Arc::clone(&backend);
    cx.spawn(async move |cx| {
        cx.background_executor().timer(BOOT_SETTLE_DELAY).await;

        let (tx, rx) = tokio::sync::oneshot::channel();
        let probe_handle = rt_handle.clone();
        probe_handle.spawn(async move {
            let _ = tx.send(probe(backend_for_probe).await);
        });
        let Ok(probe) = rx.await else {
            return;
        };
        let Some(release) = probe.release else {
            return;
        };

        let subscription = bus.subscribe();
        let Some(notice) = should_notify(
            CURRENT_VERSION,
            &release.tag,
            probe.dismissed.as_deref(),
            probe.enabled,
            obs_busy(&obs),
        ) else {
            return;
        };

        if notice == UpdateNotice::DeferUntilObsIdle && !wait_for_obs_idle(subscription, &obs).await
        {
            return;
        }

        cx.update(|cx| raise_update_toast(release, backend, rt_handle, cx));
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
            dismissed: None,
            release: None,
        };
    }
    let dismissed = repo
        .get_string(reserved_keys::UPDATES_DISMISSED_VERSION)
        .await
        .ok()
        .flatten();
    let release = fetch_latest_release().await;
    Probe {
        enabled,
        dismissed,
        release,
    }
}

fn github_limiter() -> &'static TokenBucketRateLimiter {
    static LIMITER: OnceLock<TokenBucketRateLimiter> = OnceLock::new();
    LIMITER
        .get_or_init(|| TokenBucketRateLimiter::new(GITHUB_ANON_REQUEST_BUDGET, GITHUB_ANON_WINDOW))
}

async fn fetch_latest_release() -> Option<Release> {
    let url = latest_release_url(REPO_URL)?;
    let limiter: &dyn RateLimiter = github_limiter();
    if let Err(error) = acquire_or_wait(limiter, CHECK_WEIGHT).await {
        tracing::debug!(error = %error, "update check declined by the rate limiter");
        return None;
    }

    let client = match reqwest::Client::builder().timeout(REQUEST_TIMEOUT).build() {
        Ok(client) => client,
        Err(error) => {
            let error = error.without_url();
            tracing::debug!(error = %error, "update check could not build a client");
            return None;
        }
    };

    let response = match client
        .get(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT_VALUE)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            let error = error.without_url();
            tracing::debug!(error = %error, "update check request failed");
            return None;
        }
    };

    let status = response.status();
    if !status.is_success() {
        tracing::debug!(status = %status, "update check answered with a non-success status");
        return None;
    }

    match response.text().await {
        Ok(body) => release_from_json(&body),
        Err(error) => {
            let error = error.without_url();
            tracing::debug!(error = %error, "update check response could not be read");
            None
        }
    }
}

fn obs_busy(obs: &ObsInstallSeed) -> bool {
    obs.live().is_some_and(|client| {
        client
            .metrics()
            .iter()
            .any(|metric| matches!(metric.value, HealthValue::Status { active: true, .. }))
    })
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

fn raise_update_toast(
    release: Release,
    backend: Arc<dyn DataProvider>,
    rt_handle: Handle,
    cx: &mut App,
) {
    let url = release.url.clone();
    let open_handle = rt_handle.clone();
    cx.push_toast_full(
        ToastKind::Info,
        tr!("update_toast_available", version = release.tag.as_str()),
        Some(Icon::Sparkles),
        Some(ToastAction::new(
            tr!("update_toast_open_action"),
            move |_window, _app: &mut App| {
                detached(
                    &open_handle,
                    "open the release page",
                    open_path(url.clone()),
                );
            },
        )),
        UPDATE_TOAST_DURATION,
    );
    remember_notified(release.tag, backend, &rt_handle);
}

fn remember_notified(tag: String, backend: Arc<dyn DataProvider>, rt_handle: &Handle) {
    detached(rt_handle, "remember the notified version", async move {
        let repo = backend as Arc<dyn SettingsRepo>;
        repo.set_string(reserved_keys::UPDATES_DISMISSED_VERSION, &tag)
            .await
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{
        CURRENT_VERSION, REPO_URL, Release, UpdateNotice, Version, is_newer, latest_release_url,
        parse_version, release_from_json, should_notify,
    };

    fn version(raw: &str) -> Version {
        parse_version(raw).unwrap()
    }

    #[test]
    fn parse_version_tolerates_a_v_prefix_build_metadata_and_surrounding_padding() {
        let plain = version("0.5.3");
        for raw in [
            " 0.5.3 ",
            "v0.5.3",
            "0.5.3+build.9",
            "v0.5.3+build.9",
            " v0.5.3 ",
        ] {
            assert_eq!(parse_version(raw).as_ref(), Some(&plain), "for {raw:?}");
        }
    }

    #[test]
    fn parse_version_keeps_a_prerelease_that_carries_build_metadata() {
        assert_eq!(
            parse_version("v0.5.3-beta.1+build.9"),
            parse_version("0.5.3-beta.1"),
        );
        assert_ne!(parse_version("0.5.3-beta.1"), parse_version("0.5.3"));
    }

    #[test]
    fn parse_version_rejects_anything_but_three_numeric_components() {
        for raw in [
            "",
            "   ",
            "0",
            "0.5",
            "0.5.3.1",
            "0.5.x",
            "0.-5.3",
            "next release",
            "not-a-version",
            "18446744073709551616.0.0",
        ] {
            assert!(parse_version(raw).is_none(), "accepted {raw:?}");
        }
    }

    #[test]
    fn parse_version_rejects_an_empty_prerelease_identifier() {
        for raw in ["0.5.3-", "0.5.3-beta.", "0.5.3-.1", "0.5.3-beta..1"] {
            assert!(parse_version(raw).is_none(), "accepted {raw:?}");
        }
    }

    #[test]
    fn version_ordering_follows_semver_precedence() {
        for (lower, higher) in [
            ("0.5.2", "0.5.3"),
            ("0.5.9", "0.6.0"),
            ("0.9.9", "1.0.0"),
            ("0.5.3-beta.1", "0.5.3"),
            ("0.5.3-beta.2", "0.5.3-beta.10"),
            ("0.5.3-alpha.9", "0.5.3-beta.1"),
            ("1.0.0-alpha", "1.0.0-alpha.1"),
            ("1.0.0-1", "1.0.0-alpha"),
        ] {
            assert!(
                version(lower) < version(higher),
                "expected {lower} below {higher}",
            );
        }
    }

    #[test]
    fn is_newer_never_offers_a_prerelease_to_a_release_build() {
        for latest in ["0.5.3-beta.1", "0.6.0-rc.1", "1.0.0-alpha"] {
            assert!(
                !is_newer(&version("0.5.2"), &version(latest)),
                "offered {latest} to a release build",
            );
        }
    }

    #[test]
    fn is_newer_offers_a_release_build_only_a_strictly_higher_release() {
        for (latest, expected) in [
            ("0.5.3", true),
            ("1.0.0", true),
            ("0.5.2", false),
            ("0.5.1", false),
        ] {
            assert_eq!(
                is_newer(&version("0.5.2"), &version(latest)),
                expected,
                "for {latest}",
            );
        }
    }

    #[test]
    fn is_newer_offers_a_prerelease_build_the_next_prerelease_and_the_release() {
        for (latest, expected) in [
            ("0.5.3-beta.2", true),
            ("0.5.3", true),
            ("0.5.4-beta.1", true),
            ("0.5.3-beta.1", false),
            ("0.5.3-alpha.9", false),
            ("0.5.2", false),
        ] {
            assert_eq!(
                is_newer(&version("0.5.3-beta.1"), &version(latest)),
                expected,
                "for {latest}",
            );
        }
    }

    #[test]
    fn release_from_json_takes_the_trimmed_tag_and_page_of_a_published_release() {
        for body in [
            r#"{"tag_name":" v0.6.0 ","html_url":" https://example.invalid/r ","draft":false}"#,
            r#"{"tag_name":"v0.6.0","html_url":"https://example.invalid/r"}"#,
        ] {
            assert_eq!(
                release_from_json(body),
                Some(Release {
                    tag: "v0.6.0".to_owned(),
                    url: "https://example.invalid/r".to_owned(),
                }),
                "for {body:?}",
            );
        }
    }

    #[test]
    fn release_from_json_rejects_a_draft_a_missing_field_and_a_body_that_is_not_an_object() {
        for body in [
            r#"{"tag_name":"v0.6.0","html_url":"https://example.invalid/r","draft":true}"#,
            r#"{"html_url":"https://example.invalid/r"}"#,
            r#"{"tag_name":"v0.6.0"}"#,
            r#"{"tag_name":"   ","html_url":"https://example.invalid/r"}"#,
            r#"{"tag_name":"v0.6.0","html_url":""}"#,
            r#"{"tag_name":42,"html_url":"https://example.invalid/r"}"#,
            r#"["v0.6.0"]"#,
            r#""v0.6.0""#,
            "null",
            "{not json",
            "",
        ] {
            assert_eq!(release_from_json(body), None, "accepted {body:?}");
        }
    }

    #[test]
    fn latest_release_url_reaches_the_api_whatever_the_web_url_spelling() {
        for repo in [
            "https://github.com/nova/forge",
            "https://github.com/nova/forge.git",
            "https://github.com/nova/forge/",
            "https://github.com/nova/forge.git/",
            "  https://github.com/nova/forge  ",
        ] {
            assert_eq!(
                latest_release_url(repo).as_deref(),
                Some("https://api.github.com/repos/nova/forge/releases/latest"),
                "for {repo:?}",
            );
        }
    }

    #[test]
    fn latest_release_url_is_none_for_anything_but_a_github_web_url() {
        for repo in [
            "https://gitlab.com/nova/forge",
            "http://github.com/nova/forge",
            "git@github.com:nova/forge.git",
            "https://github.com/",
            "https://github.com//",
            "",
        ] {
            assert_eq!(latest_release_url(repo), None, "accepted {repo:?}");
        }
    }

    #[test]
    fn the_manifest_version_and_repository_both_feed_the_check() {
        assert!(
            parse_version(CURRENT_VERSION).is_some(),
            "unparseable {CURRENT_VERSION:?}",
        );
        assert!(
            latest_release_url(REPO_URL).is_some(),
            "no release api url from {REPO_URL:?}",
        );
    }

    #[test]
    fn should_notify_raises_for_a_newer_release_the_user_has_not_been_shown() {
        for dismissed in [None, Some("0.5.2"), Some("v0.5.2"), Some("not a version")] {
            assert_eq!(
                should_notify("0.5.2", "v0.5.3", dismissed, true, false),
                Some(UpdateNotice::Raise),
                "for dismissed {dismissed:?}",
            );
        }
    }

    #[test]
    fn should_notify_defers_a_newer_release_while_obs_is_live() {
        assert_eq!(
            should_notify("0.5.2", "0.5.3", None, true, true),
            Some(UpdateNotice::DeferUntilObsIdle),
        );
    }

    #[test]
    fn should_notify_stays_silent_when_disabled_already_shown_or_not_newer() {
        for (label, current, latest, dismissed, enabled, obs_live) in [
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
                "already shown",
                "0.5.2",
                "0.5.3",
                Some("0.5.3"),
                true,
                false,
            ),
            (
                "already shown under a v tag",
                "0.5.2",
                "v0.5.3",
                Some("v0.5.3"),
                true,
                false,
            ),
            (
                "shown a later version",
                "0.5.2",
                "0.5.3",
                Some("0.6.0"),
                true,
                false,
            ),
            ("unparseable current", "nightly", "0.5.3", None, true, false),
            ("unparseable latest", "0.5.2", "latest", None, true, false),
            (
                "already shown while obs is live",
                "0.5.2",
                "0.5.3",
                Some("0.5.3"),
                true,
                true,
            ),
        ] {
            assert_eq!(
                should_notify(current, latest, dismissed, enabled, obs_live),
                None,
                "{label}",
            );
        }
    }
}
