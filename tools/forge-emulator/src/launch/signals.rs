use crate::EmulatorError;

const SIGNAL_FIELD: &str = "signal";
const OUTCOME_FIELD: &str = "outcome";
const CAUSE_FIELD: &str = "cause";
const FAILURE_FIELD: &str = "failure";
const VARIABLE_FIELD: &str = "variable";
const ERROR_FIELD: &str = "error";

const SIGNAL_SERVER_BIND: &str = "server_bind";
const SIGNAL_READINESS: &str = "readiness";
const SIGNAL_STARTUP_REFUSED: &str = "startup_refused";

const OUTCOME_FAILED: &str = "failed";
const OUTCOME_DISABLED: &str = "disabled";
const OUTCOME_INVALID_ADDRESS: &str = "invalid_address";
const OUTCOME_SETTINGS_UNAVAILABLE: &str = "settings_unavailable";
const OUTCOME_ENDPOINT_OVERRIDE_REFUSED: &str = "endpoint_override_refused";
const OUTCOME_ALREADY_RUNNING: &str = "already_running";

const CAUSE_ADDRESS_UNAVAILABLE: &str = "address_unavailable";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BootStop {
    ServerPortTaken,
    ServerNotStarted,
    BootFailed { failure: String },
    AlreadyRunning,
    EndpointOverrideRefused { variable: String },
}

impl BootStop {
    pub(crate) fn is_startup_refusal(&self) -> bool {
        matches!(
            self,
            Self::AlreadyRunning | Self::EndpointOverrideRefused { .. }
        )
    }

    pub(crate) fn into_error(self, line: String) -> EmulatorError {
        match self {
            Self::ServerPortTaken => EmulatorError::ServerPortTaken { attempts: 1 },
            Self::ServerNotStarted => EmulatorError::ServerNotStarted { line },
            Self::BootFailed { failure } => EmulatorError::BootFailed { failure, line },
            Self::AlreadyRunning => EmulatorError::AlreadyRunning { line },
            Self::EndpointOverrideRefused { variable } => {
                EmulatorError::EndpointOverrideRefused { variable, line }
            }
        }
    }
}

pub(crate) fn boot_stop(line: &str) -> Option<BootStop> {
    let fields = signal_fields(line)?;
    let value = |key: &str| {
        fields
            .iter()
            .find(|(field, _)| *field == key)
            .map(|(_, value)| *value)
    };
    match (value(SIGNAL_FIELD)?, value(OUTCOME_FIELD)?) {
        (SIGNAL_SERVER_BIND, OUTCOME_FAILED)
            if value(CAUSE_FIELD) == Some(CAUSE_ADDRESS_UNAVAILABLE) =>
        {
            Some(BootStop::ServerPortTaken)
        }
        (
            SIGNAL_SERVER_BIND,
            OUTCOME_FAILED
            | OUTCOME_DISABLED
            | OUTCOME_INVALID_ADDRESS
            | OUTCOME_SETTINGS_UNAVAILABLE,
        ) => Some(BootStop::ServerNotStarted),
        (SIGNAL_READINESS, OUTCOME_FAILED) => Some(BootStop::BootFailed {
            failure: value(FAILURE_FIELD)?.to_owned(),
        }),
        (SIGNAL_STARTUP_REFUSED, OUTCOME_ALREADY_RUNNING) => Some(BootStop::AlreadyRunning),
        (SIGNAL_STARTUP_REFUSED, OUTCOME_ENDPOINT_OVERRIDE_REFUSED) => {
            Some(BootStop::EndpointOverrideRefused {
                variable: value(VARIABLE_FIELD)?.to_owned(),
            })
        }
        _ => None,
    }
}

fn signal_fields(line: &str) -> Option<Vec<(&str, &str)>> {
    let start = line.find(&format!(" {SIGNAL_FIELD}="))? + 1;
    if line[..start].contains('=') {
        return None;
    }
    let rest = &line[start..];
    let (pairs, error) = match rest.split_once(&format!(" {ERROR_FIELD}=")) {
        Some((pairs, error)) => (pairs, Some(error)),
        None => (rest, None),
    };
    let mut fields = Vec::new();
    let mut open: Option<(&str, usize)> = None;
    let mut offset = 0;
    for word in pairs.split(' ') {
        let word_start = offset;
        offset += word.len() + 1;
        let Some((key, _)) = word.split_once('=') else {
            continue;
        };
        if !is_field_key(key) {
            continue;
        }
        if let Some((open_key, value_start)) = open.take() {
            fields.push((open_key, pairs[value_start..word_start].trim_end()));
        }
        open = Some((key, word_start + key.len() + 1));
    }
    if let Some((open_key, value_start)) = open {
        fields.push((open_key, pairs[value_start..].trim_end()));
    }
    if let Some(error) = error {
        fields.push((ERROR_FIELD, error));
    }
    Some(fields)
}

