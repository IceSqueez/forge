use async_trait::async_trait;
use forge_registry::{
    ChoiceField, ConfigChoice, ConfigProblem, ConfigSpec, Interpolation, ProducedVariable,
    RunContext, StepTimer, SubActionCategory, SubActionIo, SubActionRunner, TemplateField,
    ToggleField, TypedField, VariableField,
};
use forge_types::{
    ArgStack, SubActionConfig, SubActionTelemetry, UserText, Variant, VariantKind, datetime_to_text,
};
use time::OffsetDateTime;

use super::time_pattern::{
    CUSTOM_NOW_PATTERN_KEY, DEFAULT_TIME_PATTERN, ResolvedTimePattern, TIME_NOW_KIND_ID, UTC_KEY,
    checked_time_pattern, stored_utc,
};

const TIME_FORMATTED_VAR: &str = "time.formatted";
const TIME_UNIX_SECONDS_VAR: &str = "time.unix_seconds";
const INTO_VAR_KEY: &str = "into_var";
const NOW_VARIABLE: &str = "now";
const MILLIS_PER_SECOND: i64 = 1000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum NowFormat {
    Iso8601,
    UnixSeconds,
    UnixMillis,
    Custom,
}

impl NowFormat {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Iso8601 => "iso8601",
            Self::UnixSeconds => "unix_seconds",
            Self::UnixMillis => "unix_millis",
            Self::Custom => "custom",
        }
    }
}

impl ConfigChoice for NowFormat {
    const ALL: &'static [Self] = &[
        Self::Iso8601,
        Self::UnixSeconds,
        Self::UnixMillis,
        Self::Custom,
    ];
    const WIRE_NAMES: &'static [&'static str] = &[
        Self::Iso8601.as_str(),
        Self::UnixSeconds.as_str(),
        Self::UnixMillis.as_str(),
        Self::Custom.as_str(),
    ];

    fn wire_name(self) -> &'static str {
        self.as_str()
    }
}

const FORMAT: ChoiceField<NowFormat> = ChoiceField {
    key: "format",
    label: "Format",
    default: NowFormat::Iso8601,
};

const CUSTOM_PATTERN: TemplateField = TemplateField {
    key: CUSTOM_NOW_PATTERN_KEY,
    label: "Custom date pattern (e.g. HH:mm dd.MM.yyyy, d MMMM yyyy, EEEE)",
    placeholder: DEFAULT_TIME_PATTERN,
    default: "",
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
    placeholder: NOW_VARIABLE,
    default: NOW_VARIABLE,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&FORMAT, &CUSTOM_PATTERN, &UTC, &INTO_VAR]);

enum NowText {
    Iso8601,
    UnixSeconds,
    UnixMillis,
    Custom(ResolvedTimePattern),
}

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(NowText, String), ConfigProblem> {
    let text = match FORMAT.read(config, args)? {
        NowFormat::Iso8601 => NowText::Iso8601,
        NowFormat::UnixSeconds => NowText::UnixSeconds,
        NowFormat::UnixMillis => NowText::UnixMillis,
        NowFormat::Custom => NowText::Custom(checked_time_pattern(
            CUSTOM_PATTERN.key,
            &CUSTOM_PATTERN.read(config, args)?,
            stored_utc(&UTC, config, args)?,
        )?),
    };
    Ok((text, INTO_VAR.read(config, args)?))
}

fn formatted(now: OffsetDateTime, text: &NowText) -> Result<String, String> {
    Ok(match text {
        NowText::UnixSeconds => now.unix_timestamp().to_string(),
        NowText::UnixMillis => {
            let ms = now.unix_timestamp() * MILLIS_PER_SECOND + i64::from(now.millisecond());
            ms.to_string()
        }
        NowText::Custom(resolved) => {
            resolved
                .pattern()?
                .format(now, resolved.zone, resolved.language)
        }
        NowText::Iso8601 => datetime_to_text(&now),
    })
}

pub struct CoreTimeNowRunner;

#[async_trait]
impl SubActionRunner for CoreTimeNowRunner {
    fn id(&self) -> &str {
        TIME_NOW_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "Time - Get Current Time"
    }

    fn summary(&self) -> &str {
        "Capture the current time; writes the datetime to `into_var`, a formatted string to `time.formatted` (custom: a date pattern such as HH:mm dd.MM.yyyy, local time or UTC), and unix seconds to `time.unix_seconds`"
    }

    fn search_text(&self) -> &str {
        "time now current datetime timestamp unix clock pattern date"
    }

    fn icon_name(&self) -> &str {
        "clock"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn check_config(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        if FORMAT.peek(config, interpolation)? != Some(NowFormat::Custom) {
            return Ok(());
        }
        match (
            CUSTOM_PATTERN.peek(config, interpolation)?,
            UTC.peek(config, interpolation)?.map(Some),
        ) {
            (Some(pattern), Some(utc)) => {
                checked_time_pattern(CUSTOM_PATTERN.key, &pattern, utc).map(|_| ())
            }
            _ => Ok(()),
        }
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::Datetime,
                label: "Current time".to_owned(),
            }],
            fixed_outputs: vec![
                TIME_FORMATTED_VAR.to_owned(),
                TIME_UNIX_SECONDS_VAR.to_owned(),
            ],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let now: OffsetDateTime = timer.started_at();
        let (text, into_var) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let formatted = match formatted(now, &text) {
            Ok(formatted) => formatted,
            Err(e) => return (timer.failed(UserText::unlocalized(e)), None),
        };

