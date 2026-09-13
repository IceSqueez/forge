use forge_platform_core::EndpointSurface;

const SERVER_START_FAILURE: &str = "forge-desktop: server failed to start";
const BIND_FAILURE: &str = "could not bind to";
const ERROR_LEVEL: &str = "ERROR";

/// forge keeps running with its server off, so this stderr line is the only trace of a lost port race.
pub(crate) fn is_server_bind_failure(line: &str) -> bool {
    line.starts_with(SERVER_START_FAILURE) && line.contains(BIND_FAILURE)
}

/// Keys on the surface variable names forge itself defines, not on the message wording.
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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn server_bind_failure_is_recognised_only_on_the_server_start_line() {
        for (line, expected) in [
            (
                "forge-desktop: server failed to start, leaving it off: could not bind to 127.0.0.1:40123: Address already in use (os error 98)",
                true,
            ),
            (
                "forge-desktop: server failed to start, leaving it off: storage error: locked",
                false,
            ),
            (
                "2026-09-13T10:00:00Z  WARN obs: could not bind to 127.0.0.1:4455",
                false,
            ),
            ("", false),
        ] {
            assert_eq!(is_server_bind_failure(line), expected, "{line:?}");
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
