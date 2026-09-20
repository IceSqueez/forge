use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};

const SCHEMES: [&str; 2] = ["http", "https"];
const SCHEME_SEPARATOR: &str = "://";
const PORT_SEPARATOR: char = ':';
const IPV6_LITERAL_OPEN: char = '[';
const IPV6_LITERAL_CLOSE: char = ']';
const LOCALHOST: &str = "localhost";

fn loopback_origins(port: u16) -> HashSet<String> {
    let mut origins = HashSet::new();
    for scheme in SCHEMES {
        origins.insert(format!("{scheme}://127.0.0.1:{port}"));
        origins.insert(format!("{scheme}://localhost:{port}"));
        origins.insert(format!("{scheme}://[::1]:{port}"));
    }
    origins
}

pub(crate) fn build_allowed_origins(bind_addr: SocketAddr, extra: &[String]) -> HashSet<String> {
    let mut origins = loopback_origins(bind_addr.port());
    for raw in extra {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            origins.insert(trimmed.to_ascii_lowercase());
        }
    }
    origins
}

/// An absent Origin header is accepted; only a present, non-matching Origin is rejected.
pub(crate) fn is_origin_allowed(allowed: &HashSet<String>, origin_header: Option<&str>) -> bool {
    match origin_header {
        None => true,
        Some(raw) => allowed.contains(&raw.trim().to_ascii_lowercase()),
    }
}

pub(crate) fn accepts_origin(
    allowed: &HashSet<String>,
    origin_header: Option<&str>,
    host_header: Option<&str>,
) -> bool {
    is_origin_allowed(allowed, origin_header)
        || origin_matches_address_literal_host(origin_header, host_header)
}

fn origin_matches_address_literal_host(
    origin_header: Option<&str>,
    host_header: Option<&str>,
) -> bool {
    let (Some(origin), Some(host)) = (origin_header, host_header) else {
        return false;
    };
    let host = host.trim().to_ascii_lowercase();
    if !is_address_literal_authority(&host) {
        return false;
    }
    let origin = origin.trim().to_ascii_lowercase();
    SCHEMES.into_iter().any(|scheme| {
        origin
            .strip_prefix(scheme)
            .and_then(|rest| rest.strip_prefix(SCHEME_SEPARATOR))
            == Some(host.as_str())
    })
}

fn is_address_literal_authority(authority: &str) -> bool {
    match authority_host(authority) {
        Some(host) => host == LOCALHOST || host.parse::<IpAddr>().is_ok(),
        None => false,
    }
}

fn authority_host(authority: &str) -> Option<&str> {
    match authority.strip_prefix(IPV6_LITERAL_OPEN) {
        Some(rest) => {
            let (inside, after) = rest.split_once(IPV6_LITERAL_CLOSE)?;
            (after.is_empty() || after.starts_with(PORT_SEPARATOR)).then_some(inside)
        }
        None => Some(
            authority
                .split_once(PORT_SEPARATOR)
                .map_or(authority, |(host, _)| host),
        ),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn bind(raw: &str) -> SocketAddr {
        raw.parse().expect("socket addr")
    }

    fn six_loopback_origins(port: u16) -> HashSet<String> {
        HashSet::from([
            format!("http://127.0.0.1:{port}"),
            format!("http://localhost:{port}"),
            format!("http://[::1]:{port}"),
            format!("https://127.0.0.1:{port}"),
            format!("https://localhost:{port}"),
            format!("https://[::1]:{port}"),
        ])
    }

    fn allowlist_for_tests() -> HashSet<String> {
        build_allowed_origins(
            bind("127.0.0.1:8080"),
            &["https://overlay.example.com".to_owned()],
        )
    }

    #[test]
    fn a_loopback_bind_derives_exactly_the_six_scheme_host_origins() {
        for raw in ["127.0.0.1:8080", "127.0.0.5:8080"] {
            assert_eq!(
                build_allowed_origins(bind(raw), &[]),
                six_loopback_origins(8080),
                "bind {raw}"
            );
        }
    }

    #[test]
    fn a_non_loopback_bind_also_allows_the_literal_bound_address() {
        for raw in ["0.0.0.0:8080", "192.0.2.10:8080", "[2001:db8::1]:8080"] {
            let mut expected = six_loopback_origins(8080);
            expected.insert(format!("http://{raw}"));
            expected.insert(format!("https://{raw}"));
            assert_eq!(
                build_allowed_origins(bind(raw), &[]),
                expected,
                "bind {raw}"
            );
        }
    }

    #[test]
    fn extra_origins_supplement_the_derived_set_trimmed_and_lowercased() {
        let extra = [
            "  https://Overlay.Example.COM  ".to_owned(),
            "\tHTTP://Dash.Test:3000\n".to_owned(),
        ];
        let mut expected = six_loopback_origins(8080);
        expected.insert("https://overlay.example.com".to_owned());
        expected.insert("http://dash.test:3000".to_owned());

        assert_eq!(
            build_allowed_origins(bind("127.0.0.1:8080"), &extra),
            expected
        );
    }

    #[test]
    fn blank_extra_origins_leave_the_derived_set_untouched() {
        for extra in [
            Vec::new(),
            vec![String::new()],
            vec!["   ".to_owned(), "\t\n".to_owned()],
        ] {
            assert_eq!(
                build_allowed_origins(bind("127.0.0.1:8080"), &extra),
                six_loopback_origins(8080),
                "extra {extra:?}"
            );
        }
    }

    #[test]
    fn an_absent_origin_header_is_allowed_whatever_the_allowlist_holds() {
        for allowed in [HashSet::new(), allowlist_for_tests()] {
            assert!(is_origin_allowed(&allowed, None));
        }
    }

    #[test]
    fn a_present_origin_on_the_allowlist_is_allowed() {
        let allowed = allowlist_for_tests();
        for origin in [
            "http://127.0.0.1:8080",
            "https://127.0.0.1:8080",
            "http://localhost:8080",
            "http://[::1]:8080",
            "https://overlay.example.com",
            "  HTTPS://Overlay.Example.COM  ",
        ] {
            assert!(
                is_origin_allowed(&allowed, Some(origin)),
                "expected allow for {origin:?}"
            );
        }
    }

    #[test]
    fn a_present_origin_off_the_allowlist_is_rejected() {
        let allowed = allowlist_for_tests();
        for origin in [
            "http://evil.example.com",
            "http://127.0.0.1:8081",
            "ws://127.0.0.1:8080",
            "http://overlay.example.com",
            "http://127.0.0.1:8080/",
            "null",
            "",
        ] {
            assert!(
                !is_origin_allowed(&allowed, Some(origin)),
                "expected reject for {origin:?}"
            );
        }
    }
}
