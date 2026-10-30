use forge_types::{MessageCatalog, MessageCode, MessageRef, UpstreamDetail, VerbatimText};

use crate::vocabulary::INTEGRATION_ID;

pub const STORED_SIGN_IN_UNREADABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "stored_sign_in_unreadable");

pub const HEALTH_LIVE_CHAT: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_live_chat");
pub const HEALTH_EVENTS: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_events");
pub const HEALTH_API_CALLS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_api_calls");

pub const HEALTH_POLLER_ACTIVE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_poller_active");
pub const HEALTH_POLLER_INTERVALS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_poller_intervals");
pub const HEALTH_POLLER_AUTH_REQUIRED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_poller_auth_required");
pub const HEALTH_POLLER_SIGN_IN_AGAIN: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_poller_sign_in_again");
pub const HEALTH_VIEWERS_LIVE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_viewers_live");
pub const HEALTH_RESETS_IN: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_resets_in");
pub const HEALTH_NO_USAGE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_no_usage");

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
        HEALTH_POLLER_ACTIVE,
        HEALTH_POLLER_INTERVALS,
        HEALTH_POLLER_AUTH_REQUIRED,
        HEALTH_POLLER_SIGN_IN_AGAIN,
        HEALTH_VIEWERS_LIVE,
        HEALTH_RESETS_IN,
        HEALTH_NO_USAGE,
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
        UNRESOLVED_REFERENCE,
        NOTHING_TO_UPDATE,
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};

const STATUS_ARG: &str = "status";
const SECONDS_ARG: &str = "seconds";
const FIELD_ARG: &str = "field";
const CHANNELS_ARG: &str = "channels";
const REDEMPTIONS_ARG: &str = "redemptions";

fn detailed(code: MessageCode, detail: &str) -> MessageRef {
    MessageRef::new(code).with_detail(UpstreamDetail::from_redacted(detail))
}

fn field(code: MessageCode, name: &str) -> MessageRef {
    MessageRef::new(code).with_arg(FIELD_ARG, VerbatimText::new(name))
}

pub(crate) fn health_poller_active() -> MessageRef {
    MessageRef::new(HEALTH_POLLER_ACTIVE)
}

pub(crate) fn health_poller_intervals(channels_secs: u64, redemptions_secs: u64) -> MessageRef {
    MessageRef::new(HEALTH_POLLER_INTERVALS)
        .with_arg(CHANNELS_ARG, channels_secs)
        .with_arg(REDEMPTIONS_ARG, redemptions_secs)
}

pub(crate) fn health_poller_auth_required() -> MessageRef {
    MessageRef::new(HEALTH_POLLER_AUTH_REQUIRED)
}

pub(crate) fn health_poller_sign_in_again() -> MessageRef {
    MessageRef::new(HEALTH_POLLER_SIGN_IN_AGAIN)
}

pub(crate) fn health_viewers_live() -> MessageRef {
    MessageRef::new(HEALTH_VIEWERS_LIVE)
}

pub(crate) fn health_resets_in(seconds: u64) -> MessageRef {
    MessageRef::new(HEALTH_RESETS_IN).with_arg(SECONDS_ARG, seconds)
}

pub(crate) fn health_no_usage() -> MessageRef {
    MessageRef::new(HEALTH_NO_USAGE)
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

pub(crate) fn request_refused(status: u16) -> MessageRef {
    MessageRef::new(REQUEST_REFUSED).with_arg(STATUS_ARG, i64::from(status))
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
            health_poller_active(),
            health_poller_intervals(30, 12),
            health_poller_auth_required(),
            health_poller_sign_in_again(),
            health_viewers_live(),
            health_resets_in(9),
            health_no_usage(),
            rate_limited(3),
            rate_budget_exhausted(),
            reauth_required(),
            auth_refused("c"),
            request_refused(500),
            request_failed("c"),
            response_unreadable("c"),
            token_unavailable("c"),
            broadcaster_unavailable("c"),
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
    fn refused_request_carries_the_status_and_no_upstream_detail() {
        let failure = request_refused(403);

        assert_eq!(
            failure.arg("status"),
            Some(&forge_types::MessageArg::Int(403))
        );
        assert!(failure.detail().is_none());
    }
}
