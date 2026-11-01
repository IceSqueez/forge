#![expect(clippy::panic, reason = "test fixtures assert directly")]
use std::collections::BTreeMap;
use std::ffi::OsString;

use forge_platform_core::{EndpointRefusal, EndpointSurface, PlatformEndpoints, PlatformError};
use forge_test_support::log_capture::{CapturedLine, capture};
use tracing::Level;

const HTTP_API: EndpointSurface = EndpointSurface::http("FORGE_TEST_API", "https://api.example");
const SOCKET: EndpointSurface =
    EndpointSurface::web_socket("FORGE_TEST_WS", "wss://socket.example/ws");
const OTHER_HTTP: EndpointSurface =
    EndpointSurface::http("FORGE_TEST_OTHER", "https://other.example");
const LATER_SOCKET: EndpointSurface =
    EndpointSurface::web_socket("FORGE_TEST_LATER_WS", "wss://later.example/app");
const LATER_HTTP: EndpointSurface =
    EndpointSurface::http("FORGE_TEST_LATER", "https://later.example/v3");
const ROSTER: &[EndpointSurface] = &[HTTP_API, SOCKET, OTHER_HTTP, LATER_SOCKET, LATER_HTTP];

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
    PlatformEndpoints::resolve(ROSTER, lookup_from(&[(surface, raw.into())]))
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
fn unconfigured_endpoints_answer_each_surfaces_declared_default() {
    let resolved = PlatformEndpoints::resolve(ROSTER, |_| None).unwrap();
    for endpoints in [PlatformEndpoints::default(), resolved] {
        assert_eq!(endpoints.base_url(HTTP_API), "https://api.example");
        assert_eq!(endpoints.base_url(SOCKET), "wss://socket.example/ws");
        assert_eq!(endpoints.overridden().count(), 0);
    }
}

#[test]
fn a_variable_of_a_surface_outside_the_roster_is_ignored() {
    let lookup = lookup_from(&[(HTTP_API, "http://10.0.0.1".into())]);
    let endpoints = PlatformEndpoints::resolve(&[SOCKET], lookup).unwrap();
    assert_eq!(endpoints, PlatformEndpoints::default());
}

#[test]
fn empty_value_counts_as_unset() {
    let endpoints = PlatformEndpoints::resolve(ROSTER, |_| Some(OsString::new())).unwrap();
    assert_eq!(endpoints, PlatformEndpoints::default());
}

#[test]
fn loopback_override_resolves_to_the_parsers_serialization() {
    let cases = [
        (HTTP_API, "http://127.0.0.1", "http://127.0.0.1"),
        (HTTP_API, "http://127.0.0.1:8080/", "http://127.0.0.1:8080"),
        (
            LATER_HTTP,
            "http://127.255.255.254:9/youtube/v3/",
            "http://127.255.255.254:9/youtube/v3",
        ),
        (HTTP_API, "https://[::1]:8443", "https://[::1]:8443"),
        (HTTP_API, "http://LOCALHOST:9000", "http://localhost:9000"),
        (HTTP_API, "http://127.1", "http://127.0.0.1"),
        (
            HTTP_API,
            "HTTP://127.0.0.1:80/helix",
            "http://127.0.0.1/helix",
        ),
        (
            HTTP_API,
            "http://\u{FF4C}\u{FF4F}\u{FF43}\u{FF41}\u{FF4C}\u{FF48}\u{FF4F}\u{FF53}\u{FF54}:7",
            "http://localhost:7",
        ),
        (SOCKET, "ws://127.0.0.1:4000/ws", "ws://127.0.0.1:4000/ws"),
        (LATER_SOCKET, "wss://localhost/app/", "wss://localhost/app"),
    ];
    for (surface, raw, expected) in cases {
        let endpoints = resolve_one(surface, raw)
            .unwrap_or_else(|err| panic!("expected {raw:?} accepted, got {err}"));
        assert_eq!(endpoints.base_url(surface), expected, "input {raw:?}");
    }
}

#[test]
fn override_redirects_only_the_named_surface() {
    let endpoints = resolve_one(OTHER_HTTP, "http://127.0.0.1:5000").unwrap();
    let overridden: Vec<(EndpointSurface, &str)> = endpoints.overridden().collect();
    assert_eq!(overridden, [(OTHER_HTTP, "http://127.0.0.1:5000")]);
    let untouched = PlatformEndpoints::default();
    for surface in ROSTER.iter().copied() {
        if surface != OTHER_HTTP {
            assert_eq!(endpoints.base_url(surface), untouched.base_url(surface));
        }
    }
}