        let new_stack = ctx
            .arg_stack
            .clone()
            .set(into_var, Variant::Datetime(now))
            .set(TIME_FORMATTED_VAR.to_owned(), Variant::String(formatted))
            .set(
                TIME_UNIX_SECONDS_VAR.to_owned(),
                Variant::Int(now.unix_timestamp()),
            );

        (timer.success(), Some(new_stack))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_registry::RegistryError;
    use forge_types::{EventId, SubActionOutcome};

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    async fn run(cfg: &SubActionConfig) -> (SubActionOutcome, Option<ArgStack>) {
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let (t, out) = CoreTimeNowRunner.execute(cfg, &ctx).await;
        (t.outcome, out)
    }

    fn cfg(format: &str, custom: &str, into_var: &str) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("format".to_owned(), Variant::String(format.to_owned()));
        c.insert(
            "custom_format_string".to_owned(),
            Variant::String(custom.to_owned()),
        );
        c.insert("into_var".to_owned(), Variant::String(into_var.to_owned()));
        c
    }

    #[tokio::test]
    async fn now_default_format_writes_canonical_text_and_unix_seconds_of_the_captured_instant() {
        let (outcome, out) = run(&cfg("iso8601", "", "captured")).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        let out = out.unwrap();
        let dt = *out.get("captured").and_then(|v| v.as_datetime()).unwrap();
        let formatted = out.get("time.formatted").and_then(|v| v.as_str()).unwrap();
        let unix = out
            .get("time.unix_seconds")
            .and_then(|v| v.as_int())
            .unwrap();
        assert_eq!(formatted, datetime_to_text(&dt));
        assert_eq!(unix, dt.unix_timestamp());
    }

    #[tokio::test]
    async fn now_unix_seconds_is_within_wall_clock_window() {
        let before = OffsetDateTime::now_utc().unix_timestamp();
        let (_, out) = run(&cfg("iso8601", "", "now")).await;
        let after = OffsetDateTime::now_utc().unix_timestamp();
        let unix = out
            .unwrap()
            .get("time.unix_seconds")
            .and_then(|v| v.as_int())
            .unwrap();
        assert!(
            unix >= before && unix <= after,
            "unix {unix} not in [{before}, {after}]"
        );
    }

    #[tokio::test]
    async fn now_unix_seconds_format_writes_integer_seconds_string() {
        let (_, out) = run(&cfg("unix_seconds", "", "now")).await;
        let out = out.unwrap();
        let formatted = out.get("time.formatted").and_then(|v| v.as_str()).unwrap();
        let unix = out
            .get("time.unix_seconds")
            .and_then(|v| v.as_int())
            .unwrap();
        assert_eq!(formatted, unix.to_string());
    }

    fn custom_cfg(pattern: &str, utc: Option<bool>) -> SubActionConfig {
        let mut c = cfg("custom", pattern, "now");
        if let Some(utc) = utc {
            c.insert("utc".to_owned(), Variant::Bool(utc));
        }
        c
    }

    fn failure(outcome: SubActionOutcome) -> String {
        match outcome {
            SubActionOutcome::Failed(reason) => reason.code_or_text(),
            other => panic!("expected a failed outcome, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn now_custom_pattern_with_utc_on_renders_the_captured_instant_in_utc() {
        let pattern = "yyyy-MM-dd HH:mm:ss.SSSSSSSSS xxx";
        let (outcome, out) = run(&custom_cfg(pattern, Some(true))).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        let out = out.unwrap();
        let captured = *out.get("now").and_then(|v| v.as_datetime()).unwrap();
        let expected = forge_types::DatePattern::parse(pattern).unwrap().format(
            captured,
            forge_types::PatternZone::Utc,
            forge_types::NamesLanguage::En,
        );
        assert_eq!(
            out.get("time.formatted").and_then(|v| v.as_str()),
            Some(expected.as_str())
        );
    }

    #[tokio::test]
    async fn now_custom_legacy_pattern_without_utc_key_renders_in_utc() {
        let (outcome, out) = run(&custom_cfg(
            "[offset_hour sign:mandatory]:[offset_minute]",
            None,
        ))
        .await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(
            out.unwrap().get("time.formatted").and_then(|v| v.as_str()),
            Some("+00:00")
        );
    }

    #[tokio::test]
    async fn now_custom_rejected_patterns_fail_the_run_naming_the_cause() {
        for (pattern, named) in [
            ("yyyy qqq", "qqq"),
            ("", "empty"),
            ("[unix_timestamp]", "[unix_timestamp]"),
        ] {
            let (outcome, out) = run(&custom_cfg(pattern, None)).await;
            let reason = failure(outcome);
            assert!(reason.contains(named), "{pattern:?}: {reason}");
            assert!(out.is_none(), "{pattern:?}");
        }
    }

    #[test]
    fn validate_rejects_custom_patterns_that_cannot_render() {
        for pattern in ["yyyy qqq", "", "[unix_timestamp]"] {
            assert!(
                matches!(
                    CoreTimeNowRunner.validate_config(&custom_cfg(pattern, None)),
                    Err(RegistryError::InvalidConfig(_))
                ),
                "{pattern:?}"
            );
        }
    }

    #[test]
    fn validate_ignores_the_custom_pattern_when_the_mode_is_not_custom() {
        for format in ["iso8601", "unix_seconds", "unix_millis"] {
            assert!(
                CoreTimeNowRunner
                    .validate_config(&cfg(format, "yyyy qqq", "now"))
                    .is_ok(),
                "{format}"
            );
        }
    }
}
