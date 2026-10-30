use forge_types::{
    MessageArg, MessageCatalog, MessageCode, MessageRef, UpstreamDetail, VerbatimText,
};
use time::OffsetDateTime;

use crate::error::HotkeyError;
use crate::vocabulary::INTEGRATION_ID;
use crate::vocabulary::labels;

pub const COMBO_ARG: &str = "combo";
const TIME_ARG: &str = "time";

const BACKEND_UNAVAILABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "backend_unavailable");
const BINDING_NOT_FOUND: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "binding_not_found");
const EDGE_TAKEN: MessageCode = MessageCode::from_static(INTEGRATION_ID, "edge_taken");
const STORAGE_FAILED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "storage_failed");
pub const PORTAL_UNAVAILABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "portal_unavailable");
pub const PERMISSION_DENIED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "permission_denied");
pub const COMBO_REJECTED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "combo_rejected");
pub const ENGINE_STOPPED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "engine_stopped");
pub const BACKEND_FAILED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "backend_failed");

pub const HEALTH_REGISTERED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_registered");
pub const HEALTH_LAST_TRIGGERED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_last_triggered");
pub const HEALTH_CONFLICTS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_conflicts");
pub const HEALTH_PORTAL: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_portal");
pub const HEALTH_BACKEND: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_backend");

const HEALTH_REGISTERED_HINT: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_registered_hint");
const HEALTH_DISABLED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_disabled");
const HEALTH_LAST_TRIGGERED_AT: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_last_triggered_at");
const HEALTH_CONFLICTS_HINT: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_conflicts_hint");
const HEALTH_PORTAL_SESSION_LOST: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_portal_session_lost");
const HEALTH_PORTAL_ACTIVE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_portal_active");
const HEALTH_PORTAL_EVDEV_FALLBACK: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_portal_evdev_fallback");
const HEALTH_PORTAL_PERMISSION_DENIED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_portal_permission_denied");
const HEALTH_BACKEND_ACTIVE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_backend_active");
const HEALTH_BACKEND_UNAVAILABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_backend_unavailable");

pub(crate) const CONDITION_NO_HOTKEY: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_no_hotkey");
pub(crate) const CONDITION_ANY_HOTKEY: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_hotkey");

pub const HOTKEY_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: INTEGRATION_ID,
    codes: &[
        HEALTH_REGISTERED,
        HEALTH_LAST_TRIGGERED,
        HEALTH_CONFLICTS,
        HEALTH_PORTAL,
        HEALTH_BACKEND,
        HEALTH_REGISTERED_HINT,
        HEALTH_DISABLED,
        HEALTH_LAST_TRIGGERED_AT,
        HEALTH_CONFLICTS_HINT,
        HEALTH_PORTAL_SESSION_LOST,
        HEALTH_PORTAL_ACTIVE,
        HEALTH_PORTAL_EVDEV_FALLBACK,
        HEALTH_PORTAL_PERMISSION_DENIED,
        HEALTH_BACKEND_ACTIVE,
        HEALTH_BACKEND_UNAVAILABLE,
        BACKEND_UNAVAILABLE,
        BINDING_NOT_FOUND,
        EDGE_TAKEN,
        STORAGE_FAILED,
        PORTAL_UNAVAILABLE,
        PERMISSION_DENIED,
        COMBO_REJECTED,
        ENGINE_STOPPED,
        BACKEND_FAILED,
        CONDITION_ANY_HOTKEY,
        CONDITION_NO_HOTKEY,
        labels::ACTIVE,
        labels::RECENT_TRIGGERS,
        labels::REGISTERED_HOTKEYS,
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};

pub(crate) fn health_registered_hint() -> MessageRef {
    MessageRef::new(HEALTH_REGISTERED_HINT)
}

pub(crate) fn health_disabled() -> MessageRef {
    MessageRef::new(HEALTH_DISABLED)
}

