use axum::http::Method;

use crate::access::auth::AuthState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequestAccess {
    Open,
    Read,
    Write,
}

impl RequestAccess {
    pub(crate) fn of_http_method(method: &Method) -> Self {
        match *method {
            Method::POST | Method::PUT | Method::PATCH | Method::DELETE => Self::Write,
            _ => Self::Read,
        }
    }

    pub(crate) fn needs_credential(self, auth: &AuthState) -> bool {
        match self {
            Self::Open => false,
            Self::Read => auth.reads_required(),
            Self::Write => true,
        }
    }
}
