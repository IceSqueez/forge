pub(crate) const DEFAULT_VTS_HOST: &str = "127.0.0.1";
pub(crate) const DEFAULT_VTS_PORT: u16 = 8001;

const PLAIN_SCHEME: &str = "ws://";
const SECURE_SCHEME: &str = "wss://";

pub(crate) fn endpoint_url(host: &str, port: u16) -> String {
    endpoint_url_with(host, port, false)
}

pub(crate) fn endpoint_url_with(host: &str, port: u16, secure: bool) -> String {
    let scheme = if secure { SECURE_SCHEME } else { PLAIN_SCHEME };
    format!("{scheme}{host}:{port}/")
}

pub(crate) fn is_secure_endpoint(endpoint: &str) -> bool {
    endpoint.starts_with(SECURE_SCHEME)
}

pub(crate) fn split_endpoint(endpoint: &str) -> (String, u16) {
    let without_scheme = endpoint
        .strip_prefix(PLAIN_SCHEME)
        .or_else(|| endpoint.strip_prefix(SECURE_SCHEME))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_endpoint_recovers_the_host_and_port_that_get_persisted() {
        for (endpoint, expected) in [
            ("ws://127.0.0.1:8001/", ("127.0.0.1", 8001)),
            ("wss://vts.local:9123", ("vts.local", 9123)),
            ("ws://vts.local/", ("vts.local", 8001)),
            ("ws://[::1]:8001/", ("[::1]", 8001)),
            ("ws://vts.local:65535/", ("vts.local", 65535)),
            ("vts.local:9000?x=1", ("vts.local", 9000)),
        ] {
            let (host, port) = split_endpoint(endpoint);

            assert_eq!((host.as_str(), port), expected, "endpoint {endpoint}");
        }
    }

    #[test]
    fn a_split_endpoint_rebuilds_into_the_same_url() {
        let url = endpoint_url("vts.local", 9123);

        let (host, port) = split_endpoint(&url);

        assert_eq!(endpoint_url(&host, port), url);
    }
}
