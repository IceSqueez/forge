#![allow(clippy::panic)]
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;

use forge_platform_core::{EndpointRefusal, EndpointSurface, PlatformEndpoints, PlatformError};
use forge_test_support::log_capture::{CapturedLine, capture};
use tracing::Level;

fn lookup_from(
    pairs: &[(EndpointSurface, OsString)],
) -> impl Fn(&'static str) -> Option<OsString> + use<> {
    let by_variable: BTreeMap<&'static str, OsString> = pairs
        .iter()
        .map(|(surface, value)| (surface.env_var(), value.clone()))
        .collect();
    move |variable| by_variable.get(variable).cloned()
}

fn resolve_one(surface: EndpointSurface, raw: &str) -> Result<PlatformEndpoints, PlatformError> {
    PlatformEndpoints::resolve(lookup_from(&[(surface, raw.into())]))
}

fn refusal(result: Result<PlatformEndpoints, PlatformError>) -> (&'static str, EndpointRefusal) {
    match result {
        Err(PlatformError::EndpointOverrideRefused { variable, reason }) => (variable, reason),
        Err(other) => panic!("expected EndpointOverrideRefused, got {other:?}"),
        Ok(endpoints) => panic!("expected a refusal, resolved {endpoints:?}"),
    }
}

#[cfg(unix)]
fn non_unicode_value() -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(b"http://127.0.0.1/\xff".to_vec())
}

#[cfg(windows)]
fn non_unicode_value() -> OsString {
    use std::os::windows::ffi::OsStringExt;
    let mut wide: Vec<u16> = "http://127.0.0.1/".encode_utf16().collect();
    wide.push(0xD800);
    OsString::from_wide(&wide)
}

#[test]
fn unconfigured_endpoints_reach_the_production_services_over_tls() {
    let production = [
        (EndpointSurface::TwitchApi, "https://api.twitch.tv"),
        (
            EndpointSurface::TwitchEventSubSocket,
            "wss://eventsub.wss.twitch.tv/ws",
        ),
        (EndpointSurface::TwitchOAuth, "https://id.twitch.tv/oauth2"),
        (
            EndpointSurface::KickPublicApi,
            "https://api.kick.com/public/v1",
        ),
        (EndpointSurface::KickChannelApi, "https://kick.com/api/v2"),
        (
            EndpointSurface::KickChatSocket,
            "wss://ws-us2.pusher.com/app",
        ),
        (EndpointSurface::KickOAuth, "https://id.kick.com/oauth"),
        (
            EndpointSurface::YouTubeDataApi,
            "https://www.googleapis.com/youtube/v3",
        ),
        (
            EndpointSurface::YouTubeUploadApi,
            "https://www.googleapis.com/upload/youtube/v3",
        ),
        (
            EndpointSurface::YouTubeOAuth,
            "https://oauth2.googleapis.com",
        ),
        (EndpointSurface::DonatelloApi, "https://donatello.to/api/v1"),
        (EndpointSurface::MonobankApi, "https://api.monobank.ua"),
        (EndpointSurface::DiscordApi, "https://discord.com/api"),
    ];
    let pinned: BTreeSet<EndpointSurface> = production.iter().map(|(s, _)| *s).collect();
    assert_eq!(
        pinned,
        EndpointSurface::ALL.into_iter().collect(),
        "every surface in ALL must have a pinned production endpoint"
    );

    let resolved = PlatformEndpoints::resolve(|_| None).unwrap();
    for endpoints in [PlatformEndpoints::default(), resolved] {
        for (surface, url) in production {
            assert_eq!(endpoints.base_url(surface), url, "{surface:?}");
        }
        assert_eq!(endpoints.overridden().count(), 0);
    }
}

#[test]
fn empty_value_counts_as_unset() {
    let endpoints = PlatformEndpoints::resolve(|_| Some(OsString::new())).unwrap();
    assert_eq!(endpoints, PlatformEndpoints::default());
}

#[test]
fn each_surface_reads_a_distinct_variable() {
    let variables: BTreeSet<&str> = EndpointSurface::ALL
        .into_iter()
        .map(EndpointSurface::env_var)
        .collect();
    assert_eq!(variables.len(), EndpointSurface::ALL.len());
}

