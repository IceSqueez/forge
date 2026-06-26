use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::{OffsetDateTime, PrimitiveDateTime};

pub struct CoreTimeParseRunner;

#[async_trait]
impl SubActionRunner for CoreTimeParseRunner {
    fn id(&self) -> &str {
        "core.time.parse"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "Time — Parse Time String"
    }

    fn summary(&self) -> &str {
        "Parse a string into a datetime using a time-crate format description; treated as UTC when the format carries no UTC offset"
    }

    fn search_text(&self) -> &str {
        "time parse string datetime format convert"
    }

    fn icon_name(&self) -> &str {
        "clock-edit"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String(String::new()));
        cfg.insert(
            "format".to_owned(),
            Variant::String("[year]-[month]-[day] [hour]:[minute]:[second]".to_owned()),
        );
        cfg.insert(
            "into_var".to_owned(),
            Variant::String("time.parsed".to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "source",
                label: "Source String",
                placeholder: "2024-01-15 12:00:00",
            },
            FormField::Text {
                key: "format",
                label: "Format Description",
                placeholder: "[year]-[month]-[day] [hour]:[minute]:[second]",
            },
            FormField::Text {
                key: "into_var",
                label: "Output Variable",
                placeholder: "time.parsed",
            },
        ]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let source_template = config
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let source = ctx.arg_stack.interpolate(&source_template);

        let format_str = config
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("[year]-[month]-[day] [hour]:[minute]:[second]")
            .to_owned();
        let into_var = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("time.parsed")
            .to_owned();

        let desc = match time::format_description::parse_borrowed::<2>(&format_str) {
            Ok(d) => d,
            Err(e) => return fail(started_at, ctx.index, format!("invalid format: {e}")),
        };

        // Try OffsetDateTime first (format includes offset), fall back to
        // PrimitiveDateTime + assume UTC for formats without an offset component.
        let dt = OffsetDateTime::parse(&source, &desc)
            .or_else(|_| PrimitiveDateTime::parse(&source, &desc).map(|pdt| pdt.assume_utc()))
            .map_err(|_| format!("cannot parse '{source}' with format '{format_str}'"));

        let dt = match dt {
            Ok(dt) => dt,
            Err(e) => return fail(started_at, ctx.index, e),
        };

        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::Datetime(dt));

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.time.parse".to_owned(),
                started_at,
                duration_ms,
                outcome: SubActionOutcome::Success,
            },
            Some(new_stack),
        )
    }
}

fn fail(
    started_at: OffsetDateTime,
    index: usize,
    msg: String,
) -> (SubActionTelemetry, Option<ArgStack>) {
    let duration_ms = (OffsetDateTime::now_utc() - started_at)
        .whole_milliseconds()
        .max(0) as u64;
    (
        SubActionTelemetry {
            index,
            kind: "core.time.parse".to_owned(),
            started_at,
            duration_ms,
            outcome: SubActionOutcome::Failed(msg),
        },
        None,
    )
}
