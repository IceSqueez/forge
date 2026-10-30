use forge_registry::{ConfigProblem, ConfigReason, FormField};
use forge_types::{MessageArg, VerbatimText};

use crate::config_field_label::form_field_label;
use crate::user_text::resolve_message;

const FIELD_NAME_JOINER: &str = ", ";

pub(crate) fn config_problem_text(
    problem: &ConfigProblem,
    kind_id: &str,
    fields: &[FormField],
) -> String {
    resolve_message(&problem.message(|keys| field_names(keys, kind_id, fields))).inline()
}

pub(super) fn config_reason_text(
    reason: &ConfigReason,
    kind_id: &str,
    fields: &[FormField],
) -> String {
    resolve_message(&reason.message(|keys| field_names(keys, kind_id, fields))).inline()
}

fn field_names(keys: &[String], kind_id: &str, fields: &[FormField]) -> MessageArg {
    let names: Vec<String> = keys
        .iter()
        .map(|key| form_field_label(kind_id, fields, key).unwrap_or_else(|| key.clone()))
        .collect();
    VerbatimText::new(names.join(FIELD_NAME_JOINER)).into()
}

#[cfg(test)]
mod tests {
    use forge_registry::ValueBounds;
    use forge_storage::Language;
    use forge_types::{MessageCode, MessageRef, UpstreamDetail};

    use super::*;
    use crate::i18n::install_language;

    fn plain(rendered: String) -> String {
        rendered
            .chars()
            .filter(|c| !matches!(c, '\u{2068}' | '\u{2069}'))
            .collect()
    }

    fn reason_in(language: Language, reason: ConfigReason) -> String {
        install_language(language);
        plain(config_reason_text(&reason, "test.step", &[]))
    }

    fn coded(name: &'static str) -> ConfigReason {
        ConfigReason::Coded(MessageRef::new(MessageCode::from_static("config", name)))
    }

    fn not_json_with_cause() -> MessageRef {
        MessageRef::new(MessageCode::from_static("config", "not_json"))
            .with_detail(UpstreamDetail::from_redacted("expected value"))
    }

    fn count(bounds: ValueBounds) -> ConfigReason {
        ConfigReason::CountOutOfRange(bounds)
    }

