use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};

const SCHEMES: [&str; 2] = ["http", "https"];
const SCHEME_SEPARATOR: &str = "://";
const PORT_SEPARATOR: char = ':';
const IPV6_LITERAL_OPEN: char = '[';
const IPV6_LITERAL_CLOSE: char = ']';
const LOCALHOST: &str = "localhost";
const BEYOND_AUTHORITY_MARKERS: [char; 4] = ['/', '?', '#', '@'];

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

/// A rebinding page reaches us under a name it controls, so a name is trusted only when it is
/// `localhost` or the host of an allowed origin; any address literal is accepted.
pub(crate) fn accepts_host(allowed_origins: &HashSet<String>, host_header: &str) -> bool {
    let authority = host_header.trim().to_ascii_lowercase();
    if is_address_literal_authority(&authority) {
        return true;
    }
    let Some(name) = authority_host(&authority) else {
        return false;
    };
    allowed_origins
        .iter()
        .filter_map(|origin| origin_authority(origin))
        .filter_map(authority_host)
        .any(|allowed| allowed == name)
}

fn origin_authority(origin: &str) -> Option<&str> {
    SCHEMES.into_iter().find_map(|scheme| {
        origin
            .strip_prefix(scheme)
            .and_then(|rest| rest.strip_prefix(SCHEME_SEPARATOR))
    })
}

pub(crate) fn is_well_formed_origin(value: &str) -> bool {
    let trimmed = value.trim().to_ascii_lowercase();
    SCHEMES.into_iter().any(|scheme| {
        trimmed
            .strip_prefix(scheme)
            .and_then(|rest| rest.strip_prefix(SCHEME_SEPARATOR))
            .is_some_and(|authority| {
                !authority.is_empty() && !authority.contains(BEYOND_AUTHORITY_MARKERS)
            })
    })
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
            has_valid_optional_port(after).then_some(inside)
        }
        None => match authority.split_once(PORT_SEPARATOR) {
            Some((host, port)) => is_valid_port(port).then_some(host),
            None => Some(authority),
        },
    }
}

fn has_valid_optional_port(after: &str) -> bool {
    after.is_empty()
        || after
            .strip_prefix(PORT_SEPARATOR)
            .is_some_and(is_valid_port)
}