fn is_field_key(key: &str) -> bool {
    !key.is_empty() && key.chars().all(|c| c.is_ascii_lowercase() || c == '_')
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
    use super::*;

    const STAMP: &str = "2026-10-28T10:00:00.000000Z";

    fn line(level: &str, message: &str, fields: &str) -> String {
        format!("{STAMP} {level:>5} {message} {fields}")
    }

    fn bind_line(fields: &str) -> String {
        line("ERROR", "server failed to start, leaving it off", fields)
    }

    #[test]
    fn only_a_taken_port_is_a_retryable_bind_failure_and_every_other_unbound_server_stops_readiness()
     {
        for (fields, expected) in [
            (
                "signal=server_bind outcome=bound addr=127.0.0.1:40000 port=40000",
                None,
            ),
            (
                "signal=server_bind outcome=failed cause=address_unavailable addr=127.0.0.1:40000 port=40000 error=could not bind to 127.0.0.1:40000: Address already in use (os error 98)",
                Some(BootStop::ServerPortTaken),
            ),
            (
                "signal=server_bind outcome=failed cause=lan_bind_disabled addr=0.0.0.0:40000 port=40000 error=refusing to bind to 0.0.0.0:40000: server.lan_bind_enabled is false",
                Some(BootStop::ServerNotStarted),
            ),
            (
                "signal=server_bind outcome=failed cause=lan_bind_unauthenticated addr=0.0.0.0:40000 port=40000 error=refusing to bind to 0.0.0.0:40000: bearer token missing",
                Some(BootStop::ServerNotStarted),
            ),
            (
                "signal=server_bind outcome=failed cause=other addr=127.0.0.1:40000 port=40000 error=storage error: locked",
                Some(BootStop::ServerNotStarted),
            ),
            (
                "signal=server_bind outcome=disabled",
                Some(BootStop::ServerNotStarted),
            ),
            (
                "signal=server_bind outcome=invalid_address bind_address=not an address",
                Some(BootStop::ServerNotStarted),
            ),
            (
                "signal=server_bind outcome=settings_unavailable error=database is locked",
                Some(BootStop::ServerNotStarted),
            ),
            ("signal=server_bind outcome=rebinding", None),
        ] {
            assert_eq!(boot_stop(&bind_line(fields)), expected, "{fields}");
        }
    }

    #[test]
    fn readiness_and_startup_refusal_signals_carry_their_kind() {
        for (fields, expected) in [
            ("signal=readiness outcome=ready", None),
            (
                "signal=readiness outcome=failed failure=upgrade_required",
                Some(BootStop::BootFailed {
                    failure: "upgrade_required".to_owned(),
                }),
            ),
            (
                "signal=startup_refused outcome=already_running",
                Some(BootStop::AlreadyRunning),
            ),
            (
                "signal=startup_refused outcome=endpoint_override_refused variable=FORGE_TWITCH_EVENTSUB_WS_URL error=endpoint override FORGE_TWITCH_EVENTSUB_WS_URL refused: host is not 127.0.0.0/8, ::1 or localhost",
                Some(BootStop::EndpointOverrideRefused {
                    variable: "FORGE_TWITCH_EVENTSUB_WS_URL".to_owned(),
                }),
            ),
        ] {
            assert_eq!(
                boot_stop(&line("ERROR", "boot", fields)),
                expected,
                "{fields}"
            );
        }
    }

    #[test]
    fn signal_fields_are_read_only_from_the_leading_field_run_and_never_from_the_error_text() {
        assert_eq!(
            boot_stop(&line(
                "ERROR",
                "platform endpoint override refused; exiting",
                "signal=startup_refused outcome=endpoint_override_refused variable=FORGE_KICK_API_BASE_URL error=bad value variable=FORGE_OTHER outcome=already_running",
            )),
            Some(BootStop::EndpointOverrideRefused {
                variable: "FORGE_KICK_API_BASE_URL".to_owned(),
            })
        );
    }

    #[test]
    fn lines_that_do_not_open_their_fields_with_a_complete_signal_are_ignored() {
        for text in [
            bind_line(
                "error=could not bind to 127.0.0.1:1: in use signal=server_bind outcome=failed cause=address_unavailable",
            ),
            line(
                " WARN",
                "chat message relayed",
                "user=viewer text=signal=server_bind outcome=failed cause=address_unavailable",
            ),
            line(
                "ERROR",
                "platform endpoint override refused; exiting",
                "error=endpoint override FORGE_TWITCH_API_BASE_URL refused: malformed",
            ),
            line(
                "ERROR",
                "refused",
                "signal=startup_refused outcome=endpoint_override_refused error=x",
            ),
            line("ERROR", "boot failed", "signal=readiness outcome=failed"),
            line("ERROR", "boot failed", "signal=readiness"),
            line("ERROR", "x", "signal=telemetry outcome=failed"),
            String::new(),
        ] {
            assert_eq!(boot_stop(&text), None, "{text:?}");
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
