use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

const URL_SCHEME: &str = "http";
pub const OVERLAY_ROUTE: &str = "overlays";
const LOOPBACK_HOST: &str = "127.0.0.1";
const EPHEMERAL_PORT: u16 = 0;
const DISCARD_PORT: u16 = 9;
const ROUTE_PROBE_BIND: SocketAddr =
    SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), EPHEMERAL_PORT);
const ROUTE_PROBE_TARGET: SocketAddr =
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), DISCARD_PORT);

pub fn resolve_routable_host(bind: SocketAddr) -> Option<String> {
    if !bind.ip().is_unspecified() {
        return None;
    }
    let socket = UdpSocket::bind(ROUTE_PROBE_BIND).ok()?;
    socket.connect(ROUTE_PROBE_TARGET).ok()?;
    let address = socket.local_addr().ok()?.ip();
    (!address.is_loopback() && !address.is_unspecified()).then(|| address.to_string())
}

pub fn overlay_origin(bind: SocketAddr, routable_host: Option<&str>) -> String {
    if bind.ip().is_unspecified() {
        let host = routable_host.unwrap_or(LOOPBACK_HOST);
        format!("{URL_SCHEME}://{host}:{port}", port = bind.port())
    } else {
        format!("{URL_SCHEME}://{bind}")
    }
}

pub fn overlay_page_url(origin: &str, token: &str, identity: &str) -> String {
    format!("{origin}/{OVERLAY_ROUTE}/{token}/{identity}/")
}

pub fn overlay_file_url(origin: &str, token: &str, file_name: &str) -> String {
    format!("{origin}/{OVERLAY_ROUTE}/{token}/{file_name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROUTABLE_HOST: &str = "192.168.1.5";

    #[test]
    fn overlay_origin_substitutes_the_routable_host_for_a_wildcard_bind_only() {
        for (bind, routable_host, expected) in [
            (
                "0.0.0.0:9515",
                Some(ROUTABLE_HOST),
                "http://192.168.1.5:9515",
            ),
            ("[::]:9515", Some(ROUTABLE_HOST), "http://192.168.1.5:9515"),
            ("0.0.0.0:9515", None, "http://127.0.0.1:9515"),
            ("[::]:9515", None, "http://127.0.0.1:9515"),
            (
                "192.168.1.9:9515",
                Some(ROUTABLE_HOST),
                "http://192.168.1.9:9515",
            ),
            (
                "127.0.0.1:9515",
                Some(ROUTABLE_HOST),
                "http://127.0.0.1:9515",
            ),
            ("[::1]:9515", Some(ROUTABLE_HOST), "http://[::1]:9515"),
        ] {
            assert_eq!(
                overlay_origin(bind.parse().unwrap(), routable_host),
                expected,
                "bind {bind} with routable host {routable_host:?}"
            );
        }
    }

    #[test]
    fn resolve_routable_host_declines_a_bind_that_already_names_a_dialable_host() {
        for bind in [
            "127.0.0.1:9515",
            "192.168.1.9:9515",
            "[::1]:9515",
            "0.0.0.1:9515",
        ] {
            assert!(
                resolve_routable_host(bind.parse().unwrap()).is_none(),
                "bind {bind}"
            );
        }
    }

    #[test]
    fn a_page_url_trails_a_slash_under_the_same_route_a_file_url_serves_without_one() {
        let origin = overlay_origin("0.0.0.0:9515".parse().unwrap(), Some(ROUTABLE_HOST));

        assert_eq!(
            overlay_page_url(&origin, "fgo_t", "alerts"),
            "http://192.168.1.5:9515/overlays/fgo_t/alerts/",
            "without the trailing slash a browser source requests a file, not the entry document"
        );
        assert_eq!(
            overlay_file_url(&origin, "fgo_t", "alerts.html"),
            "http://192.168.1.5:9515/overlays/fgo_t/alerts.html",
            "a trailing slash on a file would redirect a browser source to a directory listing"
        );
    }
}