#[test]
fn override_is_refused_with_the_reason_it_fails() {
    use EndpointRefusal::{
        EmbeddedCredentials, Malformed, NotLoopback, QueryOrFragment, SchemeMismatch,
    };
    let cases = [
        (HTTP_API, "http://10.0.0.1", NotLoopback),
        (OTHER_HTTP, "http://203.0.113.5/api", NotLoopback),
        (OTHER_HTTP, "https://discord.com/api", NotLoopback),
        (OTHER_HTTP, "wss://127.0.0.1/api", SchemeMismatch),
        (HTTP_API, "http://192.168.1.5:8080", NotLoopback),
        (HTTP_API, "http://126.255.255.255", NotLoopback),
        (HTTP_API, "http://128.0.0.0", NotLoopback),
        (HTTP_API, "http://0.0.0.0", NotLoopback),
        (HTTP_API, "http://[::ffff:127.0.0.1]", NotLoopback),
        (HTTP_API, "http://[::]", NotLoopback),
        (HTTP_API, "http://foo.localhost", NotLoopback),
        (HTTP_API, "http://localhost.", NotLoopback),
        (HTTP_API, "http://127.0.0.1.nip.io", NotLoopback),
        (LATER_SOCKET, "wss://ws-us2.pusher.com/app", NotLoopback),
        (HTTP_API, "http://127.0.0.1@evil.com", EmbeddedCredentials),
        (HTTP_API, "http://u:p@127.0.0.1", EmbeddedCredentials),
        (HTTP_API, "http://:p@127.0.0.1", EmbeddedCredentials),
        (HTTP_API, "ws://127.0.0.1", SchemeMismatch),
        (HTTP_API, "file:///etc/passwd", SchemeMismatch),
        (SOCKET, "http://127.0.0.1", SchemeMismatch),
        (LATER_SOCKET, "https://localhost", SchemeMismatch),
        (HTTP_API, "http://127.0.0.1/?x=1", QueryOrFragment),
        (HTTP_API, "http://127.0.0.1/?", QueryOrFragment),
        (SOCKET, "ws://127.0.0.1/ws#frag", QueryOrFragment),
        (HTTP_API, "localhost:8080", SchemeMismatch),
        (HTTP_API, "127.0.0.1:8080", Malformed),
        (HTTP_API, "http://", Malformed),
        (HTTP_API, "http://[::1", Malformed),
        (HTTP_API, " ", Malformed),
    ];
    for (surface, raw, expected) in cases {
        let (variable, reason) = refusal(resolve_one(surface, raw));
        assert_eq!(reason, expected, "input {raw:?} on {surface:?}");
        assert_eq!(variable, surface.env_var(), "input {raw:?}");
    }
}

#[test]
fn non_unicode_override_is_refused() {
    let lookup = lookup_from(&[(LATER_HTTP, non_unicode_value())]);
    let (variable, reason) = refusal(PlatformEndpoints::resolve(ROSTER, lookup));
    assert_eq!(reason, EndpointRefusal::NotUnicode);
    assert_eq!(variable, LATER_HTTP.env_var());
}

#[test]
fn refusal_names_the_first_invalid_surface_in_roster_order() {
    let lookup = lookup_from(&[
        (LATER_SOCKET, "http://127.0.0.1".into()),
        (HTTP_API, "http://127.0.0.1:1".into()),
        (LATER_HTTP, "http://10.0.0.1".into()),
        (SOCKET, "ws://10.0.0.1".into()),
    ]);
    let (variable, reason) = refusal(PlatformEndpoints::resolve(ROSTER, lookup));
    assert_eq!(variable, SOCKET.env_var());
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
        let err = resolve_one(HTTP_API, raw).unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains(HTTP_API.env_var()),
            "variable missing from {text:?}"
        );
        assert!(!text.contains("sentinel"), "raw value leaked into {text:?}");
    }
}

#[test]
fn override_emits_one_warn_naming_every_overridden_variable() {
    let lookup = lookup_from(&[
        (HTTP_API, "http://127.0.0.1:1".into()),
        (LATER_SOCKET, "ws://localhost:2/app".into()),
    ]);
    let (result, lines) = capture(Level::TRACE, || PlatformEndpoints::resolve(ROSTER, lookup));
    result.unwrap();
    let warns: Vec<&CapturedLine> = lines.iter().filter(|l| l.level == Level::WARN).collect();
    assert_eq!(warns.len(), 1, "expected exactly one WARN");
    let surfaces = warns[0].field("surfaces");
    for surface in [HTTP_API, LATER_SOCKET] {
        assert!(
            surfaces.is_some_and(|s| s.contains(surface.env_var())),
            "WARN surfaces field {surfaces:?} must name {}",
            surface.env_var()
        );
    }
}

#[test]
fn resolve_without_override_emits_no_log_line() {
    let (result, lines) = capture(Level::TRACE, || {
        PlatformEndpoints::resolve(ROSTER, |_| None)
    });
    result.unwrap();
    assert!(
        lines.is_empty(),
        "expected silence, captured {} lines",
        lines.len()
    );
}
