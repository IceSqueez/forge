use forge_types::{MessageCatalog, MessageCode, MessageRef, UpstreamDetail, VerbatimText};

use crate::vocabulary::INTEGRATION_ID;
use crate::vocabulary::labels;

pub const STORED_SIGN_IN_UNREADABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "stored_sign_in_unreadable");

pub const HEALTH_LIVE_CHAT: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_live_chat");
pub const HEALTH_EVENTS: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_events");
pub const HEALTH_API_CALLS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_api_calls");

pub const HEALTH_QUOTA_RESETS_DAILY: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_quota_resets_daily");
pub const HEALTH_UNAVAILABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_unavailable");
pub const HEALTH_SHARED_CHAT_FEED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_shared_chat_feed");
pub const HEALTH_VIEWERS_LIVE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_viewers_live");

const RATE_LIMITED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "rate_limited");
const RATE_BUDGET_EXHAUSTED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "rate_budget_exhausted");
const QUOTA_EXHAUSTED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "quota_exhausted");
const REAUTH_REQUIRED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "reauth_required");
const AUTH_REFUSED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "auth_refused");
const REQUEST_REFUSED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "request_refused");
const REQUEST_FAILED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "request_failed");
const RESPONSE_UNREADABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "response_unreadable");
const UNRESOLVED_REFERENCE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "unresolved_reference");

pub(crate) const CONDITION_PERMANENT_BAN_ONLY: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_permanent_ban_only");
pub(crate) const CONDITION_TEMPORARY_TIMEOUT_ONLY: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_temporary_timeout_only");
pub(crate) const CONDITION_ANY_BAN_TYPE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_ban_type");

