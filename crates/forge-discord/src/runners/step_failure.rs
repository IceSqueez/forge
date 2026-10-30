use forge_types::{MessageRef, SubActionOutcome};

use crate::error::{DiscordError, ValidationFault};
use crate::vocabulary::messages;

pub(crate) fn message(error: &DiscordError) -> MessageRef {
    match error {
        DiscordError::Connect(text) => messages::network_failed(text),
        DiscordError::RateLimited { retry_after_secs } => messages::rate_limited(*retry_after_secs),
        DiscordError::WebhookNotFound { name } => messages::webhook_not_found(name),
        DiscordError::Validation(fault) => validation_message(fault),
        DiscordError::Serde(source) => messages::serialization_failed(&source.to_string()),
        DiscordError::Credential(text) => messages::credential_failed(text),
        DiscordError::BadResponse { status, .. } => messages::http_rejected(*status),
    }
}

fn validation_message(fault: &ValidationFault) -> MessageRef {
    match fault {
        ValidationFault::ColorOutOfRange { color } => messages::embed_color_out_of_range(*color),
        ValidationFault::TooManyFields { count, max } => {
            messages::embed_too_many_fields(*count, *max)
        }
        ValidationFault::PartTooLong { part, length, max } => {
            messages::embed_part_too_long(part, *length, *max)
        }
        ValidationFault::TotalTooLong { total, max } => {
            messages::embed_total_too_long(*total, *max)
        }
        ValidationFault::ControlCharacter { part } => messages::embed_control_character(part),
        ValidationFault::MessageIdNotSnowflake => messages::message_id_invalid(),
        ValidationFault::NotWebhookEndpoint => messages::not_webhook_endpoint(),
        ValidationFault::WebhookNameEmpty => messages::webhook_name_empty(),
        ValidationFault::ContentTooLong { length, max } => {
            messages::content_too_long(*length, *max)
        }
        ValidationFault::NothingToEdit => messages::nothing_to_edit(),
        ValidationFault::BodyEncoding(text) => messages::body_encoding_failed(text),
    }
}

pub(crate) fn failed(error: &DiscordError) -> SubActionOutcome {
    SubActionOutcome::Failed(message(error).into())
}

