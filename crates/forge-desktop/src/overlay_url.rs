use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

const URL_SCHEME: &str = "http";
const OVERLAY_ROUTE: &str = "overlays";
const LOOPBACK_HOST: &str = "127.0.0.1";
const WILDCARD_HOSTS: [&str; 3] = ["0.0.0.0", "::", "[::]"];
const EPHEMERAL_PORT: u16 = 0;
const DISCARD_PORT: u16 = 9;
const ROUTE_PROBE_BIND: SocketAddr =
    SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), EPHEMERAL_PORT);
const ROUTE_PROBE_TARGET: SocketAddr =
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), DISCARD_PORT);

pub fn extract_port(bind_address: &str) -> &str {
    bind_address.split(':').next_back().unwrap_or_default()
}

fn bind_host(bind_address: &str) -> &str {
    bind_address
        .rsplit_once(':')
        .map(|(host, _)| host)
        .unwrap_or(bind_address)
}

fn is_wildcard(host: &str) -> bool {
    WILDCARD_HOSTS.contains(&host)
}

pub fn resolve_routable_host(bind_address: &str) -> Option<String> {
    if !is_wildcard(bind_host(bind_address)) {
        return None;
    }
    let socket = UdpSocket::bind(ROUTE_PROBE_BIND).ok()?;
    socket.connect(ROUTE_PROBE_TARGET).ok()?;
    let address = socket.local_addr().ok()?.ip();
    (!address.is_loopback() && !address.is_unspecified()).then(|| address.to_string())
}

pub fn overlay_origin(bind_address: &str, routable_host: Option<&str>) -> String {
    let port = extract_port(bind_address);
    let host = bind_host(bind_address);
    let host = if is_wildcard(host) {
        routable_host.unwrap_or(LOOPBACK_HOST)
    } else {
        host
    };
    format!("{URL_SCHEME}://{host}:{port}")
}

/// The trailing slash is what resolves the directory to its entry document; OBS receives this string verbatim.
pub fn overlay_page_url(origin: &str, identity: &str) -> String {
    format!("{origin}/{OVERLAY_ROUTE}/{identity}/")
}

pub fn overlay_file_url(origin: &str, file_name: &str) -> String {
    format!("{origin}/{OVERLAY_ROUTE}/{file_name}")
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
