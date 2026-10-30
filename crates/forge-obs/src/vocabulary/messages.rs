use forge_types::{MessageCatalog, MessageCode, MessageRef, UpstreamDetail, VerbatimText};

use crate::vocabulary::INTEGRATION_ID;

const CONNECT_REFUSED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "connect_refused");
const UNREACHABLE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "unreachable");
const PASSWORD_REJECTED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "password_rejected");
const CONNECT_FAILED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "connect_failed");
const CONNECTION_LOST: MessageCode = MessageCode::from_static(INTEGRATION_ID, "connection_lost");
const UNRESPONSIVE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "unresponsive");
const NOT_CONNECTED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "not_connected");
const REQUEST_TIMED_OUT: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "request_timed_out");
const REQUEST_REJECTED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "request_rejected");
const PROTOCOL_ERROR: MessageCode = MessageCode::from_static(INTEGRATION_ID, "protocol_error");
const PAYLOAD_INVALID: MessageCode = MessageCode::from_static(INTEGRATION_ID, "payload_invalid");
const STUDIO_MODE_INACTIVE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "studio_mode_inactive");

const HEALTH_LIVE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_live");
const HEALTH_OFF: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_off");
const HEALTH_PAUSED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_paused");
const HEALTH_ACTIVE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_active");
const HEALTH_LAGGING: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_lagging");
const HEALTH_SMOOTH: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_smooth");
const HEALTH_CPU_FPS_VALUE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_cpu_fps_value");
const HEALTH_DROPPED_COUNT: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_dropped_count");
const HEALTH_DROPPED_SHARE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_dropped_share");

const REQUEST_ARG: &str = "request";
const CPU_ARG: &str = "cpu";
const FPS_ARG: &str = "fps";
const COUNT_ARG: &str = "count";
const PERCENT_ARG: &str = "percent";

pub const HEALTH_STREAM: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_stream");
pub const HEALTH_RECORDING: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_recording");
pub const HEALTH_CPU_FPS: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_cpu_fps");
pub const HEALTH_DROPPED_FRAMES: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_dropped_frames");

pub const OBS_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: INTEGRATION_ID,
    codes: &[
        HEALTH_STREAM,
        HEALTH_RECORDING,
        HEALTH_CPU_FPS,
        HEALTH_DROPPED_FRAMES,
        HEALTH_LIVE,
        HEALTH_OFF,
        HEALTH_PAUSED,
        HEALTH_ACTIVE,
        HEALTH_LAGGING,
        HEALTH_SMOOTH,
        HEALTH_CPU_FPS_VALUE,
        HEALTH_DROPPED_COUNT,
        HEALTH_DROPPED_SHARE,
        CONNECT_REFUSED,
        UNREACHABLE,
        PASSWORD_REJECTED,
        CONNECT_FAILED,
        CONNECTION_LOST,
        UNRESPONSIVE,
        NOT_CONNECTED,
        REQUEST_TIMED_OUT,
        REQUEST_REJECTED,
        PROTOCOL_ERROR,
        PAYLOAD_INVALID,
        STUDIO_MODE_INACTIVE,
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};

pub(crate) fn health_live() -> MessageRef {
    MessageRef::new(HEALTH_LIVE)
}

pub(crate) fn health_off() -> MessageRef {
    MessageRef::new(HEALTH_OFF)
}

pub(crate) fn health_paused() -> MessageRef {
    MessageRef::new(HEALTH_PAUSED)
}

pub(crate) fn health_active() -> MessageRef {
    MessageRef::new(HEALTH_ACTIVE)
}

pub(crate) fn health_lagging() -> MessageRef {
    MessageRef::new(HEALTH_LAGGING)
}

pub(crate) fn health_smooth() -> MessageRef {
    MessageRef::new(HEALTH_SMOOTH)
}

pub(crate) fn health_cpu_fps_value(cpu_percent: f64, fps: f64) -> MessageRef {
    MessageRef::new(HEALTH_CPU_FPS_VALUE)
        .with_arg(CPU_ARG, cpu_percent)
        .with_arg(FPS_ARG, fps)
}

pub(crate) fn health_dropped_count(dropped: u64) -> MessageRef {
    MessageRef::new(HEALTH_DROPPED_COUNT).with_arg(COUNT_ARG, dropped)
}

pub(crate) fn health_dropped_share(percent: f64) -> MessageRef {
    MessageRef::new(HEALTH_DROPPED_SHARE).with_arg(PERCENT_ARG, percent)
}

pub(crate) fn connect_refused() -> MessageRef {
    MessageRef::new(CONNECT_REFUSED)
}

pub(crate) fn unreachable() -> MessageRef {
    MessageRef::new(UNREACHABLE)
}

