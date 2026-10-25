use std::error::Error as StdError;
use std::fmt;
use std::time::Duration;

use thiserror::Error;

use crate::address_guard::HostRejection;
use crate::throttle_hint::ThrottleScope;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BodyExcerpt(String);

impl BodyExcerpt {
    pub(crate) fn new(text: String) -> Self {
        Self(text)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportKind {
    Timeout,
    Connect,
    Redirect,
    Other,
}

impl fmt::Display for TransportKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Timeout => "timed out",
            Self::Connect => "could not connect",
            Self::Redirect => "redirect refused",
            Self::Other => "transport error",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("network request failed ({kind}): {cause}")]
pub struct TransportFailure {
    kind: TransportKind,
    cause: String,
}

impl TransportFailure {
    pub(crate) fn new(kind: TransportKind, cause: impl Into<String>) -> Self {
        Self {
            kind,
            cause: cause.into(),
        }
    }

    pub(crate) fn from_reqwest(error: reqwest::Error) -> Self {
        let kind = if error.is_timeout() {
            TransportKind::Timeout
        } else if error.is_connect() {
            TransportKind::Connect
        } else if error.is_redirect() {
            TransportKind::Redirect
        } else {
            TransportKind::Other
        };
        Self::new(kind, url_free_cause(error))
    }

    pub fn kind(&self) -> TransportKind {
        self.kind
    }

    pub fn cause(&self) -> &str {
        &self.cause
    }
}

fn url_free_cause(error: reqwest::Error) -> String {
    let error = error.without_url();
    let mut cause = error.to_string();
    let mut source = StdError::source(&error);
    while let Some(inner) = source {
        if !inner.is::<reqwest::Error>() {
            let text = inner.to_string();
            if !cause.ends_with(&text) {
                cause.push_str(": ");
                cause.push_str(&text);
            }
        }
        source = inner.source();
    }
    cause
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Error)]
pub enum InvalidRequest {
    #[error("absolute URLs are accepted only for user-directed egress")]
    AbsoluteUrlNotAllowed,
    #[error("request target is not a valid URL")]
    MalformedUrl,
    #[error("only http and https targets are allowed")]
    UnsupportedScheme,
    #[error("request target carries a username or password")]
    EmbeddedCredentials,
    #[error("request path carries a query or fragment; use query pairs")]
    QueryInPath,
    #[error("credential headers must be attached as a credential")]
    SensitiveHeader,
    #[error("request header name or value is invalid")]
    InvalidHeader,
    #[error("request content type is invalid")]
    InvalidContentType,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GatewayFailure {
    #[error("request was not authorized (status 401)")]
    Unauthorized { excerpt: BodyExcerpt },
    #[error("request was forbidden (status 403)")]
    Forbidden { excerpt: BodyExcerpt },
    #[error("requested resource was not found (status 404)")]
    NotFound { excerpt: BodyExcerpt },
    #[error("upstream rate limit reached (status {status}); retry after {retry_after:?}")]
    RateLimited {
        status: u16,
        retry_after: Duration,
        scope: ThrottleScope,
        excerpt: BodyExcerpt,
    },
    #[error("request was rejected (status {status})")]
    ClientRejected { status: u16, excerpt: BodyExcerpt },
    #[error("upstream server error (status {status})")]
    ServerError { status: u16, excerpt: BodyExcerpt },
    #[error("unexpected response status {status}")]
    UnexpectedStatus { status: u16, excerpt: BodyExcerpt },
    #[error("local rate-limit budget exhausted")]
    BudgetExhausted { retry_after: Option<Duration> },
    #[error("response body exceeds the {cap}-byte cap")]
    ResponseTooLarge { cap: u64 },
    #[error(transparent)]
    Transport(TransportFailure),
    #[error("request was cancelled")]
    Cancelled,
    #[error("request refused by the address guard: {0}")]
    AddressRefused(HostRejection),
    #[error("invalid request: {0}")]
    InvalidRequest(InvalidRequest),
}

impl GatewayFailure {
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Unauthorized { .. } => Some(reqwest::StatusCode::UNAUTHORIZED.as_u16()),
            Self::Forbidden { .. } => Some(reqwest::StatusCode::FORBIDDEN.as_u16()),
            Self::NotFound { .. } => Some(reqwest::StatusCode::NOT_FOUND.as_u16()),
            Self::RateLimited { status, .. }
            | Self::ClientRejected { status, .. }
            | Self::ServerError { status, .. }
            | Self::UnexpectedStatus { status, .. } => Some(*status),
            Self::BudgetExhausted { .. }
            | Self::ResponseTooLarge { .. }
            | Self::Transport(_)
            | Self::Cancelled
            | Self::AddressRefused(_)
            | Self::InvalidRequest(_) => None,
        }
    }

    pub fn excerpt(&self) -> Option<&BodyExcerpt> {
        match self {
            Self::Unauthorized { excerpt }
            | Self::Forbidden { excerpt }
            | Self::NotFound { excerpt }
            | Self::RateLimited { excerpt, .. }
            | Self::ClientRejected { excerpt, .. }
            | Self::ServerError { excerpt, .. }
            | Self::UnexpectedStatus { excerpt, .. } => Some(excerpt),
            Self::BudgetExhausted { .. }
            | Self::ResponseTooLarge { .. }
            | Self::Transport(_)
            | Self::Cancelled
            | Self::AddressRefused(_)
            | Self::InvalidRequest(_) => None,
        }
    }
}
