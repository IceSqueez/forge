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
