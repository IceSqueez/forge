use std::fmt;

use forge_types::MessageRef;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValueBounds {
    pub min: Option<i64>,
    pub max: Option<i64>,
}

impl ValueBounds {
    pub const UNBOUNDED: Self = Self {
        min: None,
        max: None,
    };

    pub fn admits_whole(&self, value: i64) -> bool {
        self.min.is_none_or(|min| value >= min) && self.max.is_none_or(|max| value <= max)
    }

    pub fn admits(&self, value: f64) -> bool {
        self.min.is_none_or(|min| value >= min as f64)
            && self.max.is_none_or(|max| value <= max as f64)
    }

    pub fn between(min: impl TryInto<i64>, max: impl TryInto<i64>) -> Self {
        Self {
            min: Some(saturating_bound(min)),
            max: Some(saturating_bound(max)),
        }
    }

    pub fn at_least(min: impl TryInto<i64>) -> Self {
        Self {
            min: Some(saturating_bound(min)),
            max: None,
        }
    }

    pub fn at_most(max: impl TryInto<i64>) -> Self {
        Self {
            min: None,
            max: Some(saturating_bound(max)),
        }
    }
}

fn saturating_bound(value: impl TryInto<i64>) -> i64 {
    value.try_into().unwrap_or(i64::MAX)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigReason {
    Required,
    OutOfRange(ValueBounds),
    LengthOutOfRange(ValueBounds),
    CountOutOfRange(ValueBounds),
    NotANumber,
    NotText,
    InvalidChoice(Vec<String>),
    AnyOfRequired(Vec<String>),
    Coded(MessageRef),
}

impl ConfigReason {
    pub fn choices<I, S>(choices: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::InvalidChoice(choices.into_iter().map(Into::into).collect())
    }

    pub fn any_of<I, S>(fields: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::AnyOfRequired(fields.into_iter().map(Into::into).collect())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigProblem {
    pub field: Option<String>,
    pub reason: ConfigReason,
}

impl ConfigProblem {
    pub fn field(field: impl Into<String>, reason: ConfigReason) -> Self {
        Self {
            field: Some(field.into()),
            reason,
        }
    }

    pub fn whole(reason: ConfigReason) -> Self {
        Self {
            field: None,
            reason,
        }
    }
}

impl fmt::Display for ValueBounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.min, self.max) {
            (Some(min), Some(max)) => write!(f, "between {min} and {max}"),
            (Some(min), None) => write!(f, "at least {min}"),
            (None, Some(max)) => write!(f, "at most {max}"),
            (None, None) => f.write_str("within its allowed range"),
        }
    }
}

impl fmt::Display for ConfigReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Required => f.write_str("is required"),
            Self::OutOfRange(bounds) => write!(f, "must be {bounds}"),
            Self::LengthOutOfRange(bounds) => write!(f, "must be {bounds} characters long"),
            Self::CountOutOfRange(bounds) => write!(f, "must list {bounds} items"),
            Self::NotANumber => f.write_str("must be a number"),
            Self::NotText => f.write_str("must be text"),
            Self::InvalidChoice(choices) => write!(f, "must be one of: {}", choices.join(", ")),
            Self::AnyOfRequired(fields) => {
                write!(f, "needs at least one of: {}", fields.join(", "))
            }
            Self::Coded(message) => write!(f, "is invalid: {}", message.code()),
        }
    }
}

