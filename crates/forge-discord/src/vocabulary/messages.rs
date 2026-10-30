use std::time::Duration;

use forge_types::{MessageCatalog, MessageCode, MessageRef, UpstreamDetail, VerbatimText};

use crate::vocabulary::INTEGRATION_ID;

pub const HEALTH_WEBHOOK_LATENCY: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_webhook_latency");
pub const HEALTH_RATE_LIMIT: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_rate_limit");
pub const HEALTH_LAST_SEND: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_last_send");
pub const HEALTH_ERRORS: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_errors");

const NETWORK_FAILED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "network_failed");
const RATE_LIMITED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "rate_limited");
const WEBHOOK_NOT_FOUND: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "webhook_not_found");
const SERIALIZATION_FAILED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "serialization_failed");
const CREDENTIAL_FAILED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "credential_failed");
const HTTP_REJECTED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "http_rejected");
const EMBED_COLOR_OUT_OF_RANGE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "embed_color_out_of_range");
const EMBED_TOO_MANY_FIELDS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "embed_too_many_fields");
const EMBED_PART_TOO_LONG: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "embed_part_too_long");
const EMBED_TOTAL_TOO_LONG: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "embed_total_too_long");
const EMBED_CONTROL_CHARACTER: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "embed_control_character");
const MESSAGE_ID_INVALID: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "message_id_invalid");
const NOT_WEBHOOK_ENDPOINT: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "not_webhook_endpoint");
const WEBHOOK_NAME_EMPTY: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "webhook_name_empty");
const BODY_ENCODING_FAILED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "body_encoding_failed");
const FILE_NAME_UNAVAILABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "file_name_unavailable");
const FILE_UNREADABLE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "file_unreadable");
const CONTENT_TOO_LONG: MessageCode = MessageCode::from_static(INTEGRATION_ID, "content_too_long");
const NOTHING_TO_EDIT: MessageCode = MessageCode::from_static(INTEGRATION_ID, "nothing_to_edit");
const FILE_NOT_UPLOADABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "file_not_uploadable");

const SECONDS_ARG: &str = "seconds";
const MILLIS_ARG: &str = "millis";
const SPAN_ARG: &str = "span";

const HEALTH_LATENCY_MILLIS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_latency_millis");
const HEALTH_LATENCY_SAMPLES: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_latency_samples");
const HEALTH_RESET_IN: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_reset_in");
const HEALTH_SEND_OK: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_send_ok");
const HEALTH_SEND_FAILED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_send_failed");
const HEALTH_SEND_NONE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_send_none");
const HEALTH_ERRORS_WINDOW: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_errors_window");
const WEBHOOK_ARG: &str = "webhook";
const STATUS_ARG: &str = "status";
const COLOR_ARG: &str = "color";
const COUNT_ARG: &str = "count";
const MAX_ARG: &str = "max";
const PART_ARG: &str = "part";
const LENGTH_ARG: &str = "length";
const TOTAL_ARG: &str = "total";
const PATH_ARG: &str = "path";

pub const DISCORD_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: INTEGRATION_ID,
    codes: &[
        HEALTH_WEBHOOK_LATENCY,
        HEALTH_RATE_LIMIT,
        HEALTH_LAST_SEND,
        HEALTH_ERRORS,
        HEALTH_LATENCY_MILLIS,
        HEALTH_LATENCY_SAMPLES,
        HEALTH_RESET_IN,
        HEALTH_SEND_OK,
        HEALTH_SEND_FAILED,
        HEALTH_SEND_NONE,
        HEALTH_ERRORS_WINDOW,
        NETWORK_FAILED,
        RATE_LIMITED,
        WEBHOOK_NOT_FOUND,
        SERIALIZATION_FAILED,
        CREDENTIAL_FAILED,
        HTTP_REJECTED,
        EMBED_COLOR_OUT_OF_RANGE,
        EMBED_TOO_MANY_FIELDS,
        EMBED_PART_TOO_LONG,
        EMBED_TOTAL_TOO_LONG,
        EMBED_CONTROL_CHARACTER,
        MESSAGE_ID_INVALID,
        NOT_WEBHOOK_ENDPOINT,
        WEBHOOK_NAME_EMPTY,
        BODY_ENCODING_FAILED,
        FILE_NAME_UNAVAILABLE,
        FILE_UNREADABLE,
        FILE_NOT_UPLOADABLE,
        CONTENT_TOO_LONG,
        NOTHING_TO_EDIT,
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};

