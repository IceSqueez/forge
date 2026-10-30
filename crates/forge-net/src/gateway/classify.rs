use std::time::{Duration, SystemTime};

use reqwest::StatusCode;

use super::failure::{BodyExcerpt, GatewayFailure};
use super::profile::GatewayProfile;
use crate::cooldown::MAX_REMOTE_COOLDOWN;
use crate::throttle_hint::{ThrottleHintFamily, ThrottleScope, parse_throttle_hint};

pub(crate) const ERROR_BODY_READ_CAP: u64 = 64 * 1024;
const EXCERPT_MAX_CHARS: usize = 512;
const MASK: &str = "***";

#[derive(Debug, Clone, Copy)]
pub(crate) struct Throttle {
    pub(crate) wait: Duration,
    pub(crate) scope: ThrottleScope,
}

pub(crate) fn throttle_for(
    status: StatusCode,
    headers: &[(String, String)],
    profile: &GatewayProfile,
    now: SystemTime,
) -> Option<Throttle> {
    match status {
        StatusCode::TOO_MANY_REQUESTS => {
            let hint = parse_throttle_hint(headers, profile.hint_family, now);
            Some(Throttle {
                wait: hint.wait_or(profile.no_hint_default),
                scope: hint.scope,
            })
        }
        StatusCode::SERVICE_UNAVAILABLE => {
            let hint = parse_throttle_hint(headers, ThrottleHintFamily::RetryAfter, now);
            hint.wait.map(|wait| Throttle {
                wait: wait.min(MAX_REMOTE_COOLDOWN),
                scope: hint.scope,
            })
        }
        _ => None,
    }
}

pub(crate) fn status_failure(status: StatusCode, excerpt: BodyExcerpt) -> GatewayFailure {
    match status {
        StatusCode::UNAUTHORIZED => GatewayFailure::Unauthorized { excerpt },
        StatusCode::FORBIDDEN => GatewayFailure::Forbidden { excerpt },
        StatusCode::NOT_FOUND => GatewayFailure::NotFound { excerpt },
        status if status.is_client_error() => GatewayFailure::ClientRejected {
            status: status.as_u16(),
            excerpt,
        },
        status if status.is_server_error() => GatewayFailure::ServerError {
            status: status.as_u16(),
            excerpt,
        },
        status => GatewayFailure::UnexpectedStatus {
            status: status.as_u16(),
            excerpt,
        },
    }
}

pub(crate) fn excerpt_of(body: &[u8], secrets: &[&str]) -> BodyExcerpt {
    let mut text = String::from_utf8_lossy(body).into_owned();
    let mut longest_first = secrets.to_vec();
    longest_first.sort_by_key(|secret| std::cmp::Reverse(secret.len()));
    for secret in longest_first {
        if text.contains(secret) {
            text = text.replace(secret, MASK);
        }
    }
    let text = text.trim();
    let cut = text
        .char_indices()
        .nth(EXCERPT_MAX_CHARS)
        .map_or(text.len(), |(index, _)| index);
    BodyExcerpt::new(text.get(..cut).unwrap_or_default().to_owned())
}
