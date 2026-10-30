use std::fmt::Display;
use std::slice;

use forge_types::{
    MessageArg, MessageCatalog, MessageCode, MessageRef, UpstreamDetail, VerbatimText,
};

use crate::config_problem::{ConfigProblem, ConfigReason, ValueBounds};

const CONFIG_NAMESPACE: &str = "config";

const MIN_ARG: &str = "min";
const MAX_ARG: &str = "max";
const CHOICES_ARG: &str = "choices";
const FIELDS_ARG: &str = "fields";
const FIELD_ARG: &str = "field";
const PROBLEM_ARG: &str = "problem";
const LIST_JOINER: &str = ", ";

const REQUIRED: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "required");
const NOT_A_NUMBER: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "not_a_number");
const NOT_TEXT: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "not_text");
const BETWEEN: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "between");
const AT_LEAST: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "at_least");
const AT_MOST: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "at_most");
const LENGTH_BETWEEN: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "length_between");
const LENGTH_AT_LEAST: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "length_at_least");
const LENGTH_AT_MOST: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "length_at_most");
const COUNT_BETWEEN: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "count_between");
const COUNT_AT_LEAST: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "count_at_least");
const COUNT_AT_MOST: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "count_at_most");
const OUT_OF_RANGE: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "out_of_range");
const ANY_OF_REQUIRED: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "any_of_required");
const INVALID_CHOICE: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "invalid_choice");
const FIELD_PROBLEM: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "field_problem");
const UNKNOWN_ENTRY: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "unknown_entry");
const NOT_A_DATETIME: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "not_a_datetime");
const TIMESTAMP_OUT_OF_RANGE: MessageCode =
    MessageCode::from_static(CONFIG_NAMESPACE, "timestamp_out_of_range");
const NOT_A_STEP_LIST: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "not_a_step_list");
const NOT_JSON: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "not_json");
const NOT_A_JSON_OBJECT: MessageCode =
    MessageCode::from_static(CONFIG_NAMESPACE, "not_a_json_object");
const INVALID_ID: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "invalid_id");
const STEP_REJECTED: MessageCode = MessageCode::from_static(CONFIG_NAMESPACE, "step_rejected");

pub const CONFIG_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: CONFIG_NAMESPACE,
    codes: &[
        REQUIRED,
        NOT_A_NUMBER,
        NOT_TEXT,
        BETWEEN,
        AT_LEAST,
        AT_MOST,
        LENGTH_BETWEEN,
        LENGTH_AT_LEAST,
        LENGTH_AT_MOST,
        COUNT_BETWEEN,
        COUNT_AT_LEAST,
        COUNT_AT_MOST,
        OUT_OF_RANGE,
        ANY_OF_REQUIRED,
        INVALID_CHOICE,
        FIELD_PROBLEM,
        UNKNOWN_ENTRY,
        NOT_A_DATETIME,
        TIMESTAMP_OUT_OF_RANGE,
        NOT_A_STEP_LIST,
        NOT_JSON,
        NOT_A_JSON_OBJECT,
        INVALID_ID,
        STEP_REJECTED,
    ],
    en_ftl: include_str!("../locales/en/config.ftl"),
    uk_ftl: include_str!("../locales/uk/config.ftl"),
};

pub fn invalid_id() -> ConfigReason {
    ConfigReason::Coded(MessageRef::new(INVALID_ID))
}

pub(crate) fn unknown_entry() -> ConfigReason {
    ConfigReason::Coded(MessageRef::new(UNKNOWN_ENTRY))
}

pub(crate) fn not_a_datetime() -> ConfigReason {
    ConfigReason::Coded(MessageRef::new(NOT_A_DATETIME))
}

pub(crate) fn timestamp_out_of_range() -> ConfigReason {
    ConfigReason::Coded(MessageRef::new(TIMESTAMP_OUT_OF_RANGE))
}

pub(crate) fn not_a_step_list() -> ConfigReason {
    ConfigReason::Coded(MessageRef::new(NOT_A_STEP_LIST))
}

pub(crate) fn not_json(cause: impl Display) -> ConfigReason {
    ConfigReason::Coded(
        MessageRef::new(NOT_JSON).with_detail(UpstreamDetail::from_redacted(cause.to_string())),
    )
}

pub(crate) fn not_a_json_object() -> ConfigReason {
    ConfigReason::Coded(MessageRef::new(NOT_A_JSON_OBJECT))
}

pub(crate) fn step_rejected() -> ConfigReason {
    ConfigReason::Coded(MessageRef::new(STEP_REJECTED))
}

impl ConfigProblem {
    pub fn message(&self, field_names: impl Fn(&[String]) -> MessageArg) -> MessageRef {
        let reason = self.reason.message(&field_names);
        let Some(field) = &self.field else {
            return reason;
        };
        let detail = reason.detail().cloned();
        let wrapped = MessageRef::new(FIELD_PROBLEM)
            .with_arg(FIELD_ARG, field_names(slice::from_ref(field)))
            .with_arg(PROBLEM_ARG, reason);
        match detail {
            Some(detail) => wrapped.with_detail(detail),
            None => wrapped,
        }
    }
}

