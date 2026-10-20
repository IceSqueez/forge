use forge_components::tr;
use forge_registry::{ConfigProblem, ConfigReason, ValueBounds};

pub(super) fn config_problem_text(problem: &ConfigProblem) -> String {
    let reason = config_reason_text(&problem.reason);
    match &problem.field {
        Some(field) => tr!(
            "step_config_field_problem",
            field = field.clone(),
            problem = reason
        ),
        None => reason,
    }
}

pub(super) fn config_reason_text(reason: &ConfigReason) -> String {
    match reason {
        ConfigReason::Required => tr!("step_config_required"),
        ConfigReason::OutOfRange(bounds) => bounds_text(bounds, BoundsUnit::Value),
        ConfigReason::LengthOutOfRange(bounds) => bounds_text(bounds, BoundsUnit::Characters),
        ConfigReason::NotANumber => tr!("step_config_not_a_number"),
        ConfigReason::InvalidChoice(choices) => {
            tr!("step_config_invalid_choice", choices = choices.join(", "))
        }
        ConfigReason::InvalidFormat(detail) => {
            tr!("step_config_invalid_format", detail = detail.clone())
        }
        ConfigReason::Custom(text) => text.clone(),
    }
}

enum BoundsUnit {
    Value,
    Characters,
}

fn bounds_text(bounds: &ValueBounds, unit: BoundsUnit) -> String {
    match (bounds.min, bounds.max, unit) {
        (Some(min), Some(max), BoundsUnit::Value) => {
            tr!("step_config_between", min = min, max = max)
        }
        (Some(min), Some(max), BoundsUnit::Characters) => {
            tr!("step_config_length_between", min = min, max = max)
        }
        (Some(min), None, BoundsUnit::Value) => tr!("step_config_at_least", min = min),
        (Some(min), None, BoundsUnit::Characters) => {
            tr!("step_config_length_at_least", min = min)
        }
        (None, Some(max), BoundsUnit::Value) => tr!("step_config_at_most", max = max),
        (None, Some(max), BoundsUnit::Characters) => {
            tr!("step_config_length_at_most", max = max)
        }
        (None, None, _) => tr!("step_config_out_of_range"),
    }
}

#[cfg(test)]
mod tests {
    use forge_storage::Language;

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
        plain(config_reason_text(&reason))
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
                ConfigReason::InvalidFormat("unknown letter q".to_owned()),
                "Invalid format: unknown letter q",
            ),
            (
                ConfigReason::Custom("pick a scene first".to_owned()),
                "pick a scene first",
            ),
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
            (
                ConfigReason::InvalidFormat("q".to_owned()),
                "Неправильний формат: q",
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
    fn a_problem_names_its_field_after_the_reason_and_a_whole_config_problem_does_not() {
        install_language(Language::En);
        for (problem, expected) in [
            (
                ConfigProblem::field("scene", ConfigReason::Required),
                "Required (scene)",
            ),
            (ConfigProblem::whole(ConfigReason::Required), "Required"),
        ] {
            assert_eq!(plain(config_problem_text(&problem)), expected);
        }
    }
}