pub(crate) fn health_last_triggered_at(at: OffsetDateTime, combo: &str) -> MessageRef {
    MessageRef::new(HEALTH_LAST_TRIGGERED_AT)
        .with_arg(TIME_ARG, MessageArg::TimeOfDay(at))
        .with_arg(COMBO_ARG, forge_types::VerbatimText::new(combo))
}

pub(crate) fn health_conflicts_hint() -> MessageRef {
    MessageRef::new(HEALTH_CONFLICTS_HINT)
}

#[cfg(target_os = "linux")]
pub(crate) fn health_portal_session_lost() -> MessageRef {
    MessageRef::new(HEALTH_PORTAL_SESSION_LOST)
}

#[cfg(target_os = "linux")]
pub(crate) fn health_portal_active() -> MessageRef {
    MessageRef::new(HEALTH_PORTAL_ACTIVE)
}

#[cfg(target_os = "linux")]
pub(crate) fn health_portal_evdev_fallback() -> MessageRef {
    MessageRef::new(HEALTH_PORTAL_EVDEV_FALLBACK)
}

#[cfg(target_os = "linux")]
pub(crate) fn health_portal_permission_denied() -> MessageRef {
    MessageRef::new(HEALTH_PORTAL_PERMISSION_DENIED)
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn health_backend_active() -> MessageRef {
    MessageRef::new(HEALTH_BACKEND_ACTIVE)
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn health_backend_unavailable() -> MessageRef {
    MessageRef::new(HEALTH_BACKEND_UNAVAILABLE)
}

fn combo_rejected(combo: &str) -> MessageRef {
    MessageRef::new(COMBO_REJECTED).with_arg(COMBO_ARG, VerbatimText::new(combo))
}

fn with_detail(code: MessageCode, detail: &str) -> MessageRef {
    MessageRef::new(code).with_detail(UpstreamDetail::from_redacted(detail))
}

pub fn failure(error: &HotkeyError) -> MessageRef {
    match error {
        HotkeyError::PortalUnavailable { reason } => with_detail(PORTAL_UNAVAILABLE, reason),
        HotkeyError::PermissionDenied => MessageRef::new(PERMISSION_DENIED),
        HotkeyError::AlreadyRegistered { combo } | HotkeyError::InvalidCombo(combo) => {
            combo_rejected(combo)
        }
        HotkeyError::BindRejected { combo, reason } => {
            combo_rejected(combo).with_detail(UpstreamDetail::from_redacted(reason))
        }
        HotkeyError::SupervisorUnavailable | HotkeyError::MainThreadUnavailable => {
            MessageRef::new(ENGINE_STOPPED)
        }
        HotkeyError::Backend(cause) => with_detail(BACKEND_FAILED, cause),
        HotkeyError::BackendUnavailable => MessageRef::new(BACKEND_UNAVAILABLE),
        HotkeyError::Storage(source) => with_detail(STORAGE_FAILED, &source.to_string()),
        HotkeyError::BindingNotFound => MessageRef::new(BINDING_NOT_FOUND),
        HotkeyError::EdgeTaken { combo } => {
            MessageRef::new(EDGE_TAKEN).with_arg(COMBO_ARG, VerbatimText::new(combo.as_str()))
        }
    }
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

    fn every_failure() -> Vec<(
        HotkeyError,
        MessageCode,
        Option<&'static str>,
        Option<&'static str>,
    )> {
        vec![
            (
                HotkeyError::PortalUnavailable {
                    reason: "no d-bus".to_owned(),
                },
                PORTAL_UNAVAILABLE,
                None,
                Some("no d-bus"),
            ),
            (HotkeyError::PermissionDenied, PERMISSION_DENIED, None, None),
            (
                HotkeyError::AlreadyRegistered {
                    combo: "Ctrl+A".to_owned(),
                },
                COMBO_REJECTED,
                Some("Ctrl+A"),
                None,
            ),
            (
                HotkeyError::InvalidCombo("bad+".to_owned()),
                COMBO_REJECTED,
                Some("bad+"),
                None,
            ),
            (
                HotkeyError::BindRejected {
                    combo: "Ctrl+B".to_owned(),
                    reason: "cancelled".to_owned(),
                },
                COMBO_REJECTED,
                Some("Ctrl+B"),
                Some("cancelled"),
            ),
            (
                HotkeyError::SupervisorUnavailable,
                ENGINE_STOPPED,
                None,
                None,
            ),
            (
                HotkeyError::MainThreadUnavailable,
                ENGINE_STOPPED,
                None,
                None,
            ),
            (
                HotkeyError::Backend("channel closed".to_owned()),
                BACKEND_FAILED,
                None,
                Some("channel closed"),
            ),
            (
                HotkeyError::BackendUnavailable,
                BACKEND_UNAVAILABLE,
                None,
                None,
            ),
            (
                HotkeyError::Storage(forge_storage::StorageError::Connection {
                    reason: "disk full".to_owned(),
                }),
                STORAGE_FAILED,
                None,
                Some("connection failed: disk full"),
            ),
            (HotkeyError::BindingNotFound, BINDING_NOT_FOUND, None, None),
            (
                HotkeyError::EdgeTaken {
                    combo: "Ctrl+E".to_owned(),
                },
                EDGE_TAKEN,
                Some("Ctrl+E"),
                None,
            ),
        ]
    }

    #[test]
    fn each_hotkey_error_maps_to_its_code_with_the_combo_and_detail_it_carries() {
        for (error, code, combo, detail) in every_failure() {
            let message = failure(&error);

            assert_eq!(message.code(), &code, "{error:?}");
            assert_eq!(
                message.arg(COMBO_ARG),
                combo
                    .map(|combo| MessageArg::Verbatim(VerbatimText::new(combo)))
                    .as_ref(),
                "{error:?}"
            );
            assert_eq!(
                message.detail().map(UpstreamDetail::as_str),
                detail,
                "{error:?}"
            );
        }
    }

    fn every_constructor() -> Vec<MessageRef> {
        let mut all = vec![
            MessageRef::new(CONDITION_ANY_HOTKEY),
            MessageRef::new(CONDITION_NO_HOTKEY),
            health_registered_hint(),
            health_disabled(),
            health_last_triggered_at(OffsetDateTime::UNIX_EPOCH, "Ctrl+A"),
            health_conflicts_hint(),
        ];
        #[cfg(target_os = "linux")]
        all.extend([
            health_portal_session_lost(),
            health_portal_active(),
            health_portal_evdev_fallback(),
            health_portal_permission_denied(),
        ]);
        #[cfg(not(target_os = "linux"))]
        all.extend([health_backend_active(), health_backend_unavailable()]);
        all.extend(every_failure().iter().map(|(error, ..)| failure(error)));
        all
    }

    #[test]
    fn every_constructor_is_catalogued_and_localized_with_the_variables_it_passes() {
        let en = ftl_variables(HOTKEY_MESSAGES.en_ftl);
        let uk = ftl_variables(HOTKEY_MESSAGES.uk_ftl);
        for message in every_constructor() {
            let code = message.code();
            assert!(
                HOTKEY_MESSAGES.codes.contains(code),
                "{code} not catalogued"
            );
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
    fn every_catalogued_code_is_declared_in_both_languages() {
        for (language, source) in [
            ("en", HOTKEY_MESSAGES.en_ftl),
            ("uk", HOTKEY_MESSAGES.uk_ftl),
        ] {
            let declared = ftl_variables(source);
            for code in HOTKEY_MESSAGES.codes {
                assert!(
                    declared.contains_key(&code.ftl_key()),
                    "{code} missing in {language}"
                );
            }
        }
    }

    #[test]
    fn english_and_ukrainian_declare_the_same_messages_with_the_same_variables() {
        assert_eq!(
            ftl_variables(HOTKEY_MESSAGES.en_ftl),
            ftl_variables(HOTKEY_MESSAGES.uk_ftl)
        );
    }
}
