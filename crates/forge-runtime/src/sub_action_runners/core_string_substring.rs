use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String(String::new()));
        cfg.insert("start_index".to_owned(), Variant::Int(0));
        cfg.insert("end_index".to_owned(), Variant::Int(-1));
        cfg.insert(
            "into_var".to_owned(),
            Variant::String("string.result".to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "source",
                label: "Source",
                placeholder: "Hello World",
            },
            FormField::Integer {
                key: "start_index",
                label: "Start Index (0-based, chars)",
                min: 0,
                max: i64::MAX,
            },
            FormField::Integer {
                key: "end_index",
                label: "End Index (-1 = to end, exclusive)",
                min: -1,
                max: i64::MAX,
            },
            FormField::Text {
                key: "into_var",
                label: "Output Variable",
                placeholder: "string.result",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let start = config
            .get("start_index")
            .and_then(|v| v.as_int())
            .unwrap_or(0);
        if start < 0 {
            return Err(RegistryError::UnknownKindId(
                "core.string.substring: start_index must be >= 0".to_owned(),
            ));
        }
        let end = config
            .get("end_index")
            .and_then(|v| v.as_int())
            .unwrap_or(-1);
        if end < -1 {
            return Err(RegistryError::UnknownKindId(
                "core.string.substring: end_index must be -1 (to end) or >= 0".to_owned(),
            ));
        }
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let source = config.get("source").and_then(|v| v.as_str()).unwrap_or("");

        let start = config
            .get("start_index")
            .and_then(|v| v.as_int())
            .unwrap_or(0)
            .max(0) as usize;

        let end_raw = config
            .get("end_index")
            .and_then(|v| v.as_int())
            .unwrap_or(-1);

        let chars: Vec<char> = source.chars().collect();
        let char_count = chars.len();

        // -1 means extend to end of string; any other negative value is rejected by validate_config.
        let end = if end_raw == -1 {
            char_count
        } else {
            end_raw as usize
        };

        let outcome = if start > char_count {
            SubActionOutcome::Failed(format!(
                "start_index {start} exceeds string length {char_count}"
            ))
        } else if end > char_count {
            SubActionOutcome::Failed(format!(
                "end_index {end} exceeds string length {char_count}"
            ))
        } else if start > end {
            SubActionOutcome::Failed(format!(
                "start_index {start} is greater than end_index {end}"
            ))
        } else {
            let slice: String = chars[start..end].iter().collect();
            let into_var = super::interpolate::sanitize_var_name(
                config
                    .get("into_var")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("string.result"),
            );
            let new_stack = ctx.arg_stack.clone().set(into_var, Variant::String(slice));
            let duration_ms = (OffsetDateTime::now_utc() - started_at)
                .whole_milliseconds()
                .max(0) as u64;
            return (
                SubActionTelemetry {
                    index: ctx.index,
                    kind: "core.string.substring".to_owned(),
                    started_at,
                    duration_ms,
                    outcome: SubActionOutcome::Success,
                },
                Some(new_stack),
            );
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.string.substring".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
        // "Привіт" is 12 UTF-8 bytes; a byte-slice [0..3] would split a codepoint.
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
        // (start > len), (end > len), (start > end) - each rejected, no slice emitted.
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
        // start < 0 and end < -1 are illegal; start>=0 with end==-1 or end>=0 are legal.
        assert!(runner.validate_config(&cfg("x", -1, 5)).is_err());
        assert!(runner.validate_config(&cfg("x", 0, -2)).is_err());
        assert!(runner.validate_config(&cfg("x", 0, -1)).is_ok());
        assert!(runner.validate_config(&cfg("x", 0, 5)).is_ok());
    }
}
