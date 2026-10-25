use forge_platform_core::EndpointSurface;

const SERVER_START_FAILURE_MESSAGE: &str = "server failed to start";
const ERROR_FIELD: &str = "error";
const BIND_FAILURE_REASON: &str = "could not bind to";
const ERROR_LEVEL: &str = "ERROR";

pub(crate) fn is_server_bind_failure(line: &str) -> bool {
    let Some((_timestamp, after_timestamp)) = line.split_once(' ') else {
        return false;
    };
    let Some(body) = after_timestamp
        .trim_start()
        .strip_prefix(ERROR_LEVEL)
        .and_then(|rest| rest.strip_prefix(' '))
    else {
        return false;
    };
    body.starts_with(SERVER_START_FAILURE_MESSAGE)
        && body.contains(&format!(" {ERROR_FIELD}={BIND_FAILURE_REASON} "))
}

pub(crate) fn is_endpoint_override_refusal(line: &str) -> bool {
    line.contains(ERROR_LEVEL)
        && EndpointSurface::ALL.iter().any(|surface| {
            line.contains(&format!("endpoint override {} refused", surface.env_var()))
        })
}

pub(crate) fn strip_ansi(line: &str) -> String {
    let mut plain = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for terminator in chars.by_ref() {
                if ('\u{40}'..='\u{7e}').contains(&terminator) {
                    break;
                }
            }
            continue;
        }
        plain.push(c);
    }
    plain
}

#[cfg(test)]
mod tests {
    use forge_server::ServerError;

    use super::*;

    const CAPTURED_BIND_FAILURE: &str = "2026-10-24T15:22:05.467435Z ERROR server failed to start, leaving it off error=could not bind to 127.0.0.1:39555: Address already in use (os error 98)";

    fn server_start_failure(level: &str, message: &str, error: &ServerError) -> String {
        format!("2026-10-24T15:22:05.467435Z {level:>5} {message} error={error}")
    }

    fn bind_error() -> ServerError {
        ServerError::Bind {
            addr: "127.0.0.1:40123".to_owned(),
            reason: "Address already in use (os error 98)".to_owned(),
        }
    }

    #[test]
    fn captured_console_bind_failure_is_recognised() {
        assert!(is_server_bind_failure(CAPTURED_BIND_FAILURE));
    }

    #[test]
    fn server_bind_failure_follows_the_server_crate_bind_error_text() {
        let line = server_start_failure(
            "ERROR",
            "server failed to start, leaving it off",
            &bind_error(),
        );
        assert!(is_server_bind_failure(&line), "{line:?}");
    }

    #[test]
    fn server_bind_failure_needs_the_error_level_the_start_message_and_a_bind_error() {
        let lan_refusal = ServerError::LanBindNotEnabled {
            addr: "0.0.0.0:8080".to_owned(),
        };
        for line in [
            server_start_failure("WARN", "server failed to start, leaving it off", &bind_error()),
            server_start_failure("ERROR", "server state unavailable, leaving it off", &bind_error()),
            server_start_failure("ERROR", "server failed to start, leaving it off", &lan_refusal),
            "forge-desktop: server failed to start, leaving it off: could not bind to 127.0.0.1:1: in use".to_owned(),
            "2026-10-24T15:22:05Z ERRORserver failed to start error=could not bind to 127.0.0.1:1: in use".to_owned(),
            String::new(),
        ] {
            assert!(!is_server_bind_failure(&line), "{line:?}");
        }
    }

    #[test]
    fn endpoint_override_refusal_needs_an_error_level_and_a_known_surface() {
        for (line, expected) in [
            (
                "2026-09-13T10:00:00.1Z ERROR platform endpoint override refused; exiting error=endpoint override FORGE_TWITCH_EVENTSUB_WS_URL refused: host is not 127.0.0.0/8, ::1 or localhost",
                true,
            ),
            (
                "ERROR x error=endpoint override FORGE_YOUTUBE_UPLOAD_BASE_URL refused: value is not an absolute URL",
                true,
            ),
            (
                " WARN x error=endpoint override FORGE_TWITCH_API_BASE_URL refused: malformed",
                false,
            ),
            (
                "ERROR x error=endpoint override FORGE_UNKNOWN_URL refused: malformed",
                false,
            ),
            (
                "ERROR another forge instance is already running for this data directory; exiting",
                false,
            ),
        ] {
            assert_eq!(is_endpoint_override_refusal(line), expected, "{line:?}");
        }
    }

    #[test]
    fn ansi_colour_sequences_are_removed_and_other_text_kept() {
        for (raw, plain) in [
            (
                "\u{1b}[2m2026\u{1b}[0m \u{1b}[31mERROR\u{1b}[0m done",
                "2026 ERROR done",
            ),
            ("no escapes, ünïcode", "no escapes, ünïcode"),
            ("lone \u{1b} escape", "lone \u{1b} escape"),
            ("truncated \u{1b}[31", "truncated "),
        ] {
            assert_eq!(strip_ansi(raw), plain, "{raw:?}");
        }
    }
}
