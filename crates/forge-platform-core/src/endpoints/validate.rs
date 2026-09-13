use std::ffi::OsStr;
use std::fmt;
use std::net::IpAddr;

use reqwest::Url;

use super::surface::EndpointProtocol;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointRefusal {
    NotUnicode,
    Malformed,
    SchemeMismatch,
    EmbeddedCredentials,
    QueryOrFragment,
    NotLoopback,
}

impl fmt::Display for EndpointRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotUnicode => "value is not valid unicode",
            Self::Malformed => "value is not an absolute URL",
            Self::SchemeMismatch => "scheme does not match the surface protocol",
            Self::EmbeddedCredentials => "URL carries a username or password",
            Self::QueryOrFragment => "URL carries a query or fragment",
            Self::NotLoopback => "host is not 127.0.0.0/8, ::1 or localhost",
        })
    }
}

/// Yields the parsed URL's own serialization, so the string later dialed is the one validated.
pub(crate) fn validate_override(
    protocol: EndpointProtocol,
    raw: &OsStr,
) -> Result<String, EndpointRefusal> {
    let raw = raw.to_str().ok_or(EndpointRefusal::NotUnicode)?;
    let url = Url::parse(raw).map_err(|_| EndpointRefusal::Malformed)?;
    if !protocol.accepts_scheme(url.scheme()) {
        return Err(EndpointRefusal::SchemeMismatch);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(EndpointRefusal::EmbeddedCredentials);
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(EndpointRefusal::QueryOrFragment);
    }
    if !url.host_str().is_some_and(is_loopback_host) {
        return Err(EndpointRefusal::NotLoopback);
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let bare = host
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(host);
    bare.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}