impl ConfigReason {
    pub fn message(&self, field_names: impl Fn(&[String]) -> MessageArg) -> MessageRef {
        match self {
            Self::Required => MessageRef::new(REQUIRED),
            Self::OutOfRange(bounds) => bounds_message(bounds, [BETWEEN, AT_LEAST, AT_MOST]),
            Self::LengthOutOfRange(bounds) => {
                bounds_message(bounds, [LENGTH_BETWEEN, LENGTH_AT_LEAST, LENGTH_AT_MOST])
            }
            Self::CountOutOfRange(bounds) => {
                bounds_message(bounds, [COUNT_BETWEEN, COUNT_AT_LEAST, COUNT_AT_MOST])
            }
            Self::NotANumber => MessageRef::new(NOT_A_NUMBER),
            Self::NotText => MessageRef::new(NOT_TEXT),
            Self::InvalidChoice(choices) => MessageRef::new(INVALID_CHOICE)
                .with_arg(CHOICES_ARG, VerbatimText::new(choices.join(LIST_JOINER))),
            Self::AnyOfRequired(fields) => {
                MessageRef::new(ANY_OF_REQUIRED).with_arg(FIELDS_ARG, field_names(fields))
            }
            Self::Coded(message) => message.clone(),
        }
    }
}

fn bounds_message(
    bounds: &ValueBounds,
    [between, at_least, at_most]: [MessageCode; 3],
) -> MessageRef {
    match (bounds.min, bounds.max) {
        (Some(min), Some(max)) => MessageRef::new(between)
            .with_arg(MIN_ARG, min)
            .with_arg(MAX_ARG, max),
        (Some(min), None) => MessageRef::new(at_least).with_arg(MIN_ARG, min),
        (None, Some(max)) => MessageRef::new(at_most).with_arg(MAX_ARG, max),
        (None, None) => MessageRef::new(OUT_OF_RANGE),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    const UNBOUNDED: ValueBounds = ValueBounds {
        min: None,
        max: None,
    };

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

    fn verbatim_names(keys: &[String]) -> MessageArg {
        VerbatimText::new(keys.join("+")).into()
    }

    fn rendered(reason: &ConfigReason) -> MessageRef {
        reason.message(verbatim_names)
    }

    fn int_args(message: &MessageRef) -> Vec<(String, MessageArg)> {
        message
            .args()
            .map(|(name, value)| (name.to_owned(), value.clone()))
            .collect()
    }

    fn every_constructor() -> Vec<MessageRef> {
        let cases = [
            ConfigReason::Required,
            ConfigReason::NotANumber,
            ConfigReason::NotText,
            ConfigReason::OutOfRange(ValueBounds::between(1, 2)),
            ConfigReason::OutOfRange(ValueBounds::at_least(1)),
            ConfigReason::OutOfRange(ValueBounds::at_most(2)),
            ConfigReason::OutOfRange(UNBOUNDED),
            ConfigReason::LengthOutOfRange(ValueBounds::between(1, 2)),
            ConfigReason::LengthOutOfRange(ValueBounds::at_least(1)),
            ConfigReason::LengthOutOfRange(ValueBounds::at_most(2)),
            ConfigReason::CountOutOfRange(ValueBounds::between(1, 2)),
            ConfigReason::CountOutOfRange(ValueBounds::at_least(1)),
            ConfigReason::CountOutOfRange(ValueBounds::at_most(2)),
            ConfigReason::any_of(["a", "b"]),
            ConfigReason::choices(["a", "b"]),
            invalid_id(),
            unknown_entry(),
            not_a_datetime(),
            timestamp_out_of_range(),
            not_a_step_list(),
            not_json("c"),
            not_a_json_object(),
            step_rejected(),
        ];
        let mut messages: Vec<MessageRef> = cases.iter().map(rendered).collect();
        messages.push(ConfigProblem::field("k", ConfigReason::Required).message(verbatim_names));
        messages
    }

    #[test]
    fn every_constructor_code_is_catalogued_and_localized_in_both_languages_with_matching_variables()
     {
        let en = ftl_variables(CONFIG_MESSAGES.en_ftl);
        let uk = ftl_variables(CONFIG_MESSAGES.uk_ftl);
        for message in every_constructor() {
            let code = message.code();
            assert!(
                CONFIG_MESSAGES.codes.contains(code),
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
        let catalogued: BTreeSet<MessageCode> = CONFIG_MESSAGES.codes.iter().cloned().collect();
        assert_eq!(produced, catalogued);
    }

    #[test]
    fn english_and_ukrainian_declare_the_same_messages_with_the_same_variables() {
        assert_eq!(
            ftl_variables(CONFIG_MESSAGES.en_ftl),
            ftl_variables(CONFIG_MESSAGES.uk_ftl)
        );
    }

    #[test]
    fn upstream_cause_text_is_carried_as_detail_and_never_as_an_argument() {
        let ConfigReason::Coded(failure) = not_json("expected value at line 1") else {
            panic!("not_json is a coded reason");
        };

        assert_eq!(
            failure.detail().map(|d| d.as_str()),
            Some("expected value at line 1")
        );
        assert_eq!(failure.args().count(), 0);
    }

    #[test]
    fn plain_reasons_map_to_their_own_code_without_arguments() {
        for (reason, code) in [
            (ConfigReason::Required, REQUIRED),
            (ConfigReason::NotANumber, NOT_A_NUMBER),
            (ConfigReason::NotText, NOT_TEXT),
        ] {
            let message = rendered(&reason);

            assert_eq!(message.code(), &code);
            assert_eq!(message.args().count(), 0, "{reason:?}");
        }
    }

    #[test]
    fn each_bounds_shape_picks_its_code_and_passes_only_the_bounds_it_has() {
        let both = |range: i64| {
            vec![
                (MAX_ARG.to_owned(), MessageArg::Int(range + 1)),
                (MIN_ARG.to_owned(), MessageArg::Int(range)),
            ]
        };
        let min_only = vec![(MIN_ARG.to_owned(), MessageArg::Int(3))];
        let max_only = vec![(MAX_ARG.to_owned(), MessageArg::Int(9))];
        for (wrap, codes) in [
            (
                ConfigReason::OutOfRange as fn(ValueBounds) -> ConfigReason,
                [BETWEEN, AT_LEAST, AT_MOST],
            ),
            (
                ConfigReason::LengthOutOfRange,
                [LENGTH_BETWEEN, LENGTH_AT_LEAST, LENGTH_AT_MOST],
            ),
            (
                ConfigReason::CountOutOfRange,
                [COUNT_BETWEEN, COUNT_AT_LEAST, COUNT_AT_MOST],
            ),
        ] {
            let [between, at_least, at_most] = codes;
            for (bounds, code, args) in [
                (ValueBounds::between(1, 2), between, both(1)),
                (ValueBounds::at_least(3), at_least, min_only.clone()),
                (ValueBounds::at_most(9), at_most, max_only.clone()),
                (UNBOUNDED, OUT_OF_RANGE, Vec::new()),
            ] {
                let message = rendered(&wrap(bounds));

                assert_eq!(message.code(), &code);
                assert_eq!(int_args(&message), args, "{bounds:?}");
            }
        }
    }

    #[test]
    fn invalid_choice_lists_the_choices_as_verbatim_text() {
        let message = rendered(&ConfigReason::choices(["public", "unlisted"]));

        assert_eq!(message.code(), &INVALID_CHOICE);
        assert_eq!(
            message.arg(CHOICES_ARG),
            Some(&MessageArg::Verbatim(VerbatimText::new("public, unlisted")))
        );
    }

    #[test]
    fn any_of_required_names_its_fields_through_the_caller() {
        let message = rendered(&ConfigReason::any_of(["title", "cost"]));

        assert_eq!(message.code(), &ANY_OF_REQUIRED);
        assert_eq!(
            message.arg(FIELDS_ARG),
            Some(&MessageArg::Verbatim(VerbatimText::new("title+cost")))
        );
    }

    #[test]
    fn a_coded_reason_renders_as_the_message_it_carries() {
        let carried = MessageRef::new(MessageCode::from_static("probe", "own"))
            .with_arg("n", 7_i64)
            .with_detail(UpstreamDetail::from_redacted("cause"));

        assert_eq!(rendered(&ConfigReason::Coded(carried.clone())), carried);
    }

    #[test]
    fn a_whole_config_problem_is_its_reason_message_unwrapped() {
        let reason = ConfigReason::any_of(["a"]);

        assert_eq!(
            ConfigProblem::whole(reason.clone()).message(verbatim_names),
            rendered(&reason)
        );
    }

    #[test]
    fn a_field_problem_nests_the_reason_message_and_names_the_field_through_the_caller() {
        let reason = ConfigReason::OutOfRange(ValueBounds::at_most(5));

        let message = ConfigProblem::field("volume", reason.clone()).message(verbatim_names);

        assert_eq!(message.code(), &FIELD_PROBLEM);
        assert_eq!(
            message.arg(FIELD_ARG),
            Some(&MessageArg::Verbatim(VerbatimText::new("volume")))
        );
        assert_eq!(
            message.arg(PROBLEM_ARG),
            Some(&MessageArg::Message(Box::new(rendered(&reason))))
        );
        assert!(message.detail().is_none());
    }

    #[test]
    fn a_field_problem_hoists_the_reason_detail_to_the_outer_message() {
        let message =
            ConfigProblem::field("payload", not_json("expected value")).message(verbatim_names);

        assert_eq!(
            message.detail().map(|detail| detail.as_str()),
            Some("expected value")
        );
    }
}
