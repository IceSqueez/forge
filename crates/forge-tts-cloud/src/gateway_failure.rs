use forge_net::GatewayFailure;

#[cfg(any(feature = "elevenlabs", feature = "openai"))]
pub(crate) const PAYMENT_REQUIRED: u16 = 402;
const TOO_MANY_REQUESTS: u16 = 429;

pub(crate) fn quota_excerpt<'a>(failure: &'a GatewayFailure, markers: &[&str]) -> Option<&'a str> {
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

pub(crate) fn rejected_excerpt(failure: &GatewayFailure, status: u16) -> Option<&str> {
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
