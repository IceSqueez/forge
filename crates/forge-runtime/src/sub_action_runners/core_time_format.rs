use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};

use super::datetime_input::resolve_datetime;
use super::time_pattern::{
    DEFAULT_TIME_PATTERN, FORMAT_PATTERN_KEY, TIME_FORMAT_KIND_ID, UTC_KEY, resolve_time_pattern,
};

pub struct CoreTimeFormatRunner;

#[async_trait]
impl SubActionRunner for CoreTimeFormatRunner {
    fn id(&self) -> &str {
        TIME_FORMAT_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "Time - Format Datetime"
    }

    fn summary(&self) -> &str {
        "Format a datetime as text with a date pattern (e.g. HH:mm dd.MM.yyyy or d MMMM yyyy) in local time, or in UTC when the toggle is on"
    }

    fn search_text(&self) -> &str {
        "time format datetime string timestamp display pattern date"
    }

    fn icon_name(&self) -> &str {
        "clock-check"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String(String::new()));
        cfg.insert(
            FORMAT_PATTERN_KEY.to_owned(),
            Variant::String(DEFAULT_TIME_PATTERN.to_owned()),
        );
        cfg.insert(UTC_KEY.to_owned(), Variant::Bool(false));
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
                key: FORMAT_PATTERN_KEY,
                label: "Date pattern (e.g. HH:mm dd.MM.yyyy, d MMMM yyyy, EEEE)",
                placeholder: DEFAULT_TIME_PATTERN,
            },
            FormField::Toggle {
                key: UTC_KEY,
                label: "Use UTC instead of local time",
            },
            FormField::Text {
                key: "into_var",
                label: "Output Variable",
                placeholder: "time.formatted",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        resolve_time_pattern(config, FORMAT_PATTERN_KEY)
            .and_then(|resolved| resolved.pattern().map(|_| ()))
            .map_err(RegistryError::InvalidConfig)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "into_var".to_owned(),
                kind: VariantKind::String,
                label: "Formatted datetime".to_owned(),
            }],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, TIME_FORMAT_KIND_ID);

        let source_v = config
            .get("source")
            .cloned()
            .unwrap_or_else(|| Variant::String(String::new()));
        let source_str = ctx.arg_stack.interpolate(source_v.as_str().unwrap_or(""));

        let dt = match resolve_datetime(&source_v, &source_str) {
            Ok(dt) => dt,
            Err(e) => return (timer.failed(e), None),
        };

        let into_var = forge_types::strip_var_decoration(
            config.str_nonempty("into_var").unwrap_or("time.formatted"),
        );

        let resolved = match resolve_time_pattern(config, FORMAT_PATTERN_KEY) {
            Ok(resolved) => resolved,
            Err(e) => return (timer.failed(e), None),
        };
        let formatted = match resolved.pattern() {
            Ok(pattern) => pattern.format(dt, resolved.zone, resolved.language),
            Err(e) => return (timer.failed(e), None),
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
    use time::{Date, Month, OffsetDateTime, Time};

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
