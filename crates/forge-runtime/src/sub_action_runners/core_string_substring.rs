use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, IntegerEditor, IntegerField, ProducedVariable, RunContext,
    StepTimer, SubActionCategory, SubActionIo, SubActionRunner, TextField, TypedField, ValueBounds,
};
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant, VariantKind,
};

use super::core_string_shared::{INTO_RESULT, INTO_VAR_KEY, source_field};
use crate::core_messages;

const TO_END_INDEX: i64 = -1;

const SOURCE: TextField = source_field("Hello World");

const START_INDEX: IntegerField = IntegerField {
    key: "start_index",
    label: "Start Index (0-based, chars)",
    editor: IntegerEditor::Number,
    default: 0,
    bounds: ValueBounds {
        min: Some(0),
        max: None,
    },
};

const END_INDEX: IntegerField = IntegerField {
    key: "end_index",
    label: "End Index (-1 = to end, exclusive)",
    editor: IntegerEditor::Number,
    default: TO_END_INDEX,
    bounds: ValueBounds {
        min: Some(TO_END_INDEX),
        max: None,
    },
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SOURCE, &START_INDEX, &END_INDEX, &INTO_RESULT]);

struct SubstringConfig {
    source: String,
    start: usize,
    end: Option<usize>,
    into_var: String,
}

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<SubstringConfig, ConfigProblem> {
    Ok(SubstringConfig {
        source: SOURCE.read(config, args)?,
        start: usize::try_from(START_INDEX.read(config, args)?).unwrap_or(usize::MAX),
        end: usize::try_from(END_INDEX.read(config, args)?).ok(),
        into_var: INTO_RESULT.read(config, args)?,
    })
}

pub struct CoreStringSubstringRunner;

#[async_trait]
impl SubActionRunner for CoreStringSubstringRunner {
    fn id(&self) -> &str {
        "core.string.substring"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "String - Substring"
    }

    fn summary(&self) -> &str {
        "Extract a slice of a string by char index range"
    }

    fn search_text(&self) -> &str {
        "string substring slice extract range index"
    }

    fn icon_name(&self) -> &str {
        "text-recognition"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::String,
                label: "Substring".to_owned(),
            }],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let SubstringConfig {
            source,
            start,
            end,
            into_var,
        } = match read_config(config, ctx.arg_stack) {
            Ok(read) => read,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let chars: Vec<char> = source.chars().collect();
        let char_count = chars.len();
        let end = end.unwrap_or(char_count);

        let outcome = if start > char_count {
            SubActionOutcome::Failed(
                core_messages::substring_start_out_of_range(start, char_count).into(),
            )
        } else if end > char_count {
            SubActionOutcome::Failed(
                core_messages::substring_end_out_of_range(end, char_count).into(),
            )
        } else if start > end {
            SubActionOutcome::Failed(core_messages::substring_start_after_end(start, end).into())
        } else {
            let slice: String = chars[start..end].iter().collect();
            let new_stack = ctx.arg_stack.clone().set(into_var, Variant::String(slice));
            return (timer.success(), Some(new_stack));
        };

        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    fn cfg(source: &str, start: i64, end: i64) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("source".to_owned(), Variant::String(source.to_owned()));
        c.insert("start_index".to_owned(), Variant::Int(start));
        c.insert("end_index".to_owned(), Variant::Int(end));
        c
    }

    async fn run(cfg: &SubActionConfig) -> (SubActionTelemetry, Option<ArgStack>) {
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        CoreStringSubstringRunner.execute(cfg, &ctx).await
    }

    #[tokio::test]
    async fn substring_indices_count_chars_not_bytes() {
        let out = run(&cfg("Привіт", 0, 3)).await.1.unwrap();
        assert_eq!(
            out.get("string.result").and_then(|v| v.as_str()),
            Some("При")
        );
    }

    #[tokio::test]
    async fn substring_end_minus_one_extends_to_string_end() {
        let out = run(&cfg("hello", 2, -1)).await.1.unwrap();
        assert_eq!(
            out.get("string.result").and_then(|v| v.as_str()),
            Some("llo")
        );
    }

    #[tokio::test]
    async fn substring_out_of_range_or_inverted_bounds_fail_without_panic() {
        for (start, end) in [(6, 7), (0, 9), (3, 1)] {
            let (tel, stack) = run(&cfg("hello", start, end)).await;
            assert!(
                matches!(tel.outcome, SubActionOutcome::Failed(_)),
                "expected Failed for start={start} end={end}"
            );
            assert!(
                stack.is_none(),
                "no stack on failure for start={start} end={end}"
            );
        }
    }

    #[tokio::test]
    async fn substring_writes_to_into_var_override() {
        let mut c = cfg("hello", 0, 2);
        c.insert("into_var".to_owned(), Variant::String("slice".to_owned()));
        let out = run(&c).await.1.unwrap();
        assert_eq!(out.get("slice").and_then(|v| v.as_str()), Some("he"));
        assert!(out.get("string.result").is_none());
    }

    #[test]
    fn validate_config_rejects_negative_start_and_below_minus_one_end() {
        let runner = CoreStringSubstringRunner;
        assert!(runner.validate_config(&cfg("x", -1, 5)).is_err());
        assert!(runner.validate_config(&cfg("x", 0, -2)).is_err());
        assert!(runner.validate_config(&cfg("x", 0, -1)).is_ok());
        assert!(runner.validate_config(&cfg("x", 0, 5)).is_ok());
    }

    #[tokio::test]
    async fn substring_negative_indices_fail_at_run_time_naming_the_index_and_its_bound() {
        for (start, end, named) in [
            (-1, 3, "'start_index' must be at least 0"),
            (i64::MIN, -1, "'start_index' must be at least 0"),
            (0, -2, "'end_index' must be at least -1"),
            (0, i64::MIN, "'end_index' must be at least -1"),
        ] {
            let (tel, stack) = run(&cfg("hello", start, end)).await;
            assert!(
                matches!(&tel.outcome, SubActionOutcome::Failed(reason) if reason.code_or_text().contains(named)),
                "start={start} end={end}: {:?}",
                tel.outcome
            );
            assert!(stack.is_none(), "start={start} end={end}");
        }
    }
}
