use std::collections::HashSet;
use std::net::SocketAddr;

const SCHEMES: [&str; 2] = ["http", "https"];

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
    if !bind_addr.ip().is_loopback() {
        for scheme in SCHEMES {
            origins.insert(format!("{scheme}://{bind_addr}"));
        }
    }
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
