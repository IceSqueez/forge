use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, Interpolation, ProducedVariable, RunContext, StepTimer,
    SubActionCategory, SubActionIo, SubActionRunner, TemplateField, TextEditor, TextField,
    ToggleField, TypedField, VariableField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};

use super::time_pattern::{
    DEFAULT_TIME_PATTERN, PARSE_PATTERN_KEY, ResolvedTimePattern, TIME_PARSE_KIND_ID, UTC_KEY,
    checked_time_pattern, stored_utc,
};
use crate::core_messages;

const INTO_VAR_KEY: &str = "into_var";
const PARSED_VARIABLE: &str = "time.parsed";

const SOURCE: TextField = TextField {
    key: "source",
    label: "Source String",
    editor: TextEditor::Line {
        placeholder: "2024-01-15 12:00:00",
    },
    default: "",
    required: false,
};

const PATTERN: TemplateField = TemplateField {
    key: PARSE_PATTERN_KEY,
    label: "Date pattern (e.g. dd.MM.yyyy HH:mm, yyyy-MM-dd'T'HH:mm:ssxxx)",
    placeholder: DEFAULT_TIME_PATTERN,
    default: DEFAULT_TIME_PATTERN,
    required: false,
};

const UTC: ToggleField = ToggleField {
    key: UTC_KEY,
    label: "Read text without an offset as UTC instead of local time",
    default: false,
};

const INTO_VAR: VariableField = VariableField {
    key: INTO_VAR_KEY,
    label: "Output Variable",
    placeholder: PARSED_VARIABLE,
    default: PARSED_VARIABLE,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SOURCE, &PATTERN, &UTC, &INTO_VAR]);

struct ParseConfig {
    source: String,
    pattern: ResolvedTimePattern,
    into_var: String,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<ParseConfig, ConfigProblem> {
    Ok(ParseConfig {
        source: SOURCE.read(config, args)?,
        pattern: checked_time_pattern(
            PATTERN.key,
            &PATTERN.read(config, args)?,
            stored_utc(&UTC, config, args)?,
        )?,
        into_var: INTO_VAR.read(config, args)?,
    })
}

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
        let timer = StepTimer::start(ctx, self.id());
        let ParseConfig {
            source,
            pattern: resolved,
            into_var,
        } = match read_config(config, ctx.arg_stack) {
            Ok(read) => read,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let pattern = match resolved.runnable_pattern() {
            Ok(pattern) => pattern,
            Err(e) => return (timer.failed(e.into()), None),
        };

        let dt = match pattern.parse_text(&source, resolved.zone, resolved.language) {
            Ok(dt) => dt,
            Err(e) => {
                return (
                    timer.failed(
                        core_messages::time_text_unreadable(&source, resolved.text(), e).into(),
                    ),
                    None,
                );
            }
        };

        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::Datetime(dt));
        (timer.success(), Some(new_stack))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::failure::{failure_message, verbatim_arg};
    use forge_events::test_support::NullPublisher;

    use forge_registry::ConfigReason;
    use forge_registry::RegistryError;
    use forge_types::{EventId, SubActionOutcome};
    use time::{Date, Month, OffsetDateTime, Time};

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

    fn pattern_problem(outcome: &SubActionOutcome) -> forge_types::MessageRef {
        match outcome {
            SubActionOutcome::Failed(reason) => {
                crate::test_support::failure::nested_problem(reason)
                    .cloned()
                    .expect("a failure on the pattern field")
            }
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
        let reason = failure_message(&outcome);
        assert_eq!(reason.code(), &core_messages::TIME_TEXT_UNREADABLE);
        assert_eq!(verbatim_arg(reason, "text"), Some("not-a-date"));
        assert_eq!(verbatim_arg(reason, "pattern"), Some("yyyy-MM-dd"));
        assert!(reason.detail().is_some());
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn parse_with_invalid_pattern_yields_failed() {
        let (outcome, out) = run(&cfg("2024-01-15", "yyyy qqq")).await;
        let problem = pattern_problem(&outcome);
        assert_eq!(problem.code(), &crate::core_messages::TIME_PATTERN_INVALID);
        assert_eq!(
            crate::test_support::failure::verbatim_arg(&problem, "pattern"),
            Some("yyyy qqq")
        );
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn parse_with_untranslatable_legacy_component_fails_naming_it() {
        let (outcome, out) = run(&cfg("1700000000", "[unix_timestamp]")).await;
        let problem = pattern_problem(&outcome);
        assert_eq!(problem.code(), &crate::core_messages::TIME_PATTERN_LEGACY);
        assert_eq!(
            crate::test_support::failure::verbatim_arg(&problem, "component"),
            Some("[unix_timestamp]")
        );
        assert!(out.is_none());
    }

    #[test]
    fn validate_reports_invalid_and_untranslatable_patterns_as_format_problems_on_the_pattern_key()
    {
        for (fmt, code, arg, named) in [
            (
                "yyyy qqq",
                core_messages::TIME_PATTERN_INVALID,
                "pattern",
                "yyyy qqq",
            ),
            (
                "[unix_timestamp]",
                core_messages::TIME_PATTERN_LEGACY,
                "component",
                "[unix_timestamp]",
            ),
        ] {
            let result = CoreTimeParseRunner.validate_config(&cfg("", fmt));
            assert!(
                matches!(
                    &result,
                    Err(RegistryError::InvalidConfig(forge_registry::ConfigProblem {
                        field: Some(field),
                        reason: ConfigReason::Coded(message),
                    })) if field == PARSE_PATTERN_KEY
                        && message.code() == &code
                        && message.arg(arg)
                            == Some(&forge_types::MessageArg::Verbatim(
                                forge_types::VerbatimText::new(named)
                            ))
                ),
                "{fmt}: {result:?}"
            );
        }
    }
}