fn is_valid_port(port: &str) -> bool {
    port.bytes().all(|byte| byte.is_ascii_digit()) && port.parse::<u16>().is_ok()
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
        build_allowed_origins(bind(ALLOWLIST_BIND), &[EXTRA_ORIGIN.to_owned()])
    }

    fn accepts(origin: &str, host: &str) -> bool {
        accepts_origin(&allowlist_for_tests(), Some(origin), Some(host))
    }

    const ALLOWLIST_BIND: &str = "127.0.0.1:8080";
    const ALLOWLIST_PORT: u16 = 8080;
    const EXTRA_ORIGIN: &str = "https://overlay.example.com";
    const LAN_AUTHORITY: &str = "192.168.1.4:8081";

    #[test]
    fn a_bind_derives_exactly_the_six_loopback_origins_whatever_address_it_holds() {
        for raw in [
            ALLOWLIST_BIND,
            "127.0.0.5:8080",
            "0.0.0.0:8080",
            "192.0.2.10:8080",
            "[2001:db8::1]:8080",
        ] {
            assert_eq!(
                build_allowed_origins(bind(raw), &[]),
                six_loopback_origins(ALLOWLIST_PORT),
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

    #[test]
    fn a_page_dialled_at_an_address_literal_authority_is_accepted_as_same_origin() {
        for (origin, host) in [
            ("http://192.168.1.4:8081", LAN_AUTHORITY),
            ("https://192.168.1.4:8081", LAN_AUTHORITY),
            ("http://[2001:db8::4]:8081", "[2001:db8::4]:8081"),
            (
                "http://[::ffff:192.168.1.4]:8081",
                "[::ffff:192.168.1.4]:8081",
            ),
            ("http://10.0.0.5:9000", "10.0.0.5:9000"),
            ("http://localhost:9000", "localhost:9000"),
            ("http://192.168.1.4", "192.168.1.4"),
            ("http://192.168.1.4:0", "192.168.1.4:0"),
            ("http://192.168.1.4:65535", "192.168.1.4:65535"),
        ] {
            assert!(
                accepts(origin, host),
                "expected accept for origin {origin:?} host {host:?}"
            );
        }
    }

    #[test]
    fn a_host_that_is_a_name_never_takes_the_same_origin_shortcut() {
        for authority in [
            "evil.example:8081",
            "forge.local:8081",
            "1.2.3.4.evil:8081",
            "192.168.1.4.evil.example:8081",
            "192.168.1.4.:8081",
            "localhost.evil.example:8081",
        ] {
            let origin = format!("http://{authority}");
            assert!(
                !accepts(&origin, authority),
                "a self-consistent name authority {authority:?} is the DNS-rebinding shape"
            );
        }
    }

    #[test]
    fn an_origin_whose_authority_is_not_byte_equal_to_the_host_is_rejected() {
        for origin in [
            "https://evil.example",
            "http://evil.example:8081",
            "http://192.168.1.4:9999",
            "http://192.168.1.5:8081",
            "http://192.168.1.4",
            "http://192.168.1.4:8081/",
            "http://192.168.1.4:8081/overlays/alerts",
            "http://192.168.1.4:8081?q=1",
            "http://user@192.168.1.4:8081",
            "http://user:pass@192.168.1.4:8081",
            "ws://192.168.1.4:8081",
            "file://192.168.1.4:8081",
            "//192.168.1.4:8081",
            "192.168.1.4:8081",
            "null",
            "",
        ] {
            assert!(
                !accepts(origin, LAN_AUTHORITY),
                "expected reject for origin {origin:?} against host {LAN_AUTHORITY:?}"
            );
        }
    }

    #[test]
    fn a_host_that_is_not_a_usable_address_literal_takes_no_shortcut() {
        for host in [
            "2001:db8::4",
            "::1",
            "[fe80::1%eth0]:8081",
            "[fe80::1%25eth0]:8081",
            "[2001:db8::4]x:8081",
            "[2001:db8::4",
            "[]:8081",
            "010.0.0.1:8081",
            "3232235780:8081",
            "0xc0a80104:8081",
            "",
        ] {
            let origin = format!("http://{host}");
            assert!(
                !accepts(&origin, host),
                "host {host:?} must not pass the address-literal gate"
            );
        }
    }

    #[test]
    fn a_port_segment_that_is_not_a_plain_u16_takes_no_shortcut() {
        for host in [
            "192.168.1.4:8081.evil.example",
            "localhost:8081.evil.example",
            "192.168.1.4:8081@evil",
            "192.168.1.4:",
            "192.168.1.4:-1",
            "192.168.1.4:+8081",
            "192.168.1.4:65536",
            "192.168.1.4:80 81",
            "192.168.1.4:8081\u{660}",
            "localhost:x",
            "[2001:db8::4]:80x",
            "[2001:db8::4]:",
            "[2001:db8::4]:65536",
        ] {
            let origin = format!("http://{host}");
            assert!(
                !accepts(&origin, host),
                "host {host:?} carries no dialable port and must not reach the shortcut"
            );
        }
    }

    #[test]
    fn a_bare_scheme_and_authority_is_the_only_shape_worth_logging_verbatim() {
        for well_formed in [
            "http://192.168.1.4:8081",
            "https://overlay.example.com",
            "http://localhost",
            "http://[::1]:9515",
            "HTTP://Overlay.Example.COM",
            "  https://dash.test:3000  ",
        ] {
            assert!(
                is_well_formed_origin(well_formed),
                "expected well formed: {well_formed:?}"
            );
        }

        for malformed in [
            "http://192.168.1.4:8081/overlays/alerts",
            "http://192.168.1.4:8081/",
            "http://192.168.1.4:8081?q=1",
            "http://192.168.1.4:8081#frag",
            "http://user:pass@192.168.1.4:8081",
            "http://",
            "https://",
            "ws://192.168.1.4:8081",
            "file://192.168.1.4:8081",
            "//192.168.1.4:8081",
            "https:/dash.test",
            "httpx://dash.test",
            "null",
            "",
        ] {
            assert!(
                !is_well_formed_origin(malformed),
                "expected malformed: {malformed:?}"
            );
        }
    }

    #[test]
    fn the_shortcut_needs_a_host_while_an_absent_origin_stays_accepted() {
        let allowed = allowlist_for_tests();

        assert!(!accepts_origin(
            &allowed,
            Some("http://192.168.1.4:8081"),
            None
        ));
        assert!(accepts_origin(&allowed, None, Some(LAN_AUTHORITY)));
        assert!(accepts_origin(&allowed, None, None));
    }

    #[test]
    fn letter_case_and_surrounding_whitespace_do_not_change_the_same_origin_decision() {
        assert!(accepts(
            "  HTTP://192.168.1.4:8081  ",
            "\t192.168.1.4:8081\n"
        ));
        assert!(accepts("HTTPS://[2001:DB8::4]:8081", "[2001:db8::4]:8081"));
    }

    #[test]
    fn a_configured_additional_origin_is_accepted_although_its_host_is_a_name() {
        assert!(accepts(EXTRA_ORIGIN, "overlay.example.com"));
    }

    #[test]
    fn a_host_that_a_rebinding_page_cannot_present_is_accepted() {
        let allowed = allowlist_for_tests();
        for host in [
            "127.0.0.1:8080",
            "127.0.0.1",
            "192.168.1.4:65535",
            "[::1]:8080",
            "[::1]",
            "[2001:db8::4]:0",
            "localhost",
            "localhost:0",
            "LOCALHOST:8080",
            "  localhost:8080\t",
            "overlay.example.com",
            "overlay.example.com:8443",
            "Overlay.Example.COM:443",
        ] {
            assert!(accepts_host(&allowed, host), "expected accept for {host:?}");
        }
    }

    #[test]
    fn a_host_naming_anything_but_localhost_or_an_allowed_origin_is_refused() {
        let allowed = allowlist_for_tests();
        for host in [
            "evil.example:8080",
            "evil.example",
            "localhost.evil.example:8080",
            "overlay.example.com.evil:8080",
            "evil.overlay.example.com",
            "overlay.example.com:abc",
            "overlay.example.com:",
            "localhost:",
            "localhost:65536",
            "localhost:+80",
            "127.0.0.1:65536",
            "[::1",
            "[fe80::1%25eth0]:8080",
            "https://overlay.example.com",
            "",
        ] {
            assert!(
                !accepts_host(&allowed, host),
                "expected refusal for {host:?}"
            );
        }
    }
}