impl fmt::Display for ConfigProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.field, &self.reason) {
            (Some(field), reason) => write!(f, "'{field}' {reason}"),
            (None, reason @ ConfigReason::AnyOfRequired(_)) => write!(f, "step {reason}"),
            (None, reason) => write!(f, "value {reason}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_messages::{invalid_id, step_rejected};
    use crate::error::RegistryError;

    #[test]
    fn display_prefixes_the_field_the_word_value_or_the_word_step_before_each_reason() {
        let keyed = |reason| ConfigProblem::field("k", reason);
        for (problem, expected) in [
            (keyed(ConfigReason::Required), "'k' is required"),
            (
                ConfigProblem::whole(ConfigReason::Required),
                "value is required",
            ),
            (keyed(invalid_id()), "'k' is invalid: config.invalid_id"),
            (
                ConfigProblem::whole(invalid_id()),
                "value is invalid: config.invalid_id",
            ),
            (
                keyed(ConfigReason::OutOfRange(ValueBounds::between(1, 10))),
                "'k' must be between 1 and 10",
            ),
            (
                keyed(ConfigReason::OutOfRange(ValueBounds::at_least(-1))),
                "'k' must be at least -1",
            ),
            (
                keyed(ConfigReason::OutOfRange(ValueBounds::at_most(5))),
                "'k' must be at most 5",
            ),
            (
                keyed(ConfigReason::OutOfRange(ValueBounds {
                    min: None,
                    max: None,
                })),
                "'k' must be within its allowed range",
            ),
            (
                keyed(ConfigReason::LengthOutOfRange(ValueBounds::at_most(100))),
                "'k' must be at most 100 characters long",
            ),
            (keyed(ConfigReason::NotANumber), "'k' must be a number"),
            (keyed(ConfigReason::NotText), "'k' must be text"),
            (
                keyed(ConfigReason::CountOutOfRange(ValueBounds::at_most(25))),
                "'k' must list at most 25 items",
            ),
            (
                ConfigProblem::whole(ConfigReason::any_of(["title", "cost"])),
                "step needs at least one of: title, cost",
            ),
            (
                keyed(ConfigReason::choices(["public", "unlisted"])),
                "'k' must be one of: public, unlisted",
            ),
        ] {
            assert_eq!(problem.to_string(), expected);
        }
    }

    #[test]
    fn bounds_beyond_i64_saturate_to_i64_max_while_in_range_values_pass_through() {
        assert_eq!(
            ValueBounds::between(-3_i32, usize::MAX),
            ValueBounds {
                min: Some(-3),
                max: Some(i64::MAX),
            }
        );
        assert_eq!(ValueBounds::at_least(u64::MAX).min, Some(i64::MAX));
    }

    #[test]
    fn admits_whole_includes_each_bound_and_excludes_one_past_it() {
        let bounded = ValueBounds::between(1, 10);
        for (bounds, value, admitted) in [
            (bounded, 0, false),
            (bounded, 1, true),
            (bounded, 10, true),
            (bounded, 11, false),
            (ValueBounds::at_least(1), i64::MAX, true),
            (ValueBounds::at_least(1), 0, false),
            (ValueBounds::at_most(10), i64::MIN, true),
            (ValueBounds::at_most(10), 11, false),
            (ValueBounds::UNBOUNDED, i64::MIN, true),
            (ValueBounds::UNBOUNDED, i64::MAX, true),
        ] {
            assert_eq!(
                bounds.admits_whole(value),
                admitted,
                "{bounds:?} with {value}"
            );
        }
    }

    #[test]
    fn admits_includes_each_bound_and_excludes_fractions_past_it() {
        let bounded = ValueBounds::between(1, 10);
        for (bounds, value, admitted) in [
            (bounded, 0.999, false),
            (bounded, 1.0, true),
            (bounded, 5.5, true),
            (bounded, 10.0, true),
            (bounded, 10.001, false),
            (bounded, f64::NAN, false),
            (ValueBounds::at_least(1), f64::MAX, true),
            (ValueBounds::at_most(10), f64::MIN, true),
        ] {
            assert_eq!(bounds.admits(value), admitted, "{bounds:?} with {value}");
        }
    }

    #[test]
    fn into_config_problem_keeps_an_invalid_config_problem_intact() {
        let problem = ConfigProblem::field("volume", ConfigReason::NotANumber);

        assert_eq!(
            RegistryError::InvalidConfig(problem.clone()).into_config_problem(),
            problem
        );
    }

    #[test]
    fn into_config_problem_folds_other_errors_into_a_whole_config_rejection() {
        for error in [
            RegistryError::DuplicateId("a.b".to_owned()),
            RegistryError::UnknownKindId("a.b".to_owned()),
            RegistryError::DepthExceeded(9),
            RegistryError::InvalidRemap {
                namespace: "action".to_owned(),
                value: "x".to_owned(),
            },
            RegistryError::RunnerInit("boom".to_owned()),
        ] {
            assert_eq!(
                error.into_config_problem(),
                ConfigProblem::whole(step_rejected())
            );
        }
    }
}
