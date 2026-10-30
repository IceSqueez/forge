use forge_types::{MessageCatalog, MessageCode, MessageRef, UpstreamDetail};

use crate::vocabulary::{INTEGRATION_ID, VTubeDisconnectReason};

const CONNECT_REFUSED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "connect_refused");
const UNREACHABLE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "unreachable");
const CONNECT_FAILED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "connect_failed");
const TOKEN_REJECTED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "token_rejected");
const ACCESS_DENIED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "access_denied");
const APPROVAL_TIMEOUT: MessageCode = MessageCode::from_static(INTEGRATION_ID, "approval_timeout");
const AUTH_FAILED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "auth_failed");
const SUBSCRIBE_FAILED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "subscribe_failed");
const CONNECTION_LOST: MessageCode = MessageCode::from_static(INTEGRATION_ID, "connection_lost");
const UNRESPONSIVE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "unresponsive");
const NOT_CONNECTED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "not_connected");
const API_DISABLED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "api_disabled");
const REQUEST_FAILED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "request_failed");
const REQUEST_REJECTED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "request_rejected");
const REQUEST_TIMED_OUT: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "request_timed_out");
const PAYLOAD_INVALID: MessageCode = MessageCode::from_static(INTEGRATION_ID, "payload_invalid");
const ITEM_INSTANCE_MISSING: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "item_instance_missing");

const HEALTH_MODEL_NOT_LOADED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_model_not_loaded");
const HEALTH_MODEL_PARAMETERS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_model_parameters");
const HEALTH_EXPRESSIONS_HINT: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_expressions_hint");
const HEALTH_ITEMS_HINT: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_items_hint");
const HEALTH_TRACKING_FACE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_tracking_face");
const HEALTH_TRACKING_OFF: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_tracking_off");

const ERROR_ID_ARG: &str = "error_id";
const COUNT_ARG: &str = "count";

pub const HEALTH_MODEL: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_model");
pub const HEALTH_EXPRESSIONS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_expressions");
pub const HEALTH_ITEMS: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_items");
pub const HEALTH_TRACKING: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_tracking");

pub(crate) const CONDITION_ANY_MODEL: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_model");
pub(crate) const CONDITION_ANY_ITEM: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_item");
pub(crate) const CONDITION_ANY_EXPRESSION: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_expression");
pub(crate) const CONDITION_FACE_FOUND: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_face_found");
pub(crate) const CONDITION_FACE_LOST: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_face_lost");
pub(crate) const CONDITION_ANY_HOTKEY: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_hotkey");

pub const VTUBE_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: INTEGRATION_ID,
    codes: &[
        HEALTH_MODEL,
        HEALTH_EXPRESSIONS,
        HEALTH_ITEMS,
        HEALTH_TRACKING,
        HEALTH_MODEL_NOT_LOADED,
        HEALTH_MODEL_PARAMETERS,
        HEALTH_EXPRESSIONS_HINT,
        HEALTH_ITEMS_HINT,
        HEALTH_TRACKING_FACE,
        HEALTH_TRACKING_OFF,
        CONNECT_REFUSED,
        UNREACHABLE,
        CONNECT_FAILED,
        TOKEN_REJECTED,
        ACCESS_DENIED,
        APPROVAL_TIMEOUT,
        AUTH_FAILED,
        SUBSCRIBE_FAILED,
        CONNECTION_LOST,
        UNRESPONSIVE,
        NOT_CONNECTED,
        API_DISABLED,
        REQUEST_FAILED,
        REQUEST_REJECTED,
        REQUEST_TIMED_OUT,
        PAYLOAD_INVALID,
        ITEM_INSTANCE_MISSING,
        CONDITION_ANY_MODEL,
        CONDITION_ANY_ITEM,
        CONDITION_ANY_EXPRESSION,
        CONDITION_FACE_FOUND,
        CONDITION_FACE_LOST,
        CONDITION_ANY_HOTKEY,
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};

