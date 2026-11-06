use std::time::{Duration, SystemTime};

use percent_encoding::percent_decode;
use reqwest::StatusCode;
use reqwest::header::HeaderMap;

use super::super::classify::{excerpt_of, status_failure};
use super::super::failure::{BodyExcerpt, GatewayFailure, TransportFailure, TransportKind};
use super::super::profile::GatewayProfile;
use super::details::{StatusDetails, decode_status_details};
use crate::cooldown::MAX_REMOTE_COOLDOWN;
use crate::throttle_hint::{ThrottleScope, parse_throttle_hint};

pub(super) const GRPC_STATUS: &str = "grpc-status";
const GRPC_MESSAGE: &str = "grpc-message";
const GRPC_STATUS_DETAILS: &str = "grpc-status-details-bin";
const CANCELLED_BY_SERVER: &str = "call cancelled by the server";
const DEADLINE_EXCEEDED: &str = "call deadline exceeded";
const SERVICE_UNAVAILABLE: &str = "service unavailable";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum GrpcCode {
    Ok = 0,
    Cancelled = 1,
    Unknown = 2,
    InvalidArgument = 3,
    DeadlineExceeded = 4,
    NotFound = 5,
    AlreadyExists = 6,
    PermissionDenied = 7,
    ResourceExhausted = 8,
    FailedPrecondition = 9,
    Aborted = 10,
    OutOfRange = 11,
    Unimplemented = 12,
    Internal = 13,
    Unavailable = 14,
    DataLoss = 15,
    Unauthenticated = 16,
}

impl GrpcCode {
    const ALL: [Self; 17] = [
        Self::Ok,
        Self::Cancelled,
        Self::Unknown,
        Self::InvalidArgument,
        Self::DeadlineExceeded,
        Self::NotFound,
        Self::AlreadyExists,
        Self::PermissionDenied,
        Self::ResourceExhausted,
        Self::FailedPrecondition,
        Self::Aborted,
        Self::OutOfRange,
        Self::Unimplemented,
        Self::Internal,
        Self::Unavailable,
        Self::DataLoss,
        Self::Unauthenticated,
    ];

    fn from_wire(code: u32) -> Self {
        Self::ALL
            .into_iter()
            .find(|known| *known as u32 == code)
            .unwrap_or(Self::Unknown)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GrpcStatus {
    code: GrpcCode,
    excerpt: BodyExcerpt,
    retry_after: Option<Duration>,
}

impl GrpcStatus {
    pub(super) fn into_outcome(self, profile: &GatewayProfile) -> Result<(), GatewayFailure> {
        let Self {
            code,
            excerpt,
            retry_after,
        } = self;
        match code {
            GrpcCode::Ok => Ok(()),
            GrpcCode::Unimplemented => Err(GatewayFailure::ProtocolNotOffered { excerpt }),
            GrpcCode::ResourceExhausted => Err(GatewayFailure::RateLimited {
                status: StatusCode::TOO_MANY_REQUESTS.as_u16(),
                retry_after: retry_after
                    .unwrap_or(profile.no_hint_default)
                    .min(MAX_REMOTE_COOLDOWN),
                scope: ThrottleScope::Bucket,
                excerpt,
            }),
            GrpcCode::Cancelled => Err(transport(TransportKind::Other, CANCELLED_BY_SERVER)),
            GrpcCode::DeadlineExceeded => Err(transport(TransportKind::Timeout, DEADLINE_EXCEEDED)),
            GrpcCode::Unavailable => Err(transport(TransportKind::Other, SERVICE_UNAVAILABLE)),
            GrpcCode::Unauthenticated => Err(status_failure(StatusCode::UNAUTHORIZED, excerpt)),
            GrpcCode::PermissionDenied => Err(status_failure(StatusCode::FORBIDDEN, excerpt)),
            GrpcCode::NotFound => Err(status_failure(StatusCode::NOT_FOUND, excerpt)),
            GrpcCode::InvalidArgument | GrpcCode::FailedPrecondition | GrpcCode::OutOfRange => {
                Err(status_failure(StatusCode::BAD_REQUEST, excerpt))
            }
            GrpcCode::AlreadyExists | GrpcCode::Aborted => {
                Err(status_failure(StatusCode::CONFLICT, excerpt))
            }
            GrpcCode::Unknown | GrpcCode::Internal | GrpcCode::DataLoss => {
                Err(status_failure(StatusCode::INTERNAL_SERVER_ERROR, excerpt))
            }
        }
    }
}

pub(super) enum StatusField {
    Absent,
    Malformed,
    Present(GrpcStatus),
}

pub(super) fn status_in(
    headers: &HeaderMap,
    secrets: &[&str],
    profile: &GatewayProfile,
) -> StatusField {
    let Some(raw) = headers.get(GRPC_STATUS) else {
        return StatusField::Absent;
    };
    let Some(code) = raw
        .to_str()
        .ok()
        .and_then(|text| text.trim().parse::<u32>().ok())
    else {
        return StatusField::Malformed;
    };
    let code = GrpcCode::from_wire(code);
    if code == GrpcCode::Ok {
        return StatusField::Present(GrpcStatus {
            code,
            excerpt: BodyExcerpt::default(),
            retry_after: None,
        });
    }
    let StatusDetails {
        message: details_message,
        reason,
        retry_delay,
    } = headers
        .get(GRPC_STATUS_DETAILS)
        .map(|value| decode_status_details(value.as_bytes()))
        .unwrap_or_default();
    let message: Vec<u8> = match headers.get(GRPC_MESSAGE) {
        Some(value) => percent_decode(value.as_bytes()).collect(),
        None => details_message.map(String::into_bytes).unwrap_or_default(),
    };
    let reason = reason.filter(|reason| !secrets.iter().any(|secret| reason.contains(secret)));
    let retry_after = match code {
        GrpcCode::ResourceExhausted => retry_delay.or_else(|| header_hint(headers, profile)),
        _ => None,
    };
    StatusField::Present(GrpcStatus {
        code,
        excerpt: excerpt_of(&message, secrets).with_reason(reason),
        retry_after,
    })
}

fn header_hint(headers: &HeaderMap, profile: &GatewayProfile) -> Option<Duration> {
    let pairs: Vec<(&str, &str)> = headers
        .iter()
        .filter_map(|(name, value)| value.to_str().ok().map(|value| (name.as_str(), value)))
        .collect();
    parse_throttle_hint(pairs.as_slice(), profile.hint_family, SystemTime::now()).wait
}

fn transport(kind: TransportKind, cause: &'static str) -> GatewayFailure {
    GatewayFailure::Transport(TransportFailure::new(kind, cause))
}