#[test]
fn loopback_override_resolves_to_the_parsers_serialization() {
    use EndpointSurface::{KickChatSocket, TwitchApi, TwitchEventSubSocket, YouTubeDataApi};
    let cases = [
        (TwitchApi, "http://127.0.0.1", "http://127.0.0.1"),
        (TwitchApi, "http://127.0.0.1:8080/", "http://127.0.0.1:8080"),
        (
            YouTubeDataApi,
            "http://127.255.255.254:9/youtube/v3/",
            "http://127.255.255.254:9/youtube/v3",
        ),
        (TwitchApi, "https://[::1]:8443", "https://[::1]:8443"),
        (TwitchApi, "http://LOCALHOST:9000", "http://localhost:9000"),
        (TwitchApi, "http://127.1", "http://127.0.0.1"),
        (
            TwitchApi,
            "HTTP://127.0.0.1:80/helix",
            "http://127.0.0.1/helix",
        ),
        (
            TwitchApi,
            "http://\u{FF4C}\u{FF4F}\u{FF43}\u{FF41}\u{FF4C}\u{FF48}\u{FF4F}\u{FF53}\u{FF54}:7",
            "http://localhost:7",
        ),
        (
            TwitchEventSubSocket,
            "ws://127.0.0.1:4000/ws",
            "ws://127.0.0.1:4000/ws",
        ),
        (
            KickChatSocket,
            "wss://localhost/app/",
            "wss://localhost/app",
        ),
    ];
    for (surface, raw, expected) in cases {
        let endpoints = resolve_one(surface, raw)
            .unwrap_or_else(|err| panic!("expected {raw:?} accepted, got {err}"));
        assert_eq!(endpoints.base_url(surface), expected, "input {raw:?}");
    }
}

#[test]
fn override_redirects_only_the_named_surface() {
    let endpoints = resolve_one(EndpointSurface::KickPublicApi, "http://127.0.0.1:5000").unwrap();
    let overridden: Vec<(EndpointSurface, &str)> = endpoints.overridden().collect();
    assert_eq!(
        overridden,
        [(EndpointSurface::KickPublicApi, "http://127.0.0.1:5000")]
    );
    let untouched = PlatformEndpoints::default();
    for surface in EndpointSurface::ALL {
        if surface != EndpointSurface::KickPublicApi {
            assert_eq!(endpoints.base_url(surface), untouched.base_url(surface));
        }
    }
}

#[test]
fn override_is_refused_with_the_reason_it_fails() {
    use EndpointRefusal::{
        EmbeddedCredentials, Malformed, NotLoopback, QueryOrFragment, SchemeMismatch,
    };
    use EndpointSurface::{DiscordApi, KickChatSocket, TwitchApi, TwitchEventSubSocket};
    let cases = [
        (TwitchApi, "http://10.0.0.1", NotLoopback),
        (DiscordApi, "http://203.0.113.5/api", NotLoopback),
        (DiscordApi, "https://discord.com/api", NotLoopback),
        (DiscordApi, "wss://127.0.0.1/api", SchemeMismatch),
        (TwitchApi, "http://192.168.1.5:8080", NotLoopback),
        (TwitchApi, "http://126.255.255.255", NotLoopback),
        (TwitchApi, "http://128.0.0.0", NotLoopback),
        (TwitchApi, "http://0.0.0.0", NotLoopback),
        (TwitchApi, "http://[::ffff:127.0.0.1]", NotLoopback),
        (TwitchApi, "http://[::]", NotLoopback),
        (TwitchApi, "http://foo.localhost", NotLoopback),
        (TwitchApi, "http://localhost.", NotLoopback),
        (TwitchApi, "http://127.0.0.1.nip.io", NotLoopback),
        (KickChatSocket, "wss://ws-us2.pusher.com/app", NotLoopback),
        (TwitchApi, "http://127.0.0.1@evil.com", EmbeddedCredentials),
        (TwitchApi, "http://u:p@127.0.0.1", EmbeddedCredentials),
        (TwitchApi, "http://:p@127.0.0.1", EmbeddedCredentials),
        (TwitchApi, "ws://127.0.0.1", SchemeMismatch),
        (TwitchApi, "file:///etc/passwd", SchemeMismatch),
        (TwitchEventSubSocket, "http://127.0.0.1", SchemeMismatch),
        (KickChatSocket, "https://localhost", SchemeMismatch),
        (TwitchApi, "http://127.0.0.1/?x=1", QueryOrFragment),
        (TwitchApi, "http://127.0.0.1/?", QueryOrFragment),
        (
            TwitchEventSubSocket,
            "ws://127.0.0.1/ws#frag",
            QueryOrFragment,
        ),
        (TwitchApi, "localhost:8080", SchemeMismatch),
        (TwitchApi, "127.0.0.1:8080", Malformed),
        (TwitchApi, "http://", Malformed),
        (TwitchApi, "http://[::1", Malformed),
        (TwitchApi, " ", Malformed),
    ];
    for (surface, raw, expected) in cases {
        let (variable, reason) = refusal(resolve_one(surface, raw));
        assert_eq!(reason, expected, "input {raw:?} on {surface:?}");
        assert_eq!(variable, surface.env_var(), "input {raw:?}");
    }
}

