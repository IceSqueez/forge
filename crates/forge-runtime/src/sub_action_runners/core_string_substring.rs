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
        "String — Substring"
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
            let into_var = config
                .get("into_var")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("string.result")
                .to_owned();
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