pub(crate) fn outcome<T>(result: &Result<T, DiscordError>) -> SubActionOutcome {
    match result {
        Ok(_) => SubActionOutcome::Success,
        Err(error) => failed(error),
    }
}
#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_types::{MessageArg, UserText, VerbatimText};

    use super::*;

    type FaultCase = (
        ValidationFault,
        &'static str,
        Vec<(&'static str, MessageArg)>,
    );

    #[test]
    fn every_discord_error_variant_maps_to_its_own_code_with_the_upstream_text_as_detail() {
        let serde = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
        let serde_text = serde.to_string();
        let cases: Vec<(DiscordError, &str, Option<String>)> = vec![
            (
                DiscordError::Connect("dns failure".to_owned()),
                "msg-discord-network_failed",
                Some("dns failure".to_owned()),
            ),
            (
                DiscordError::RateLimited {
                    retry_after_secs: 2.5,
                },
                "msg-discord-rate_limited",
                None,
            ),
            (
                DiscordError::WebhookNotFound {
                    name: "alerts".to_owned(),
                },
                "msg-discord-webhook_not_found",
                None,
            ),
            (
                DiscordError::Validation(ValidationFault::WebhookNameEmpty),
                "msg-discord-webhook_name_empty",
                None,
            ),
            (
                DiscordError::Serde(serde),
                "msg-discord-serialization_failed",
                Some(serde_text),
            ),
            (
                DiscordError::Credential("keystore locked".to_owned()),
                "msg-discord-credential_failed",
                Some("keystore locked".to_owned()),
            ),
            (
                DiscordError::BadResponse {
                    status: 400,
                    body: "Invalid Form Body".to_owned(),
                },
                "msg-discord-http_rejected",
                None,
            ),
        ];

        for (error, expected_key, expected_detail) in cases {
            let mapped = message(&error);

            assert_eq!(mapped.code().ftl_key(), expected_key, "{error}");
            assert_eq!(
                mapped.detail().map(|d| d.as_str().to_owned()),
                expected_detail,
                "{error}"
            );
        }
    }

    #[test]
    fn every_validation_fault_maps_to_its_own_code_with_typed_arguments() {
        let verbatim = |text: &str| MessageArg::Verbatim(VerbatimText::new(text));
        let cases: Vec<FaultCase> = vec![
            (
                ValidationFault::ColorOutOfRange { color: 0x1000000 },
                "msg-discord-embed_color_out_of_range",
                vec![("color", verbatim("0x1000000"))],
            ),
            (
                ValidationFault::TooManyFields { count: 26, max: 25 },
                "msg-discord-embed_too_many_fields",
                vec![("count", MessageArg::Int(26)), ("max", MessageArg::Int(25))],
            ),
            (
                ValidationFault::PartTooLong {
                    part: "title",
                    length: 300,
                    max: 256,
                },
                "msg-discord-embed_part_too_long",
                vec![
                    ("part", verbatim("title")),
                    ("length", MessageArg::Int(300)),
                    ("max", MessageArg::Int(256)),
                ],
            ),
            (
                ValidationFault::TotalTooLong {
                    total: 7000,
                    max: 6000,
                },
                "msg-discord-embed_total_too_long",
                vec![
                    ("total", MessageArg::Int(7000)),
                    ("max", MessageArg::Int(6000)),
                ],
            ),
            (
                ValidationFault::ControlCharacter { part: "footer" },
                "msg-discord-embed_control_character",
                vec![("part", verbatim("footer"))],
            ),
            (
                ValidationFault::MessageIdNotSnowflake,
                "msg-discord-message_id_invalid",
                vec![],
            ),
            (
                ValidationFault::NotWebhookEndpoint,
                "msg-discord-not_webhook_endpoint",
                vec![],
            ),
            (
                ValidationFault::WebhookNameEmpty,
                "msg-discord-webhook_name_empty",
                vec![],
            ),
            (
                ValidationFault::ContentTooLong {
                    length: 2001,
                    max: 2000,
                },
                "msg-discord-content_too_long",
                vec![
                    ("length", MessageArg::Int(2001)),
                    ("max", MessageArg::Int(2000)),
                ],
            ),
            (
                ValidationFault::NothingToEdit,
                "msg-discord-nothing_to_edit",
                vec![],
            ),
        ];

        for (fault, expected_key, expected_args) in cases {
            let mapped = message(&DiscordError::Validation(fault));

            assert_eq!(mapped.code().ftl_key(), expected_key);
            let args: BTreeMap<&str, MessageArg> = mapped
                .args()
                .map(|(name, value)| (name, value.clone()))
                .collect();
            assert_eq!(args, expected_args.into_iter().collect(), "{expected_key}");
            assert!(mapped.detail().is_none(), "{expected_key}");
        }
    }

    #[test]
    fn a_body_encoding_fault_carries_its_text_as_detail() {
        let mapped = message(&DiscordError::Validation(ValidationFault::BodyEncoding(
            "invalid utf-8".to_owned(),
        )));

        assert_eq!(mapped.code().ftl_key(), "msg-discord-body_encoding_failed");
        assert_eq!(mapped.detail().map(|d| d.as_str()), Some("invalid utf-8"));
    }

    #[test]
    fn a_rate_limit_carries_the_wait_as_a_numeric_argument() {
        let mapped = message(&DiscordError::RateLimited {
            retry_after_secs: 2.5,
        });

        assert!(matches!(mapped.arg("seconds"), Some(MessageArg::Float(_))));
    }

    #[test]
    fn a_bad_response_keeps_the_status_and_drops_the_upstream_body() {
        let mapped = message(&DiscordError::BadResponse {
            status: 502,
            body: "Invalid Form Body: secret-name".to_owned(),
        });

        assert!(mapped.detail().is_none());
        assert_eq!(mapped.arg("status"), Some(&MessageArg::Int(502)));
    }

    #[test]
    fn a_failed_result_becomes_a_catalogued_failure_and_an_ok_result_succeeds() {
        let failed_outcome = outcome::<()>(&Err(DiscordError::Connect("x".to_owned())));
        let ok_outcome = outcome(&Ok(()));

        assert!(matches!(
            failed_outcome,
            SubActionOutcome::Failed(UserText::Message(ref m))
                if m.code().ftl_key() == "msg-discord-network_failed"
        ));
        assert_eq!(ok_outcome, SubActionOutcome::Success);
    }
}
