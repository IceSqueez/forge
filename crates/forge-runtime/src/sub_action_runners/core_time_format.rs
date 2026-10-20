use async_trait::async_trait;
use forge_registry::{
    ConfigReason, FormField, ProducedVariable, RegistryError, RunContext, StepTimer,
    SubActionCategory, SubActionConfigExt, SubActionIo, SubActionRunner,
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

    fn checks_config_while_editing(&self) -> bool {
        true
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        resolve_time_pattern(config, FORMAT_PATTERN_KEY)
            .and_then(|resolved| resolved.pattern().map(|_| ()))
            .map_err(|detail| {
                RegistryError::invalid_field(
                    FORMAT_PATTERN_KEY,
                    ConfigReason::InvalidFormat(detail),
                )
            })
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
#[allow(clippy::unwrap_used, clippy::panic)]
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

    fn utc_cfg(source: Variant, fmt: &str) -> SubActionConfig {
        let mut c = cfg(source, fmt);
        c.insert("utc".to_owned(), Variant::Bool(true));
        c
    }

    async fn formatted(c: &SubActionConfig) -> String {
        let (outcome, out) = run(c).await;
        assert!(matches!(outcome, SubActionOutcome::Success), "{outcome:?}");
        out.unwrap()
            .get("time.formatted")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_owned()
    }

    fn failure(outcome: SubActionOutcome) -> String {
        match outcome {
            SubActionOutcome::Failed(reason) => reason,
            other => panic!("expected a failed outcome, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn format_renders_each_numeric_pattern_in_utc_when_utc_is_on() {
        let dt = utc(2024, Month::January, 15, 12, 34, 56)
            .to_offset(time::UtcOffset::from_hms(2, 0, 0).unwrap());
        for (fmt, expected) in [
            ("yyyy", "2024"),
            ("dd.MM.yyyy", "15.01.2024"),
            ("HH:mm:ss", "12:34:56"),
            ("h:mm", "12:34"),
            ("yyyy-MM-dd'T'HH:mmxxx", "2024-01-15T12:34+00:00"),
        ] {
            assert_eq!(
                formatted(&utc_cfg(Variant::Datetime(dt), fmt)).await,
                expected,
                "fmt {fmt}"
            );
        }
    }

    #[tokio::test]
    async fn format_with_legacy_description_and_no_utc_key_matches_the_old_utc_english_output() {
        let dt = utc(2024, Month::January, 15, 21, 4, 5)
            .to_offset(time::UtcOffset::from_hms(5, 30, 0).unwrap());
        for legacy in [
            "[year]-[month]-[day] [hour]:[minute]:[second]",
            "[day padding:none] [month repr:long] [year]",
            "[weekday], [hour repr:12]:[minute] [period]",
            "[offset_hour sign:mandatory]:[offset_minute]",
        ] {
            let old = dt
                .to_offset(time::UtcOffset::UTC)
                .format(&time::format_description::parse_borrowed::<2>(legacy).unwrap())
                .unwrap();
            assert_eq!(
                formatted(&cfg(Variant::Datetime(dt), legacy)).await,
                old,
                "legacy {legacy}"
            );
        }
    }

    #[tokio::test]
    async fn format_with_untranslatable_legacy_component_fails_naming_it() {
        let dt = utc(2024, Month::January, 15, 0, 0, 0);
        let (outcome, out) = run(&cfg(Variant::Datetime(dt), "[unix_timestamp]")).await;
        assert!(failure(outcome).contains("[unix_timestamp]"));
        assert!(out.is_none());
    }

    #[test]
    fn validate_rejects_invalid_and_untranslatable_patterns() {
        for (fmt, named) in [
            ("yyyy qqq", "qqq"),
            ("[unix_timestamp]", "[unix_timestamp]"),
        ] {
            let result = CoreTimeFormatRunner.validate_config(&cfg(Variant::Bool(false), fmt));
            assert!(
                matches!(&result, Err(RegistryError::InvalidConfig(reason)) if reason.contains(named)),
                "{fmt}: {result:?}"
            );
        }
    }

    #[tokio::test]
    async fn format_with_invalid_pattern_yields_failed() {
        let dt = utc(2024, Month::January, 15, 0, 0, 0);
        let (outcome, out) = run(&cfg(Variant::Datetime(dt), "yyyy qqq")).await;
        assert!(failure(outcome).contains("yyyy qqq"));
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn format_with_unparseable_source_string_yields_failed() {
        let (outcome, out) = run(&cfg(Variant::String("not a datetime".to_owned()), "yyyy")).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn format_reads_a_datetime_variable_named_in_the_source() {
        let dt = utc(2026, Month::October, 4, 12, 0, 0);
        let stack = ArgStack::new().set("when".to_owned(), Variant::Datetime(dt));
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let config = utc_cfg(Variant::String("%when%".to_owned()), "dd.MM.yyyy HH:mm");

        let (telemetry, out) = CoreTimeFormatRunner.execute(&config, &ctx).await;

        let formatted = out
            .as_ref()
            .and_then(|stack| stack.get("time.formatted"))
            .and_then(Variant::as_str)
            .map(str::to_owned);
        assert_eq!(
            (telemetry.outcome, formatted),
            (
                SubActionOutcome::Success,
                Some("04.10.2026 12:00".to_owned())
            )
        );
    }
}