fn detail(text: &str) -> UpstreamDetail {
    UpstreamDetail::from_redacted(text)
}

pub(crate) fn health_latency_millis(millis: u64) -> MessageRef {
    MessageRef::new(HEALTH_LATENCY_MILLIS).with_arg(MILLIS_ARG, millis)
}

pub(crate) fn health_latency_samples(count: usize) -> MessageRef {
    MessageRef::new(HEALTH_LATENCY_SAMPLES).with_arg(COUNT_ARG, count)
}

pub(crate) fn health_reset_in(span: Duration) -> MessageRef {
    MessageRef::new(HEALTH_RESET_IN).with_arg(SPAN_ARG, span)
}

pub(crate) fn health_send(last_send_ok: Option<bool>) -> MessageRef {
    MessageRef::new(match last_send_ok {
        Some(true) => HEALTH_SEND_OK,
        Some(false) => HEALTH_SEND_FAILED,
        None => HEALTH_SEND_NONE,
    })
}

pub(crate) fn health_errors_window() -> MessageRef {
    MessageRef::new(HEALTH_ERRORS_WINDOW)
}

pub(crate) fn network_failed(text: &str) -> MessageRef {
    MessageRef::new(NETWORK_FAILED).with_detail(detail(text))
}

pub(crate) fn rate_limited(retry_after_secs: f64) -> MessageRef {
    MessageRef::new(RATE_LIMITED).with_arg(SECONDS_ARG, retry_after_secs)
}

pub(crate) fn webhook_not_found(name: &str) -> MessageRef {
    MessageRef::new(WEBHOOK_NOT_FOUND).with_arg(WEBHOOK_ARG, VerbatimText::new(name))
}

pub(crate) fn serialization_failed(text: &str) -> MessageRef {
    MessageRef::new(SERIALIZATION_FAILED).with_detail(detail(text))
}

pub(crate) fn credential_failed(text: &str) -> MessageRef {
    MessageRef::new(CREDENTIAL_FAILED).with_detail(detail(text))
}

pub(crate) fn http_rejected(status: u16) -> MessageRef {
    MessageRef::new(HTTP_REJECTED).with_arg(STATUS_ARG, u32::from(status))
}

pub(crate) fn embed_color_out_of_range(color: u32) -> MessageRef {
    MessageRef::new(EMBED_COLOR_OUT_OF_RANGE)
        .with_arg(COLOR_ARG, VerbatimText::new(format!("0x{color:06X}")))
}

pub(crate) fn embed_too_many_fields(count: usize, max: usize) -> MessageRef {
    MessageRef::new(EMBED_TOO_MANY_FIELDS)
        .with_arg(COUNT_ARG, count)
        .with_arg(MAX_ARG, max)
}

pub(crate) fn embed_part_too_long(part: &str, length: usize, max: usize) -> MessageRef {
    MessageRef::new(EMBED_PART_TOO_LONG)
        .with_arg(PART_ARG, VerbatimText::new(part))
        .with_arg(LENGTH_ARG, length)
        .with_arg(MAX_ARG, max)
}

pub(crate) fn embed_total_too_long(total: usize, max: usize) -> MessageRef {
    MessageRef::new(EMBED_TOTAL_TOO_LONG)
        .with_arg(TOTAL_ARG, total)
        .with_arg(MAX_ARG, max)
}

pub(crate) fn embed_control_character(part: &str) -> MessageRef {
    MessageRef::new(EMBED_CONTROL_CHARACTER).with_arg(PART_ARG, VerbatimText::new(part))
}

