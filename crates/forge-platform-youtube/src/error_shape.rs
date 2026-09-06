use forge_platform_core::PlatformError;

/// `PlatformError::Http` is reused across this crate for both YouTube Data API errors and
/// the OAuth token endpoint's error body; callers here cannot tell which produced a given
/// instance, so the body is never rendered, only the status. `Network` is already sanitized
/// with `reqwest::Error::without_url()` at every construction site in this crate.
pub(crate) fn redact_platform_error(e: &PlatformError) -> String {
    match e {
        PlatformError::Http { status, .. } => format!("HTTP {status}"),
        PlatformError::Network { reason } => reason.clone(),
        other => other.to_string(),
    }
}
