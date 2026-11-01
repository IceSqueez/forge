use forge_platform_core::PlatformError;

pub(crate) fn redact_platform_error(e: &PlatformError) -> String {
    match e {
        PlatformError::Http { status, .. } => format!("HTTP {status}"),
        PlatformError::Network { reason } => reason.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY_SENTINEL: &str = "ya29.a0-yt-body-sentinel";

    #[test]
    fn http_redaction_keeps_the_status_and_never_renders_the_response_body() {
        let cases = [
            (
                403,
                format!(
                    r#"{{"error":{{"message":"insufficientPermissions","token":"{BODY_SENTINEL}"}}}}"#
                ),
            ),
            (500, BODY_SENTINEL.to_owned()),
            (404, String::new()),
        ];
        for (status, body) in cases {
            let rendered = redact_platform_error(&PlatformError::Http { status, body });
            assert!(
                rendered.contains(&status.to_string()),
                "status {status} must survive redaction: {rendered:?}"
            );
            assert!(
                !rendered.contains(BODY_SENTINEL),
                "response body leaked for status {status}: {rendered:?}"
            );
        }
    }

    #[test]
    fn redaction_preserves_diagnostic_text_for_variants_that_carry_no_response_body() {
        let cases = [
            (
                PlatformError::Network {
                    reason: "error sending request".to_owned(),
                },
                "error sending request",
            ),
            (
                PlatformError::Auth {
                    reason: "channel lookup scope missing".to_owned(),
                },
                "channel lookup scope missing",
            ),
            (
                PlatformError::ReauthRequired {
                    platform: "youtube".to_owned(),
                },
                "youtube",
            ),
            (
                PlatformError::Unsupported {
                    feature: "chat.send".to_owned(),
                },
                "chat.send",
            ),
        ];
        for (error, needle) in cases {
            let rendered = redact_platform_error(&error);
            assert!(
                rendered.contains(needle),
                "{needle:?} must survive redaction of {error:?}, got {rendered:?}"
            );
        }
    }
}
