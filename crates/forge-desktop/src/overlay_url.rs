pub fn extract_port(bind_address: &str) -> &str {
    bind_address.split(':').next_back().unwrap_or_default()
}

/// Rewrites a wildcard bind to loopback: a browser source needs a host it can dial, and `0.0.0.0` is not one.
pub fn overlay_origin(bind_address: &str) -> String {
    let port = extract_port(bind_address);
    let host = bind_address
        .rsplit_once(':')
        .map(|(host, _)| host)
        .unwrap_or(bind_address);
    let host = match host {
        "0.0.0.0" | "::" | "[::]" => "127.0.0.1",
        other => other,
    };
    format!("http://{host}:{port}")
}

/// The trailing slash is what resolves the directory to its entry document; OBS receives this string verbatim.
pub fn overlay_page_url(origin: &str, identity: &str) -> String {
    format!("{origin}/overlays/{identity}/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_origin_rewrites_a_wildcard_bind_to_a_host_a_browser_can_dial() {
        for (bind, expected) in [
            ("127.0.0.1:9515", "http://127.0.0.1:9515"),
            ("192.168.1.5:9515", "http://192.168.1.5:9515"),
            ("[::1]:9515", "http://[::1]:9515"),
            ("0.0.0.0:9515", "http://127.0.0.1:9515"),
            ("[::]:9515", "http://127.0.0.1:9515"),
        ] {
            assert_eq!(overlay_origin(bind), expected, "bind {bind}");
        }
    }

    #[test]
    fn extract_port_keeps_only_the_port_of_a_bracketed_ipv6_bind() {
        for (bind, expected) in [
            ("127.0.0.1:9515", "9515"),
            ("0.0.0.0:80", "80"),
            ("[::1]:9515", "9515"),
            ("[fe80::1%eth0]:9515", "9515"),
        ] {
            assert_eq!(extract_port(bind), expected, "bind {bind}");
        }
    }

    #[test]
    fn the_copied_url_addresses_the_overlay_directory_with_a_trailing_slash() {
        let url = overlay_page_url(&overlay_origin("0.0.0.0:9515"), "alerts");

        assert_eq!(url, "http://127.0.0.1:9515/overlays/alerts/");
        assert!(
            url.ends_with('/'),
            "without the trailing slash a browser source requests a file, not the entry document: {url}"
        );
    }
}
