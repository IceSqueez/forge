use reqwest::Url;
use thiserror::Error;

use super::failure::{GatewayFailure, InvalidRequest};
use super::profile::GatewayProfile;
use crate::address_guard::{HostRejection, ensure_public_ip_literal, is_loopback_host};

const HTTPS: &str = "https";
const HTTP: &str = "http";
const PATH_SEPARATOR: char = '/';
const QUERY_MARK: char = '?';
const FRAGMENT_MARK: char = '#';

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GatewayBuildError {
    #[error("base URL is not an absolute URL")]
    MalformedBase,
    #[error("base URL scheme must be https, or http on a loopback host")]
    InsecureBase,
    #[error("base URL carries a username or password")]
    EmbeddedCredentials,
    #[error("base URL carries a query or fragment")]
    QueryOrFragment,
    #[error("base URL refused by the address guard: {0}")]
    AddressRefused(HostRejection),
    #[error("HTTP client could not be built: {0}")]
    ClientUnavailable(String),
}

pub(crate) fn validate_base(raw: &str, profile: &GatewayProfile) -> Result<Url, GatewayBuildError> {
    let url = Url::parse(raw).map_err(|_| GatewayBuildError::MalformedBase)?;
    let host = url.host_str().ok_or(GatewayBuildError::MalformedBase)?;
    let secure = match url.scheme() {
        HTTPS => true,
        HTTP => is_loopback_host(host),
        _ => false,
    };
    if !secure {
        return Err(GatewayBuildError::InsecureBase);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(GatewayBuildError::EmbeddedCredentials);
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(GatewayBuildError::QueryOrFragment);
    }
    if profile.guards_addresses() {
        ensure_public_ip_literal(host).map_err(GatewayBuildError::AddressRefused)?;
    }
    Ok(url)
}

pub(crate) fn resolve_path(base: &Url, path: &str) -> Result<Url, GatewayFailure> {
    if path.contains([QUERY_MARK, FRAGMENT_MARK]) {
        return Err(GatewayFailure::InvalidRequest(InvalidRequest::QueryInPath));
    }
    let joined = format!(
        "{}{PATH_SEPARATOR}{}",
        base.as_str().trim_end_matches(PATH_SEPARATOR),
        path.trim_start_matches(PATH_SEPARATOR)
    );
    let url = Url::parse(&joined)
        .map_err(|_| GatewayFailure::InvalidRequest(InvalidRequest::MalformedUrl))?;
    if url.origin() != base.origin() {
        return Err(GatewayFailure::InvalidRequest(InvalidRequest::MalformedUrl));
    }
    Ok(url)
}

pub(crate) fn resolve_absolute(raw: &str, profile: &GatewayProfile) -> Result<Url, GatewayFailure> {
    if !profile.accepts_absolute_urls() {
        return Err(GatewayFailure::InvalidRequest(
            InvalidRequest::AbsoluteUrlNotAllowed,
        ));
    }
    let mut url = Url::parse(raw)
        .map_err(|_| GatewayFailure::InvalidRequest(InvalidRequest::MalformedUrl))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(GatewayFailure::InvalidRequest(
            InvalidRequest::EmbeddedCredentials,
        ));
    }
    url.set_fragment(None);
    check_hop(&url, profile)?;
    Ok(url)
}

pub(crate) fn check_hop(url: &Url, profile: &GatewayProfile) -> Result<(), GatewayFailure> {
    let host = url
        .host_str()
        .ok_or(GatewayFailure::InvalidRequest(InvalidRequest::MalformedUrl))?;
    let scheme_allowed = match url.scheme() {
        HTTPS => true,
        HTTP => profile.accepts_absolute_urls() || is_loopback_host(host),
        _ => false,
    };
    if !scheme_allowed {
        return Err(GatewayFailure::InvalidRequest(
            InvalidRequest::UnsupportedScheme,
        ));
    }
    if profile.guards_addresses() {
        ensure_public_ip_literal(host).map_err(GatewayFailure::AddressRefused)?;
    }
    Ok(())
}
