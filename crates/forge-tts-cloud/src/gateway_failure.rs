use forge_net::GatewayFailure;
use forge_speech_types::VoiceId;

use crate::network::ProviderLink;

const PAYMENT_REQUIRED: u16 = 402;
const REQUEST_TIMEOUT: u16 = 408;
const BAD_REQUEST: u16 = 400;
const TOO_MANY_REQUESTS: u16 = 429;
const VOICE_REJECTION_MARKER: &str = "voice";

#[derive(Clone, Copy)]
pub(crate) enum VoiceSignal {
    Unreported,
    NotFoundStatus,
    BadRequestNamingVoice,
}

#[derive(Clone, Copy)]
pub(crate) struct ProviderPolicy {
    pub quota_markers: &'static [&'static str],
    pub payment_required_is_quota: bool,
    pub voice_signal: VoiceSignal,
}

pub(crate) enum Classified {
    Quota(String),
    Throttled(u64),
    Auth,
    VoiceMissing(VoiceId),
    Rejected(String),
    Transient(String),
}

pub(crate) fn classify(
    failure: &GatewayFailure,
    link: &ProviderLink,
    policy: ProviderPolicy,
    voice: Option<&VoiceId>,
    detail: fn(&GatewayFailure) -> String,
) -> Classified {
    if let Some(excerpt) = quota_excerpt(failure, policy.quota_markers) {
        return Classified::Quota(excerpt.to_owned());
    }
    if let Some(retry_after_secs) = link.throttle_secs(failure) {
        return Classified::Throttled(retry_after_secs);
    }
    if policy.payment_required_is_quota
        && let Some(excerpt) = rejected_excerpt(failure, PAYMENT_REQUIRED)
    {
        return Classified::Quota(excerpt.to_owned());
    }
    if let Some(voice) = voice
        && names_missing_voice(failure, policy.voice_signal)
    {
        return Classified::VoiceMissing(voice.clone());
    }
    match failure {
        GatewayFailure::Unauthorized { .. } | GatewayFailure::Forbidden { .. } => Classified::Auth,
        GatewayFailure::ServerError { .. } | GatewayFailure::Transport(_) => {
            Classified::Transient(detail(failure))
        }
        GatewayFailure::ClientRejected {
            status: REQUEST_TIMEOUT,
            ..
        } => Classified::Transient(detail(failure)),
        other => Classified::Rejected(detail(other)),
    }
}

fn names_missing_voice(failure: &GatewayFailure, signal: VoiceSignal) -> bool {
    match signal {
        VoiceSignal::Unreported => false,
        VoiceSignal::NotFoundStatus => matches!(failure, GatewayFailure::NotFound { .. }),
        VoiceSignal::BadRequestNamingVoice => rejected_excerpt(failure, BAD_REQUEST)
            .is_some_and(|excerpt| contains_any_ignoring_case(excerpt, &[VOICE_REJECTION_MARKER])),
    }
}

fn quota_excerpt<'a>(failure: &'a GatewayFailure, markers: &[&str]) -> Option<&'a str> {
    match failure {
        GatewayFailure::RateLimited {
            status: TOO_MANY_REQUESTS,
            excerpt,
            ..
        } if contains_any_ignoring_case(excerpt.as_str(), markers) => Some(excerpt.as_str()),
        _ => None,
    }
}

pub(crate) fn failure_detail(failure: &GatewayFailure) -> String {
    match (failure.status(), failure.excerpt()) {
        (Some(status), Some(excerpt)) => format!("HTTP {status}: {}", excerpt.as_str()),
        _ => failure.to_string(),
    }
}

pub(crate) fn probe_failure_detail(failure: &GatewayFailure) -> String {
    match failure.status() {
        Some(status) => format!("unexpected status {status}"),
        None => failure.to_string(),
    }
}

fn rejected_excerpt(failure: &GatewayFailure, status: u16) -> Option<&str> {
    match failure {
        GatewayFailure::ClientRejected {
            status: rejected,
            excerpt,
        } if *rejected == status => Some(excerpt.as_str()),
        _ => None,
    }
}

fn contains_any_ignoring_case(text: &str, markers: &[&str]) -> bool {
    let text = text.to_ascii_lowercase();
    markers
        .iter()
        .any(|marker| text.contains(&marker.to_ascii_lowercase()))
}