pub(crate) fn health_model_not_loaded() -> MessageRef {
    MessageRef::new(HEALTH_MODEL_NOT_LOADED)
}

pub(crate) fn health_model_parameters(count: u64) -> MessageRef {
    MessageRef::new(HEALTH_MODEL_PARAMETERS).with_arg(COUNT_ARG, count)
}

pub(crate) fn health_expressions_hint() -> MessageRef {
    MessageRef::new(HEALTH_EXPRESSIONS_HINT)
}

pub(crate) fn health_items_hint() -> MessageRef {
    MessageRef::new(HEALTH_ITEMS_HINT)
}

pub(crate) fn health_tracking(face_found: bool) -> MessageRef {
    MessageRef::new(if face_found {
        HEALTH_TRACKING_FACE
    } else {
        HEALTH_TRACKING_OFF
    })
}

pub(crate) fn connect_refused() -> MessageRef {
    MessageRef::new(CONNECT_REFUSED)
}

pub(crate) fn unreachable() -> MessageRef {
    MessageRef::new(UNREACHABLE)
}

pub(crate) fn connect_failed() -> MessageRef {
    MessageRef::new(CONNECT_FAILED)
}

pub(crate) fn connect_failed_with(detail: &str) -> MessageRef {
    connect_failed().with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn token_rejected() -> MessageRef {
    MessageRef::new(TOKEN_REJECTED)
}

pub(crate) fn access_denied() -> MessageRef {
    MessageRef::new(ACCESS_DENIED)
}

pub(crate) fn approval_timeout() -> MessageRef {
    MessageRef::new(APPROVAL_TIMEOUT)
}

pub(crate) fn not_connected() -> MessageRef {
    MessageRef::new(NOT_CONNECTED)
}

pub(crate) fn api_disabled() -> MessageRef {
    MessageRef::new(API_DISABLED)
}

pub(crate) fn request_failed(detail: &str) -> MessageRef {
    MessageRef::new(REQUEST_FAILED).with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn request_rejected(error_id: i64, detail: &str) -> MessageRef {
    MessageRef::new(REQUEST_REJECTED)
        .with_arg(ERROR_ID_ARG, error_id)
        .with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn request_timed_out() -> MessageRef {
    MessageRef::new(REQUEST_TIMED_OUT)
}

pub(crate) fn payload_invalid(detail: &str) -> MessageRef {
    MessageRef::new(PAYLOAD_INVALID).with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn item_instance_missing() -> MessageRef {
    MessageRef::new(ITEM_INSTANCE_MISSING)
}

pub(crate) fn for_disconnect_reason(reason: VTubeDisconnectReason) -> Option<MessageRef> {
    let code = match reason {
        VTubeDisconnectReason::AwaitingApproval => return None,
        VTubeDisconnectReason::ConnectFailed => CONNECT_FAILED,
        VTubeDisconnectReason::AuthRequired => TOKEN_REJECTED,
        VTubeDisconnectReason::AuthDenied => ACCESS_DENIED,
        VTubeDisconnectReason::AuthTimeout => APPROVAL_TIMEOUT,
        VTubeDisconnectReason::AuthFailed => AUTH_FAILED,
        VTubeDisconnectReason::SubscribeFailed => SUBSCRIBE_FAILED,
        VTubeDisconnectReason::SocketClosed => CONNECTION_LOST,
        VTubeDisconnectReason::Unresponsive => UNRESPONSIVE,
    };
    Some(MessageRef::new(code))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    const REASON_CODES: [(VTubeDisconnectReason, Option<&str>); 9] = [
        (VTubeDisconnectReason::AwaitingApproval, None),
        (VTubeDisconnectReason::ConnectFailed, Some("connect_failed")),
        (VTubeDisconnectReason::AuthRequired, Some("token_rejected")),
        (VTubeDisconnectReason::AuthDenied, Some("access_denied")),
        (VTubeDisconnectReason::AuthTimeout, Some("approval_timeout")),
        (VTubeDisconnectReason::AuthFailed, Some("auth_failed")),
        (
            VTubeDisconnectReason::SubscribeFailed,
            Some("subscribe_failed"),
        ),
        (VTubeDisconnectReason::SocketClosed, Some("connection_lost")),
        (VTubeDisconnectReason::Unresponsive, Some("unresponsive")),
    ];

    #[test]
    fn each_disconnect_reason_maps_to_its_own_catalogued_code_and_awaiting_approval_to_none() {
        for (reason, expected) in REASON_CODES {
            let mapped = for_disconnect_reason(reason);

            assert_eq!(
                mapped.as_ref().map(|message| message.code().name()),
                expected,
                "{reason:?}"
            );
            if let Some(message) = mapped {
                assert!(
                    VTUBE_MESSAGES.codes.contains(message.code()),
                    "{reason:?} maps to an uncatalogued code"
                );
            }
        }
    }

    #[test]
    fn every_dial_failure_code_is_listed_in_the_catalog() {
        for emitted in [connect_refused(), unreachable(), connect_failed()] {
            assert!(
                VTUBE_MESSAGES.codes.contains(emitted.code()),
                "{} is emitted but not catalogued",
                emitted.code()
            );
        }
    }

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
        let mut all = vec![
            connect_refused(),
            unreachable(),
            connect_failed(),
            connect_failed_with("c"),
            token_rejected(),
            access_denied(),
            approval_timeout(),
            not_connected(),
            api_disabled(),
            request_failed("c"),
            request_rejected(7, "c"),
            request_timed_out(),
            payload_invalid("c"),
            item_instance_missing(),
            health_model_not_loaded(),
            health_model_parameters(2),
            health_expressions_hint(),
            health_items_hint(),
            health_tracking(true),
            health_tracking(false),
            MessageRef::new(HEALTH_MODEL),
            MessageRef::new(HEALTH_EXPRESSIONS),
            MessageRef::new(HEALTH_ITEMS),
            MessageRef::new(HEALTH_TRACKING),
            MessageRef::new(CONDITION_ANY_MODEL),
            MessageRef::new(CONDITION_ANY_ITEM),
            MessageRef::new(CONDITION_ANY_EXPRESSION),
            MessageRef::new(CONDITION_FACE_FOUND),
            MessageRef::new(CONDITION_FACE_LOST),
            MessageRef::new(CONDITION_ANY_HOTKEY),
        ];
        all.extend(
            REASON_CODES
                .iter()
                .filter_map(|(r, _)| for_disconnect_reason(*r)),
        );
        all
    }

    #[test]
    fn every_constructor_code_is_catalogued_and_localized_in_both_languages_with_matching_variables()
     {
        let en = ftl_variables(VTUBE_MESSAGES.en_ftl);
        let uk = ftl_variables(VTUBE_MESSAGES.uk_ftl);
        for message in every_constructor() {
            let code = message.code();
            assert!(VTUBE_MESSAGES.codes.contains(code), "{code} not catalogued");
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
        let catalogued: BTreeSet<MessageCode> = VTUBE_MESSAGES.codes.iter().cloned().collect();
        assert_eq!(produced, catalogued);
    }

    #[test]
    fn english_and_ukrainian_declare_the_same_messages_with_the_same_variables() {
        assert_eq!(
            ftl_variables(VTUBE_MESSAGES.en_ftl),
            ftl_variables(VTUBE_MESSAGES.uk_ftl)
        );
    }

    #[test]
    fn upstream_cause_text_is_carried_as_detail_and_never_as_an_argument() {
        let failure = request_rejected(3, "No such hotkey: secret-name");

        assert_eq!(
            failure.detail().map(|d| d.as_str()),
            Some("No such hotkey: secret-name")
        );
        assert!(
            failure
                .args()
                .all(|(_, v)| !format!("{v:?}").contains("secret-name"))
        );
    }
}