#[test]
fn non_unicode_override_is_refused() {
    let lookup = lookup_from(&[(EndpointSurface::YouTubeUploadApi, non_unicode_value())]);
    let (variable, reason) = refusal(PlatformEndpoints::resolve(lookup));
    assert_eq!(reason, EndpointRefusal::NotUnicode);
    assert_eq!(variable, EndpointSurface::YouTubeUploadApi.env_var());
}

#[test]
fn refusal_names_the_first_invalid_surface_in_roster_order() {
    let lookup = lookup_from(&[
        (EndpointSurface::KickChatSocket, "http://127.0.0.1".into()),
        (EndpointSurface::TwitchApi, "http://127.0.0.1:1".into()),
        (EndpointSurface::YouTubeUploadApi, "http://10.0.0.1".into()),
        (
            EndpointSurface::TwitchEventSubSocket,
            "ws://10.0.0.1".into(),
        ),
    ]);
    let (variable, reason) = refusal(PlatformEndpoints::resolve(lookup));
    assert_eq!(variable, EndpointSurface::TwitchEventSubSocket.env_var());
    assert_eq!(reason, EndpointRefusal::NotLoopback);
}

#[test]
fn refusal_text_names_the_variable_but_never_echoes_the_value() {
    let cases = [
        "http://exfil-sentinel-7f3a.example/helix",
        "http://sentinel-user:sentinel-secret-7f3a@127.0.0.1",
        "http://127.0.0.1/?token=sentinel-secret-7f3a",
    ];
    for raw in cases {
        let err = resolve_one(EndpointSurface::TwitchApi, raw).unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains(EndpointSurface::TwitchApi.env_var()),
            "variable missing from {text:?}"
        );
        assert!(!text.contains("sentinel"), "raw value leaked into {text:?}");
    }
}

#[test]
fn override_emits_one_warn_naming_every_overridden_variable() {
    let lookup = lookup_from(&[
        (EndpointSurface::TwitchApi, "http://127.0.0.1:1".into()),
        (
            EndpointSurface::KickChatSocket,
            "ws://localhost:2/app".into(),
        ),
    ]);
    let (result, lines) = capture(Level::TRACE, || PlatformEndpoints::resolve(lookup));
    result.unwrap();
    let warns: Vec<&CapturedLine> = lines.iter().filter(|l| l.level == Level::WARN).collect();
    assert_eq!(warns.len(), 1, "expected exactly one WARN");
    let surfaces = warns[0].field("surfaces");
    for surface in [EndpointSurface::TwitchApi, EndpointSurface::KickChatSocket] {
        assert!(
            surfaces.is_some_and(|s| s.contains(surface.env_var())),
            "WARN surfaces field {surfaces:?} must name {}",
            surface.env_var()
        );
    }
}

#[test]
fn resolve_without_override_emits_no_log_line() {
    let (result, lines) = capture(Level::TRACE, || PlatformEndpoints::resolve(|_| None));
    result.unwrap();
    assert!(
        lines.is_empty(),
        "expected silence, captured {} lines",
        lines.len()
    );
}
