use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, DateTimeField, Interpolation, ProducedVariable, RunContext,
    StepTimer, SubActionCategory, SubActionIo, SubActionRunner, TemplateField, ToggleField,
    TypedField, VariableField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, UserText, Variant, VariantKind};
use time::OffsetDateTime;

use super::time_pattern::{
    DEFAULT_TIME_PATTERN, FORMAT_PATTERN_KEY, ResolvedTimePattern, TIME_FORMAT_KIND_ID, UTC_KEY,
    checked_time_pattern, stored_utc,
};

const INTO_VAR_KEY: &str = "into_var";
const FORMATTED_VARIABLE: &str = "time.formatted";

const SOURCE: DateTimeField = DateTimeField {
    key: "source",
    label: "Source (datetime or %var%)",
    default: "",
};

const PATTERN: TemplateField = TemplateField {
    key: FORMAT_PATTERN_KEY,
    label: "Date pattern (e.g. HH:mm dd.MM.yyyy, d MMMM yyyy, EEEE)",
    placeholder: DEFAULT_TIME_PATTERN,
    default: DEFAULT_TIME_PATTERN,
    required: false,
};

const UTC: ToggleField = ToggleField {
    key: UTC_KEY,
    label: "Use UTC instead of local time",
    default: false,
};

const INTO_VAR: VariableField = VariableField {
    key: INTO_VAR_KEY,
    label: "Output Variable",
    placeholder: FORMATTED_VARIABLE,
    default: FORMATTED_VARIABLE,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SOURCE, &PATTERN, &UTC, &INTO_VAR]);

struct FormatConfig {
    source: OffsetDateTime,
    pattern: ResolvedTimePattern,
    into_var: String,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<FormatConfig, ConfigProblem> {
    Ok(FormatConfig {
        source: SOURCE.read(config, args)?,
        pattern: checked_time_pattern(
            PATTERN.key,
            &PATTERN.read(config, args)?,
            stored_utc(&UTC, config, args)?,
        )?,
        into_var: INTO_VAR.read(config, args)?,
    })
}

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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn check_config(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        match (
            PATTERN.peek(config, interpolation)?,
            UTC.peek(config, interpolation)?.map(Some),
        ) {
            (Some(pattern), Some(utc)) => {
                checked_time_pattern(PATTERN.key, &pattern, utc).map(|_| ())
            }
            _ => Ok(()),
        }
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
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
        let timer = StepTimer::start(ctx, self.id());
        let FormatConfig {
            source,
            pattern: resolved,
            into_var,
        } = match read_config(config, ctx.arg_stack) {
            Ok(read) => read,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let formatted = match resolved.pattern() {
            Ok(pattern) => pattern.format(source, resolved.zone, resolved.language),
            Err(e) => return (timer.failed(UserText::unlocalized(e)), None),
        };

        let new_stack = ctx
            .arg_stack
            .clone()
            .set(into_var, Variant::String(formatted));

        (timer.success(), Some(new_stack))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_registry::ConfigReason;
    use forge_registry::RegistryError;
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
    fn validate_reports_invalid_and_untranslatable_patterns_as_format_problems_on_the_pattern_key()
    {
        for (fmt, named) in [
            ("yyyy qqq", "qqq"),
            ("[unix_timestamp]", "[unix_timestamp]"),
        ] {
            let source = Variant::Datetime(utc(2024, Month::January, 15, 0, 0, 0));
            let result = CoreTimeFormatRunner.validate_config(&cfg(source, fmt));
            assert!(
                matches!(
                    &result,
                    Err(RegistryError::InvalidConfig(forge_registry::ConfigProblem {
                        field: Some(field),
                        reason: ConfigReason::InvalidFormat(detail),
                    })) if field == FORMAT_PATTERN_KEY && detail.contains(named)
                ),
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
