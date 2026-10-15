use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

const URL_SCHEME: &str = "http";
pub const OVERLAY_ROUTE: &str = "overlays";
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

pub fn overlay_page_url(origin: &str, identity: &str) -> String {
    format!("{origin}/{OVERLAY_ROUTE}/{identity}/")
}

pub fn overlay_file_url(origin: &str, file_name: &str) -> String {
    format!("{origin}/{OVERLAY_ROUTE}/{file_name}")
}
