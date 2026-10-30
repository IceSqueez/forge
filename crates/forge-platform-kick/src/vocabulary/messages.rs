use forge_types::{MessageCatalog, MessageCode, MessageRef, UpstreamDetail, VerbatimText};

use crate::vocabulary::INTEGRATION_ID;

pub const STORED_SIGN_IN_UNREADABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "stored_sign_in_unreadable");

pub const HEALTH_LIVE_CHAT: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_live_chat");
pub const HEALTH_EVENTS: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_events");
pub const HEALTH_API_CALLS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_api_calls");

const RATE_LIMITED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "rate_limited");
const RATE_BUDGET_EXHAUSTED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "rate_budget_exhausted");
const REAUTH_REQUIRED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "reauth_required");
const AUTH_REFUSED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "auth_refused");
const REQUEST_REFUSED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "request_refused");
const REQUEST_FAILED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "request_failed");
const RESPONSE_UNREADABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "response_unreadable");
const TOKEN_UNAVAILABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "token_unavailable");
const BROADCASTER_UNAVAILABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "broadcaster_unavailable");
const EMPTY_AFTER_INTERPOLATION: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "empty_after_interpolation");
const UNRESOLVED_REFERENCE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "unresolved_reference");
const NOTHING_TO_UPDATE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "nothing_to_update");

pub const KICK_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: INTEGRATION_ID,
    codes: &[
        HEALTH_LIVE_CHAT,
        HEALTH_EVENTS,
        HEALTH_API_CALLS,
        STORED_SIGN_IN_UNREADABLE,
        RATE_LIMITED,
        RATE_BUDGET_EXHAUSTED,
        REAUTH_REQUIRED,
        AUTH_REFUSED,
        REQUEST_REFUSED,
        REQUEST_FAILED,
        RESPONSE_UNREADABLE,
        TOKEN_UNAVAILABLE,
        BROADCASTER_UNAVAILABLE,
        EMPTY_AFTER_INTERPOLATION,
        UNRESOLVED_REFERENCE,
        NOTHING_TO_UPDATE,
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};

const STATUS_ARG: &str = "status";
const SECONDS_ARG: &str = "seconds";
const FIELD_ARG: &str = "field";

fn detailed(code: MessageCode, detail: &str) -> MessageRef {
    MessageRef::new(code).with_detail(UpstreamDetail::from_redacted(detail))
}

fn field(code: MessageCode, name: &str) -> MessageRef {
    MessageRef::new(code).with_arg(FIELD_ARG, VerbatimText::new(name))
}

pub(crate) fn rate_limited(retry_after_secs: u32) -> MessageRef {
    MessageRef::new(RATE_LIMITED).with_arg(SECONDS_ARG, i64::from(retry_after_secs))
}

pub(crate) fn rate_budget_exhausted() -> MessageRef {
    MessageRef::new(RATE_BUDGET_EXHAUSTED)
}

pub(crate) fn reauth_required() -> MessageRef {
    MessageRef::new(REAUTH_REQUIRED)
}

pub(crate) fn auth_refused(detail: &str) -> MessageRef {
    detailed(AUTH_REFUSED, detail)
}

pub(crate) fn request_refused(status: u16, detail: &str) -> MessageRef {
    detailed(REQUEST_REFUSED, detail).with_arg(STATUS_ARG, i64::from(status))
}

pub(crate) fn request_failed(detail: &str) -> MessageRef {
    detailed(REQUEST_FAILED, detail)
}

pub(crate) fn response_unreadable(detail: &str) -> MessageRef {
    detailed(RESPONSE_UNREADABLE, detail)
}

pub(crate) fn token_unavailable(detail: &str) -> MessageRef {
    detailed(TOKEN_UNAVAILABLE, detail)
}

pub(crate) fn broadcaster_unavailable(detail: &str) -> MessageRef {
    detailed(BROADCASTER_UNAVAILABLE, detail)
}

pub(crate) fn empty_after_interpolation(name: &str) -> MessageRef {
    field(EMPTY_AFTER_INTERPOLATION, name)
}

pub(crate) fn unresolved_reference(name: &str) -> MessageRef {
    field(UNRESOLVED_REFERENCE, name)
}

pub(crate) fn nothing_to_update() -> MessageRef {
    MessageRef::new(NOTHING_TO_UPDATE)
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
            MessageRef::new(STORED_SIGN_IN_UNREADABLE),
            MessageRef::new(HEALTH_LIVE_CHAT),
            MessageRef::new(HEALTH_EVENTS),
            MessageRef::new(HEALTH_API_CALLS),
            rate_limited(3),
            rate_budget_exhausted(),
            reauth_required(),
            auth_refused("c"),
            request_refused(500, "c"),
            request_failed("c"),
            response_unreadable("c"),
            token_unavailable("c"),
            broadcaster_unavailable("c"),
            empty_after_interpolation("f"),
            unresolved_reference("f"),
            nothing_to_update(),
        ]
    }

    #[test]
    fn every_constructor_code_is_catalogued_and_localized_in_both_languages_with_matching_variables()
     {
        let en = ftl_variables(KICK_MESSAGES.en_ftl);
        let uk = ftl_variables(KICK_MESSAGES.uk_ftl);
        for message in every_constructor() {
            let code = message.code();
            assert!(KICK_MESSAGES.codes.contains(code), "{code} not catalogued");
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
        let catalogued: BTreeSet<MessageCode> = KICK_MESSAGES.codes.iter().cloned().collect();
        assert_eq!(produced, catalogued);
    }

    #[test]
    fn english_and_ukrainian_declare_the_same_messages_with_the_same_variables() {
        assert_eq!(
            ftl_variables(KICK_MESSAGES.en_ftl),
            ftl_variables(KICK_MESSAGES.uk_ftl)
        );
    }

    #[test]
    fn upstream_cause_text_is_carried_as_detail_and_never_as_an_argument() {
        let failure = request_refused(403, "Forbidden: missing scope");

        assert_eq!(
            failure.detail().map(|d| d.as_str()),
            Some("Forbidden: missing scope")
        );
        assert!(
            failure
                .args()
                .all(|(_, v)| !format!("{v:?}").contains("missing scope"))
        );
    }
}