pub(crate) fn password_rejected() -> MessageRef {
    MessageRef::new(PASSWORD_REJECTED)
}

pub(crate) fn connect_failed() -> MessageRef {
    MessageRef::new(CONNECT_FAILED)
}

pub(crate) fn connect_failed_with(detail: &str) -> MessageRef {
    connect_failed().with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn not_connected() -> MessageRef {
    MessageRef::new(NOT_CONNECTED)
}

pub(crate) fn request_timed_out() -> MessageRef {
    MessageRef::new(REQUEST_TIMED_OUT)
}

pub(crate) fn request_rejected(request: &str, detail: &str) -> MessageRef {
    MessageRef::new(REQUEST_REJECTED)
        .with_arg(REQUEST_ARG, VerbatimText::new(request))
        .with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn protocol_error(detail: &str) -> MessageRef {
    MessageRef::new(PROTOCOL_ERROR).with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn payload_invalid(detail: &str) -> MessageRef {
    MessageRef::new(PAYLOAD_INVALID).with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn studio_mode_inactive() -> MessageRef {
    MessageRef::new(STUDIO_MODE_INACTIVE)
}

pub(crate) fn connection_lost() -> MessageRef {
    MessageRef::new(CONNECTION_LOST)
}

pub(crate) fn unresponsive() -> MessageRef {
    MessageRef::new(UNRESPONSIVE)
}
#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    fn ftl_variables(source: &str) -> BTreeMap<String, BTreeSet<String>> {
        let mut found: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut current = None;
        for line in source.lines() {
            if let Some((id, _)) = line.split_once(" = ").filter(|_| line.starts_with("msg-")) {
                current = Some(id.trim().to_owned());
                found.entry(id.trim().to_owned()).or_default();
            }
            let Some(id) = &current else { continue };
            let mut rest = line;
            while let Some(at) = rest.find('$') {
                rest = &rest[at + 1..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                    .collect();
                found.get_mut(id).unwrap().insert(name);
            }
        }
        found
    }

    fn every_constructor() -> Vec<MessageRef> {
        vec![
            connect_refused(),
            unreachable(),
            password_rejected(),
            connect_failed(),
            connect_failed_with("c"),
            connection_lost(),
            unresponsive(),
            not_connected(),
            request_timed_out(),
            request_rejected("GetVersion", "c"),
            protocol_error("c"),
            payload_invalid("c"),
            studio_mode_inactive(),
            health_live(),
            health_off(),
            health_paused(),
            health_active(),
            health_lagging(),
            health_smooth(),
            health_cpu_fps_value(1.0, 2.0),
            health_dropped_count(1),
            health_dropped_share(0.5),
            MessageRef::new(HEALTH_STREAM),
            MessageRef::new(HEALTH_RECORDING),
            MessageRef::new(HEALTH_CPU_FPS),
            MessageRef::new(HEALTH_DROPPED_FRAMES),
        ]
    }

    #[test]
    fn every_constructor_code_is_catalogued_and_localized_in_both_languages_with_matching_variables()
     {
        let en = ftl_variables(OBS_MESSAGES.en_ftl);
        let uk = ftl_variables(OBS_MESSAGES.uk_ftl);
        for message in every_constructor() {
            let code = message.code();
            assert!(OBS_MESSAGES.codes.contains(code), "{code} not catalogued");
            let passed: BTreeSet<String> = message.args().map(|(n, _)| n.to_owned()).collect();
            for (language, ftl) in [("en", &en), ("uk", &uk)] {
                let used = ftl
                    .get(&code.ftl_key())
                    .unwrap_or_else(|| panic!("{code} missing in {language}"));
                assert_eq!(used, &passed, "{code} variables in {language}");
            }
        }
    }

    #[test]
    fn every_catalogued_code_has_a_constructor_in_the_table() {
        let produced: BTreeSet<MessageCode> = every_constructor()
            .iter()
            .map(|m| m.code().clone())
            .collect();
        let catalogued: BTreeSet<MessageCode> = OBS_MESSAGES.codes.iter().cloned().collect();
        assert_eq!(produced, catalogued);
    }

    #[test]
    fn english_and_ukrainian_declare_the_same_messages_with_the_same_variables() {
        assert_eq!(
            ftl_variables(OBS_MESSAGES.en_ftl),
            ftl_variables(OBS_MESSAGES.uk_ftl)
        );
    }

    #[test]
    fn upstream_cause_text_is_carried_as_detail_and_never_as_an_argument() {
        let failure = request_rejected("SetInputVolume", "ResourceNotFound: no such input");

        assert_eq!(
            failure.detail().map(|d| d.as_str()),
            Some("ResourceNotFound: no such input")
        );
        assert!(
            failure
                .args()
                .all(|(_, v)| !format!("{v:?}").contains("no such input"))
        );
    }
}
