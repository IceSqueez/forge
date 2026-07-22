use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub struct CoreTimeFormatRunner;

/// `stack_interp` is only consulted when `v` is a string variant.
fn resolve_datetime(v: &Variant, stack_interp: &str) -> Result<OffsetDateTime, String> {
    if let Some(dt) = v.as_datetime() {
        return Ok(*dt);
    }
    if let Some(secs) = v.as_int() {
        return OffsetDateTime::from_unix_timestamp(secs)
            .map_err(|e| format!("unix timestamp out of range: {e}"));
    }
    let s = stack_interp.trim();
    OffsetDateTime::parse(s, &Rfc3339).or_else(|_| {
        s.parse::<i64>()
            .ok()
            .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok())
            .ok_or_else(|| format!("cannot parse '{s}' as a datetime or unix timestamp"))
    })
}

#[async_trait]
impl SubActionRunner for CoreTimeFormatRunner {
    fn id(&self) -> &str {
        "core.time.format"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "Time - Format Datetime"
    }

    fn summary(&self) -> &str {
        "Format a datetime as a string using a time-crate format description (e.g. [year]-[month]-[day]); always UTC"
    }

    fn search_text(&self) -> &str {
        "time format datetime string timestamp display"
    }

    fn icon_name(&self) -> &str {
        "clock-check"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String(String::new()));
        cfg.insert(
            "format_string".to_owned(),
            Variant::String("[year]-[month]-[day] [hour]:[minute]:[second]".to_owned()),
        );
        cfg.insert(
            "into_var".to_owned(),
            Variant::String("time.formatted".to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DateTime {
                key: "source",
                label: "Source (datetime or %var%)",
            },
            FormField::Text {
                key: "format_string",
                label: "Format Description",
                placeholder: "[year]-[month]-[day] [hour]:[minute]:[second]",
            },
            FormField::Text {
                key: "into_var",
                label: "Output Variable",
                placeholder: "time.formatted",
            },
        ]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "into_var".to_owned(),
                kind: VariantKind::String,
                label: "Formatted datetime".to_owned(),
            }],
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.time.format");

        let source_v = config
            .get("source")
            .cloned()
            .unwrap_or_else(|| Variant::String(String::new()));
        let source_str = ctx.arg_stack.interpolate(source_v.as_str().unwrap_or(""));

        let dt = match resolve_datetime(&source_v, &source_str) {
            Ok(dt) => dt,
            Err(e) => return (timer.failed(e), None),
        };

        let fmt_str = config
            .str("format_string")
            .unwrap_or("[year]-[month]-[day] [hour]:[minute]:[second]")
            .to_owned();
        let into_var = forge_types::strip_var_decoration(
            config.str_nonempty("into_var").unwrap_or("time.formatted"),
        );

        let formatted = match time::format_description::parse_borrowed::<2>(&fmt_str) {
            Ok(desc) => match dt.format(&desc) {
                Ok(s) => s,
                Err(e) => return (timer.failed(format!("format error: {e}")), None),
            },
            Err(e) => return (timer.failed(format!("invalid format_string: {e}")), None),
        };

        let new_stack = ctx
            .arg_stack
            .clone()
            .set(into_var, Variant::String(formatted));

        (timer.success(), Some(new_stack))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::{EventId, SubActionOutcome};
    use time::{Date, Month, Time};

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    fn utc(y: i32, m: Month, d: u8, h: u8, min: u8, s: u8) -> OffsetDateTime {
        OffsetDateTime::new_utc(
            Date::from_calendar_date(y, m, d).unwrap(),
            Time::from_hms(h, min, s).unwrap(),
        )
    }

    async fn run(cfg: &SubActionConfig) -> (SubActionOutcome, Option<ArgStack>) {
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let (t, out) = CoreTimeFormatRunner.execute(cfg, &ctx).await;
        (t.outcome, out)
    }

    fn cfg(source: Variant, fmt: &str) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("source".to_owned(), source);
        c.insert("format_string".to_owned(), Variant::String(fmt.to_owned()));
        c
    }

    #[tokio::test]
    async fn format_renders_fixed_datetime_for_each_description() {
        let dt = utc(2024, Month::January, 15, 12, 34, 56);
        for (fmt, expected) in [
            ("[year]", "2024"),
            ("[year]-[month]-[day]", "2024-01-15"),
            ("[hour]:[minute]:[second]", "12:34:56"),
            (
                "[year]-[month]-[day] [hour]:[minute]:[second]",
                "2024-01-15 12:34:56",
            ),
        ] {
            let (outcome, out) = run(&cfg(Variant::Datetime(dt), fmt)).await;
            assert!(matches!(outcome, SubActionOutcome::Success), "fmt {fmt}");
            assert_eq!(
                out.unwrap().get("time.formatted").and_then(|v| v.as_str()),
                Some(expected),
                "fmt {fmt}"
            );
        }
    }

    #[tokio::test]
    async fn format_resolves_unix_timestamp_source() {
        let (outcome, out) = run(&cfg(Variant::Int(0), "[year]-[month]-[day]")).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(
            out.unwrap().get("time.formatted").and_then(|v| v.as_str()),
            Some("1970-01-01")
        );
    }

    #[tokio::test]
    async fn format_with_invalid_format_string_yields_failed() {
        let dt = utc(2024, Month::January, 15, 0, 0, 0);
        let (outcome, out) = run(&cfg(Variant::Datetime(dt), "[bogus]")).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn format_with_unparseable_source_string_yields_failed() {
        let (outcome, out) =
            run(&cfg(Variant::String("not a datetime".to_owned()), "[year]")).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert!(out.is_none());
    }
}
