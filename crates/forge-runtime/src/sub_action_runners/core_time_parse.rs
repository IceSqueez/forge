use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};
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
        "Time - Parse Time String"
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

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "into_var".to_owned(),
                kind: VariantKind::Datetime,
                label: "Parsed datetime".to_owned(),
            }],
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.time.parse");

        let source_template = config.str("source").unwrap_or("").to_owned();
        let source = ctx.arg_stack.interpolate(&source_template);

        let format_str = config
            .str("format")
            .unwrap_or("[year]-[month]-[day] [hour]:[minute]:[second]")
            .to_owned();
        let into_var = forge_types::strip_var_decoration(
            config.str_nonempty("into_var").unwrap_or("time.parsed"),
        );

        let desc = match time::format_description::parse_borrowed::<2>(&format_str) {
            Ok(d) => d,
            Err(e) => return (timer.failed(format!("invalid format: {e}")), None),
        };

        // Try OffsetDateTime first (format includes offset), fall back to
        // PrimitiveDateTime + assume UTC for formats without an offset component.
        let dt = OffsetDateTime::parse(&source, &desc)
            .or_else(|_| PrimitiveDateTime::parse(&source, &desc).map(|pdt| pdt.assume_utc()))
            .map_err(|_| format!("cannot parse '{source}' with format '{format_str}'"));

        let dt = match dt {
            Ok(dt) => dt,
            Err(e) => return (timer.failed(e), None),
        };

        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::Datetime(dt));

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

    async fn run_with(
        cfg: &SubActionConfig,
        stack: ArgStack,
    ) -> (SubActionOutcome, Option<ArgStack>) {
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let (t, out) = CoreTimeParseRunner.execute(cfg, &ctx).await;
        (t.outcome, out)
    }

    async fn run(cfg: &SubActionConfig) -> (SubActionOutcome, Option<ArgStack>) {
        run_with(cfg, ArgStack::new()).await
    }

    fn cfg(source: &str, fmt: &str) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("source".to_owned(), Variant::String(source.to_owned()));
        c.insert("format".to_owned(), Variant::String(fmt.to_owned()));
        c
    }

    #[tokio::test]
    async fn parse_rfc3339_with_offset_yields_correct_instant() {
        let fmt = "[year]-[month]-[day]T[hour]:[minute]:[second][offset_hour \
                   sign:mandatory]:[offset_minute]";
        let (outcome, out) = run(&cfg("2024-01-15T12:00:00+02:00", fmt)).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        let dt = *out
            .unwrap()
            .get("time.parsed")
            .and_then(|v| v.as_datetime())
            .unwrap();
        // 12:00 at +02:00 is 10:00 UTC - offset honoured, not dropped.
        assert_eq!(
            dt.unix_timestamp(),
            utc(2024, Month::January, 15, 10, 0, 0).unix_timestamp()
        );
    }

    #[tokio::test]
    async fn parse_without_offset_assumes_utc() {
        let (outcome, out) = run(&cfg(
            "2024-01-15 12:00:00",
            "[year]-[month]-[day] [hour]:[minute]:[second]",
        ))
        .await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        let dt = *out
            .unwrap()
            .get("time.parsed")
            .and_then(|v| v.as_datetime())
            .unwrap();
        assert_eq!(dt, utc(2024, Month::January, 15, 12, 0, 0));
    }

    #[tokio::test]
    async fn parse_interpolates_source_var_before_parsing() {
        let stack = ArgStack::new().set(
            "when".to_owned(),
            Variant::String("2024-01-15 12:00:00".to_owned()),
        );
        let c = cfg("%when%", "[year]-[month]-[day] [hour]:[minute]:[second]");
        let (outcome, out) = run_with(&c, stack).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        let dt = *out
            .unwrap()
            .get("time.parsed")
            .and_then(|v| v.as_datetime())
            .unwrap();
        assert_eq!(dt, utc(2024, Month::January, 15, 12, 0, 0));
    }

    #[tokio::test]
    async fn parse_invalid_source_yields_failed() {
        let (outcome, out) = run(&cfg(
            "not-a-date",
            "[year]-[month]-[day] [hour]:[minute]:[second]",
        ))
        .await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn parse_invalid_format_string_yields_failed() {
        let (outcome, out) = run(&cfg("2024-01-15", "[nonsense]")).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert!(out.is_none());
    }
}