pub(crate) fn message_id_invalid() -> MessageRef {
    MessageRef::new(MESSAGE_ID_INVALID)
}

pub(crate) fn not_webhook_endpoint() -> MessageRef {
    MessageRef::new(NOT_WEBHOOK_ENDPOINT)
}

pub(crate) fn webhook_name_empty() -> MessageRef {
    MessageRef::new(WEBHOOK_NAME_EMPTY)
}

pub(crate) fn body_encoding_failed(text: &str) -> MessageRef {
    MessageRef::new(BODY_ENCODING_FAILED).with_detail(detail(text))
}

pub(crate) fn file_name_unavailable(path: &str) -> MessageRef {
    MessageRef::new(FILE_NAME_UNAVAILABLE).with_arg(PATH_ARG, VerbatimText::new(path))
}

pub(crate) fn content_too_long(length: usize, max: usize) -> MessageRef {
    MessageRef::new(CONTENT_TOO_LONG)
        .with_arg(LENGTH_ARG, length)
        .with_arg(MAX_ARG, max)
}

pub(crate) fn nothing_to_edit() -> MessageRef {
    MessageRef::new(NOTHING_TO_EDIT)
}

pub(crate) fn file_not_uploadable(path: &str) -> MessageRef {
    MessageRef::new(FILE_NOT_UPLOADABLE).with_arg(PATH_ARG, VerbatimText::new(path))
}

pub(crate) fn file_unreadable(path: &str, text: &str) -> MessageRef {
    MessageRef::new(FILE_UNREADABLE)
        .with_arg(PATH_ARG, VerbatimText::new(path))
        .with_detail(detail(text))
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
            MessageRef::new(HEALTH_WEBHOOK_LATENCY),
            MessageRef::new(HEALTH_RATE_LIMIT),
            MessageRef::new(HEALTH_LAST_SEND),
            MessageRef::new(HEALTH_ERRORS),
            health_latency_millis(12),
            health_latency_samples(3),
            health_reset_in(Duration::from_secs(9)),
            health_send(Some(true)),
            health_send(Some(false)),
            health_send(None),
            health_errors_window(),
            network_failed("c"),
            rate_limited(1.5),
            webhook_not_found("alerts"),
            serialization_failed("c"),
            credential_failed("c"),
            http_rejected(400),
            embed_color_out_of_range(0x1000000),
            embed_too_many_fields(26, 25),
            embed_part_too_long("title", 300, 256),
            embed_total_too_long(7000, 6000),
            embed_control_character("title"),
            message_id_invalid(),
            not_webhook_endpoint(),
            webhook_name_empty(),
            body_encoding_failed("c"),
            file_name_unavailable("/tmp/x"),
            file_unreadable("/tmp/x", "c"),
            file_not_uploadable("/tmp/x"),
            content_too_long(2001, 2000),
            nothing_to_edit(),
        ]
    }

    #[test]
    fn every_constructor_code_is_catalogued_and_localized_in_both_languages_with_matching_variables()
     {
        let en = ftl_variables(DISCORD_MESSAGES.en_ftl);
        let uk = ftl_variables(DISCORD_MESSAGES.uk_ftl);
        for message in every_constructor() {
            let code = message.code();
            assert!(
                DISCORD_MESSAGES.codes.contains(code),
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
        let catalogued: BTreeSet<MessageCode> = DISCORD_MESSAGES.codes.iter().cloned().collect();
        assert_eq!(produced, catalogued);
    }

    #[test]
    fn english_and_ukrainian_declare_the_same_messages_with_the_same_variables() {
        assert_eq!(
            ftl_variables(DISCORD_MESSAGES.en_ftl),
            ftl_variables(DISCORD_MESSAGES.uk_ftl)
        );
    }

    #[test]
    fn rejected_request_carries_the_status_and_no_upstream_detail() {
        let failure = http_rejected(400);

        assert_eq!(
            failure.arg("status"),
            Some(&forge_types::MessageArg::Int(400))
        );
        assert!(failure.detail().is_none());
    }
}