    fn form() -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "title",
                label: "Stream title",
                placeholder: "",
            },
            FormField::Text {
                key: "category_id",
                label: "Category ID (numeric)",
                placeholder: "",
            },
            FormField::Text {
                key: "scene",
                label: "Scene",
                placeholder: "",
            },
        ]
    }

    fn range(bounds: ValueBounds) -> ConfigReason {
        ConfigReason::OutOfRange(bounds)
    }

    fn length(bounds: ValueBounds) -> ConfigReason {
        ConfigReason::LengthOutOfRange(bounds)
    }

    const UNBOUNDED: ValueBounds = ValueBounds {
        min: None,
        max: None,
    };

    #[test]
    fn every_reason_renders_its_own_english_sentence() {
        for (reason, expected) in [
            (ConfigReason::Required, "Required"),
            (ConfigReason::NotANumber, "Must be a number"),
            (
                range(ValueBounds::between(1, 10080)),
                "Must be between 1 and 10080",
            ),
            (range(ValueBounds::at_least(-1)), "Must be at least -1"),
            (range(ValueBounds::at_most(100)), "Must be at most 100"),
            (range(UNBOUNDED), "Out of the allowed range"),
            (
                length(ValueBounds::between(1, 140)),
                "Must be 1 to 140 characters long",
            ),
            (
                length(ValueBounds::at_least(1)),
                "Must be at least 1 character long",
            ),
            (
                length(ValueBounds::at_most(500)),
                "Must be at most 500 characters long",
            ),
            (length(UNBOUNDED), "Out of the allowed range"),
            (
                ConfigReason::choices(["public", "unlisted", "private"]),
                "Must be one of: public, unlisted, private",
            ),
            (
                coded("not_a_datetime"),
                "Must be a date and time or a unix timestamp",
            ),
            (
                ConfigReason::Coded(not_json_with_cause()),
                "Must be valid JSON (expected value)",
            ),
            (ConfigReason::NotText, "Must be text"),
            (
                count(ValueBounds::between(2, 10)),
                "Must list 2 to 10 items",
            ),
            (count(ValueBounds::at_least(1)), "Must list at least 1 item"),
            (
                count(ValueBounds::at_least(2)),
                "Must list at least 2 items",
            ),
            (count(ValueBounds::at_most(1)), "Must list at most 1 item"),
            (
                count(ValueBounds::at_most(25)),
                "Must list at most 25 items",
            ),
            (count(UNBOUNDED), "Out of the allowed range"),
        ] {
            assert_eq!(
                reason_in(Language::En, reason.clone()),
                expected,
                "{reason:?}"
            );
        }
    }

    #[test]
    fn every_reason_renders_its_own_ukrainian_sentence() {
        for (reason, expected) in [
            (ConfigReason::Required, "Обов'язкове поле"),
            (ConfigReason::NotANumber, "Має бути числом"),
            (range(ValueBounds::between(1, 10)), "Має бути від 1 до 10"),
            (range(ValueBounds::at_least(-1)), "Має бути не менше ніж -1"),
            (
                range(ValueBounds::at_most(100)),
                "Має бути не більше ніж 100",
            ),
            (range(UNBOUNDED), "Значення поза допустимими межами"),
            (
                length(ValueBounds::between(1, 140)),
                "Довжина має бути від 1 до 140 символів",
            ),
            (
                ConfigReason::choices(["public", "private"]),
                "Допустимі значення: public, private",
            ),
            (coded("not_json"), "Має бути коректним JSON"),
            (ConfigReason::NotText, "Має бути текстом"),
            (
                count(ValueBounds::between(1, 5)),
                "Має містити від 1 до 5 елементів",
            ),
        ] {
            assert_eq!(
                reason_in(Language::Uk, reason.clone()),
                expected,
                "{reason:?}"
            );
        }
    }

    #[test]
    fn ukrainian_length_bounds_pick_the_plural_form_of_the_count() {
        for (count, at_least, at_most) in [
            (1, "Щонайменше 1 символ", "Щонайбільше 1 символ"),
            (2, "Щонайменше 2 символи", "Щонайбільше 2 символи"),
            (5, "Щонайменше 5 символів", "Щонайбільше 5 символів"),
        ] {
            assert_eq!(
                reason_in(Language::Uk, length(ValueBounds::at_least(count))),
                at_least
            );
            assert_eq!(
                reason_in(Language::Uk, length(ValueBounds::at_most(count))),
                at_most
            );
        }
    }

    #[test]
    fn ukrainian_count_bounds_pick_the_plural_form_of_the_count() {
        for (n, at_least, at_most) in [
            (1, "Щонайменше 1 елемент", "Щонайбільше 1 елемент"),
            (2, "Щонайменше 2 елементи", "Щонайбільше 2 елементи"),
            (5, "Щонайменше 5 елементів", "Щонайбільше 5 елементів"),
        ] {
            assert_eq!(
                reason_in(Language::Uk, count(ValueBounds::at_least(n))),
                at_least
            );
            assert_eq!(
                reason_in(Language::Uk, count(ValueBounds::at_most(n))),
                at_most
            );
        }
    }

    #[test]
    fn any_of_required_names_each_field_by_its_form_label_or_its_key_when_off_the_form() {
        let reason = ConfigReason::any_of(["title", "category_id", "tags"]);
        for (language, expected) in [
            (
                Language::En,
                "Fill in at least one of: Stream title, Category ID, tags",
            ),
            (
                Language::Uk,
                "Заповніть хоча б одне з полів: Stream title, Category ID, tags",
            ),
        ] {
            install_language(language);
            assert_eq!(
                plain(config_reason_text(&reason, "test.step", &form())),
                expected
            );
        }
    }

    #[test]
    fn english_length_bounds_pick_singular_only_for_one() {
        for (count, expected) in [
            (1, "Must be at most 1 character long"),
            (2, "Must be at most 2 characters long"),
        ] {
            assert_eq!(
                reason_in(Language::En, length(ValueBounds::at_most(count))),
                expected
            );
        }
    }

    #[test]
    fn a_problem_names_its_field_by_label_after_the_reason_and_a_whole_config_problem_does_not() {
        install_language(Language::En);
        for (problem, expected) in [
            (
                ConfigProblem::field("scene", ConfigReason::Required),
                "Required (Scene)",
            ),
            (
                ConfigProblem::field("volume", ConfigReason::Required),
                "Required (volume)",
            ),
            (ConfigProblem::whole(ConfigReason::Required), "Required"),
            (
                ConfigProblem::field("scene", ConfigReason::Coded(not_json_with_cause())),
                "Must be valid JSON (Scene) (expected value)",
            ),
        ] {
            assert_eq!(
                plain(config_problem_text(&problem, "test.step", &form())),
                expected
            );
        }
    }
}
