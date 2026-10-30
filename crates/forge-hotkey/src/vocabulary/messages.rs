use forge_types::{MessageArg, MessageCatalog, MessageCode, MessageRef};
use time::OffsetDateTime;

use crate::vocabulary::INTEGRATION_ID;

pub const COMBO_ARG: &str = "combo";
const TIME_ARG: &str = "time";

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
const HEALTH_BACKEND_NOT_APPLICABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_backend_not_applicable");

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
        HEALTH_BACKEND_NOT_APPLICABLE,
        PORTAL_UNAVAILABLE,
        PERMISSION_DENIED,
        COMBO_REJECTED,
        ENGINE_STOPPED,
        BACKEND_FAILED,
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
pub(crate) fn health_backend_not_applicable() -> MessageRef {
    MessageRef::new(HEALTH_BACKEND_NOT_APPLICABLE)
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

    fn health_constructors() -> Vec<MessageRef> {
        let mut all = vec![
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
        all.push(health_backend_not_applicable());
        all
    }

    #[test]
    fn every_health_constructor_is_catalogued_and_localized_with_the_variables_it_passes() {
        let en = ftl_variables(HOTKEY_MESSAGES.en_ftl);
        let uk = ftl_variables(HOTKEY_MESSAGES.uk_ftl);
        for message in health_constructors() {
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
