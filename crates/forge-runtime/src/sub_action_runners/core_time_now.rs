use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{
    ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind, datetime_to_text,
};
use time::OffsetDateTime;

use super::time_pattern::{
    CUSTOM_NOW_PATTERN_KEY, TIME_NOW_KIND_ID, UTC_KEY, resolve_time_pattern,
};

const TIME_FORMATTED_VAR: &str = "time.formatted";
const TIME_UNIX_SECONDS_VAR: &str = "time.unix_seconds";
const FORMAT_KEY: &str = "format";
const CUSTOM_FORMAT: &str = "custom";

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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("into_var".to_owned(), Variant::String("now".to_owned()));
        cfg.insert(FORMAT_KEY.to_owned(), Variant::String("iso8601".to_owned()));
        cfg.insert(
            CUSTOM_NOW_PATTERN_KEY.to_owned(),
            Variant::String(String::new()),
        );
        cfg.insert(UTC_KEY.to_owned(), Variant::Bool(false));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Select {
                key: FORMAT_KEY,
                label: "Format",
                options: &["iso8601", "unix_seconds", "unix_millis", CUSTOM_FORMAT],
            },
            FormField::Text {
                key: CUSTOM_NOW_PATTERN_KEY,
                label: "Custom date pattern (e.g. HH:mm dd.MM.yyyy, d MMMM yyyy, EEEE)",
                placeholder: "yyyy-MM-dd HH:mm:ss",
            },
            FormField::Toggle {
                key: UTC_KEY,
                label: "Use UTC instead of local time",
            },
            FormField::Text {
                key: "into_var",
                label: "Output Variable",
                placeholder: "now",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        if config.str(FORMAT_KEY) != Some(CUSTOM_FORMAT) {
            return Ok(());
        }
        resolve_time_pattern(config, CUSTOM_NOW_PATTERN_KEY)
            .and_then(|resolved| resolved.pattern().map(|_| ()))
            .map_err(RegistryError::InvalidConfig)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "into_var".to_owned(),
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
        let timer = StepTimer::start(ctx, TIME_NOW_KIND_ID);
        let now: OffsetDateTime = timer.started_at();

        let format = config.str(FORMAT_KEY).unwrap_or("iso8601");
        let into_var =
            forge_types::strip_var_decoration(config.str_nonempty("into_var").unwrap_or("now"));

        let formatted = match format {
            "unix_seconds" => now.unix_timestamp().to_string(),
            "unix_millis" => {
                let ms = now.unix_timestamp() * 1000 + i64::from(now.millisecond());
                ms.to_string()
            }
            CUSTOM_FORMAT => {
                let resolved = match resolve_time_pattern(config, CUSTOM_NOW_PATTERN_KEY) {
                    Ok(resolved) => resolved,
                    Err(e) => return (timer.failed(e), None),
                };
                match resolved.pattern() {
                    Ok(pattern) => pattern.format(now, resolved.zone, resolved.language),
                    Err(e) => return (timer.failed(e), None),
                }
            }
            _ => datetime_to_text(&now),
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
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

    #[tokio::test]
    async fn now_custom_format_with_invalid_description_yields_failed() {
        let (outcome, out) = run(&cfg("custom", "[nope]", "now")).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert!(out.is_none());
    }
}
