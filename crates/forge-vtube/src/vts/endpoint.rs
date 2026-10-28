pub(crate) const DEFAULT_VTS_HOST: &str = "127.0.0.1";
pub(crate) const DEFAULT_VTS_PORT: u16 = 8001;

pub(crate) fn endpoint_url(host: &str, port: u16) -> String {
    format!("ws://{host}:{port}/")
}

pub(crate) fn split_endpoint(endpoint: &str) -> (String, u16) {
    let without_scheme = endpoint
        .strip_prefix("ws://")
        .or_else(|| endpoint.strip_prefix("wss://"))
        .unwrap_or(endpoint);

    let authority = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(without_scheme);

    match authority.rsplit_once(':') {
        Some((host, port_str)) => match port_str.parse::<u16>() {
            Ok(port) => (host.to_owned(), port),
            Err(_) => (authority.to_owned(), DEFAULT_VTS_PORT),
        },
        None => (authority.to_owned(), DEFAULT_VTS_PORT),
    }
}
