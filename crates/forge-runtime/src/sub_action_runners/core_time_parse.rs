use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};

use super::time_pattern::{
    DEFAULT_TIME_PATTERN, PARSE_PATTERN_KEY, TIME_PARSE_KIND_ID, UTC_KEY, resolve_time_pattern,
};

pub struct CoreTimeParseRunner;

#[async_trait]
impl SubActionRunner for CoreTimeParseRunner {
    fn id(&self) -> &str {
        TIME_PARSE_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "Time - Parse Time String"
    }

    fn summary(&self) -> &str {
        "Read text into a datetime with a date pattern (e.g. dd.MM.yyyy HH:mm); text without an offset is read as local time, or as UTC when the toggle is on"
    }

    fn search_text(&self) -> &str {
        "time parse string datetime format convert pattern date"
    }

    fn icon_name(&self) -> &str {
        "clock-edit"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String(String::new()));
        cfg.insert(
            PARSE_PATTERN_KEY.to_owned(),
            Variant::String(DEFAULT_TIME_PATTERN.to_owned()),
        );
        cfg.insert(UTC_KEY.to_owned(), Variant::Bool(false));
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
                key: PARSE_PATTERN_KEY,
                label: "Date pattern (e.g. dd.MM.yyyy HH:mm, yyyy-MM-dd'T'HH:mm:ssxxx)",
                placeholder: DEFAULT_TIME_PATTERN,
            },
            FormField::Toggle {
                key: UTC_KEY,
                label: "Read text without an offset as UTC instead of local time",
            },
            FormField::Text {
                key: "into_var",
                label: "Output Variable",
                placeholder: "time.parsed",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        resolve_time_pattern(config, PARSE_PATTERN_KEY)
            .and_then(|resolved| resolved.pattern().map(|_| ()))
            .map_err(RegistryError::InvalidConfig)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "into_var".to_owned(),
                kind: VariantKind::Datetime,
                label: "Parsed datetime".to_owned(),
            }],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, TIME_PARSE_KIND_ID);

        let source_template = config.str("source").unwrap_or("").to_owned();
        let source = ctx.arg_stack.interpolate(&source_template);

        let into_var = forge_types::strip_var_decoration(
            config.str_nonempty("into_var").unwrap_or("time.parsed"),
        );

        let resolved = match resolve_time_pattern(config, PARSE_PATTERN_KEY) {
            Ok(resolved) => resolved,
            Err(e) => return (timer.failed(e), None),
        };
        let pattern = match resolved.pattern() {
            Ok(pattern) => pattern,
            Err(e) => return (timer.failed(e), None),
        };

        let dt = match pattern.parse_text(&source, resolved.zone, resolved.language) {
            Ok(dt) => dt,
            Err(e) => {
                return (
                    timer.failed(format!(
                        "cannot read '{source}' with pattern '{}': {e}",
                        resolved.text()
                    )),
                    None,
                );
            }
        };

        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::Datetime(dt));

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

    fn utc_cfg(source: &str, fmt: &str) -> SubActionConfig {
        let mut c = cfg(source, fmt);
        c.insert("utc".to_owned(), Variant::Bool(true));
        c
    }

    fn parsed(out: Option<ArgStack>) -> OffsetDateTime {
        *out.unwrap()
            .get("time.parsed")
            .and_then(|v| v.as_datetime())
            .unwrap()
    }

    fn failure(outcome: SubActionOutcome) -> String {
        match outcome {
            SubActionOutcome::Failed(reason) => reason,
            other => panic!("expected a failed outcome, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn parse_with_offset_in_text_yields_that_instant() {
        let (outcome, out) = run(&cfg(
            "2024-01-15T12:00:00+02:00",
            "yyyy-MM-dd'T'HH:mm:ssxxx",
        ))
        .await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(parsed(out), utc(2024, Month::January, 15, 10, 0, 0));
    }

    #[tokio::test]
    async fn parse_without_offset_reads_utc_when_utc_is_on() {
        let (outcome, out) = run(&utc_cfg("15.01.2024 12:00", "dd.MM.yyyy HH:mm")).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        let dt = parsed(out);
        assert_eq!(dt, utc(2024, Month::January, 15, 12, 0, 0));
        assert_eq!(dt.offset(), time::UtcOffset::UTC);
    }

    #[tokio::test]
    async fn parse_with_legacy_description_and_no_utc_key_reads_utc() {
        let (outcome, out) = run(&cfg(
            "2024-01-15 12:00:00",
            "[year]-[month]-[day] [hour]:[minute]:[second]",
        ))
        .await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        let dt = parsed(out);
        assert_eq!(dt, utc(2024, Month::January, 15, 12, 0, 0));
        assert_eq!(dt.offset(), time::UtcOffset::UTC);
    }

    #[tokio::test]
    async fn parse_interpolates_source_var_before_parsing() {
        let stack = ArgStack::new().set(
            "when".to_owned(),
            Variant::String("2024-01-15 12:00:00".to_owned()),
        );
        let c = utc_cfg("%when%", "yyyy-MM-dd HH:mm:ss");
        let (outcome, out) = run_with(&c, stack).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(parsed(out), utc(2024, Month::January, 15, 12, 0, 0));
    }

    #[tokio::test]
    async fn parse_unreadable_source_fails_naming_the_source_and_pattern() {
        let (outcome, out) = run(&utc_cfg("not-a-date", "yyyy-MM-dd")).await;
        let reason = failure(outcome);
        assert!(
            reason.contains("'not-a-date'") && reason.contains("'yyyy-MM-dd'"),
            "{reason}"
        );
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn parse_with_invalid_pattern_yields_failed() {
        let (outcome, out) = run(&cfg("2024-01-15", "yyyy qqq")).await;
        assert!(failure(outcome).contains("yyyy qqq"));
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn parse_with_untranslatable_legacy_component_fails_naming_it() {
        let (outcome, out) = run(&cfg("1700000000", "[unix_timestamp]")).await;
        assert!(failure(outcome).contains("[unix_timestamp]"));
        assert!(out.is_none());
    }

    #[test]
    fn validate_rejects_invalid_and_untranslatable_patterns() {
        for (fmt, named) in [
            ("yyyy qqq", "qqq"),
            ("[unix_timestamp]", "[unix_timestamp]"),
        ] {
            let result = CoreTimeParseRunner.validate_config(&cfg("", fmt));
            assert!(
                matches!(&result, Err(RegistryError::InvalidConfig(reason)) if reason.contains(named)),
                "{fmt}: {result:?}"
            );
        }
    }
}
