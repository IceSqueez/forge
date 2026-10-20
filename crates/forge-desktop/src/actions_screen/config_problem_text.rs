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