pub const YOUTUBE_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: INTEGRATION_ID,
    codes: &[
        HEALTH_LIVE_CHAT,
        HEALTH_EVENTS,
        HEALTH_API_CALLS,
        HEALTH_QUOTA_RESETS_DAILY,
        HEALTH_UNAVAILABLE,
        HEALTH_SHARED_CHAT_FEED,
        HEALTH_VIEWERS_LIVE,
        STORED_SIGN_IN_UNREADABLE,
        RATE_LIMITED,
        RATE_BUDGET_EXHAUSTED,
        QUOTA_EXHAUSTED,
        REAUTH_REQUIRED,
        AUTH_REFUSED,
        REQUEST_REFUSED,
        REQUEST_FAILED,
        RESPONSE_UNREADABLE,
        UNRESOLVED_REFERENCE,
        CONDITION_PERMANENT_BAN_ONLY,
        CONDITION_TEMPORARY_TIMEOUT_ONLY,
        CONDITION_ANY_BAN_TYPE,
        labels::BAN_USER,
        labels::BROADCAST,
        labels::CATEGORY_ID,
        labels::CATEGORY_ID_EXAMPLES,
        labels::CHANGE_PRIVACY,
        labels::CHANNEL_ID_PLACEHOLDER,
        labels::CHAT,
        labels::CHOICES_ONE_PER_LINE,
        labels::CONTENT,
        labels::CREATE_LIVE_POLL,
        labels::DELETE_MESSAGE,
        labels::DESCRIPTION,
        labels::IMAGE_PATH,
        labels::INSERT_AD_BREAK,
        labels::LOOKUP_VIEWER,
        labels::MESSAGE,
        labels::MESSAGE_ID_FIELD,
        labels::MESSAGE_ID_HINT,
        labels::NEW_STREAM_TITLE,
        labels::ONE_CHOICE_PER_LINE,
        labels::POLL_QUESTION,
        labels::POLLS,
        labels::PRIVATE,
        labels::PUBLIC,
        labels::QUESTION,
        labels::SEND_MESSAGE,
        labels::SET_CATEGORY,
        labels::SET_THUMBNAIL,
        labels::STREAM_DESCRIPTION,
        labels::STREAM_STATS,
        labels::TIMEOUT_USER,
        labels::TITLE,
        labels::UNLISTED,
        labels::UPDATE_DESCRIPTION,
        labels::UPDATE_TITLE,
        labels::USER,
        labels::USERNAME,
        labels::VIEWER_IDENTIFIER_EXAMPLE,
        labels::VISIBILITY,
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

pub(crate) fn health_quota_resets_daily() -> MessageRef {
    MessageRef::new(HEALTH_QUOTA_RESETS_DAILY)
}

pub(crate) fn health_unavailable() -> MessageRef {
    MessageRef::new(HEALTH_UNAVAILABLE)
}

pub(crate) fn health_shared_chat_feed() -> MessageRef {
    MessageRef::new(HEALTH_SHARED_CHAT_FEED)
}

pub(crate) fn health_viewers_live() -> MessageRef {
    MessageRef::new(HEALTH_VIEWERS_LIVE)
}

pub(crate) fn rate_limited(retry_after_secs: u32) -> MessageRef {
    MessageRef::new(RATE_LIMITED).with_arg(SECONDS_ARG, i64::from(retry_after_secs))
}

pub(crate) fn rate_budget_exhausted() -> MessageRef {
    MessageRef::new(RATE_BUDGET_EXHAUSTED)
}

pub(crate) fn quota_exhausted() -> MessageRef {
    MessageRef::new(QUOTA_EXHAUSTED)
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

pub(crate) fn unresolved_reference(name: &str) -> MessageRef {
    MessageRef::new(UNRESOLVED_REFERENCE).with_arg(FIELD_ARG, VerbatimText::new(name))
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
            MessageRef::new(HEALTH_LIVE_CHAT),
            MessageRef::new(HEALTH_EVENTS),
            MessageRef::new(HEALTH_API_CALLS),
            health_quota_resets_daily(),
            health_unavailable(),
            health_shared_chat_feed(),
            health_viewers_live(),
            MessageRef::new(STORED_SIGN_IN_UNREADABLE),
            rate_limited(7),
            rate_budget_exhausted(),
            quota_exhausted(),
            reauth_required(),
            auth_refused("c"),
            request_refused(500),
            request_failed("c"),
            response_unreadable("c"),
            unresolved_reference("title"),
            MessageRef::new(CONDITION_PERMANENT_BAN_ONLY),
            MessageRef::new(CONDITION_TEMPORARY_TIMEOUT_ONLY),
            MessageRef::new(CONDITION_ANY_BAN_TYPE),
        ]
    }

    #[test]
    fn every_constructor_code_is_catalogued_and_localized_in_both_languages_with_matching_variables()
     {
        let en = ftl_variables(YOUTUBE_MESSAGES.en_ftl);
        let uk = ftl_variables(YOUTUBE_MESSAGES.uk_ftl);
        for message in every_constructor() {
            let code = message.code();
            assert!(
                YOUTUBE_MESSAGES.codes.contains(code),
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
    fn every_catalogued_code_has_a_constructor_in_the_table() {
        let produced: BTreeSet<MessageCode> = every_constructor()
            .iter()
            .map(|m| m.code().clone())
            .collect();
        let catalogued: BTreeSet<MessageCode> = YOUTUBE_MESSAGES
            .codes
            .iter()
            .filter(|code| !code.name().starts_with("label_"))
            .cloned()
            .collect();
        assert_eq!(produced, catalogued);
    }

    #[test]
    fn english_and_ukrainian_declare_the_same_messages_with_the_same_variables() {
        assert_eq!(
            ftl_variables(YOUTUBE_MESSAGES.en_ftl),
            ftl_variables(YOUTUBE_MESSAGES.uk_ftl)
        );
    }

    #[test]
    fn upstream_cause_text_is_carried_as_detail_and_never_as_an_argument() {
        let failure = request_failed("quotaExceeded: daily limit");

        assert_eq!(
            failure.detail().map(|d| d.as_str()),
            Some("quotaExceeded: daily limit")
        );
        assert!(
            failure
                .args()
                .all(|(_, v)| !format!("{v:?}").contains("daily limit"))
        );
    }

    #[test]
    fn request_refused_status_is_an_integer_argument() {
        assert_eq!(
            request_refused(404).arg("status"),
            Some(&forge_types::MessageArg::Int(404))
        );
    }
}
