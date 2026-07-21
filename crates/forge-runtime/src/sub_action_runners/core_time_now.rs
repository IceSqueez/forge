use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub struct CoreTimeNowRunner;

#[async_trait]
impl SubActionRunner for CoreTimeNowRunner {
    fn id(&self) -> &str {
        "core.time.now"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "Time - Get Current Time"
    }

    fn summary(&self) -> &str {
        "Capture the current UTC time; writes the datetime to `into_var`, a formatted string to `time.formatted`, and unix seconds to `time.unix_seconds`"
    }

    fn search_text(&self) -> &str {
        "time now current datetime timestamp unix clock"
    }

    fn icon_name(&self) -> &str {
        "clock"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("into_var".to_owned(), Variant::String("now".to_owned()));
        cfg.insert("format".to_owned(), Variant::String("iso8601".to_owned()));
        cfg.insert(
            "custom_format_string".to_owned(),
            Variant::String(String::new()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Select {
                key: "format",
                label: "Format",
                options: &["iso8601", "unix_seconds", "unix_millis", "custom"],
            },
            FormField::Text {
                key: "custom_format_string",
                label: "Custom Format",
                placeholder: "[year]-[month]-[day] [hour]:[minute]:[second]",
            },
            FormField::Text {
                key: "into_var",
                label: "Output Variable",
                placeholder: "now",
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
                label: "Current time".to_owned(),
            }],
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.time.now");
        let now: OffsetDateTime = timer.started_at();

        let format = config.str("format").unwrap_or("iso8601");
        let custom_fmt_string = config.str("custom_format_string").unwrap_or("").to_owned();
        let into_var =
            forge_types::strip_var_decoration(config.str_nonempty("into_var").unwrap_or("now"));

        let formatted = match format {
            "unix_seconds" => now.unix_timestamp().to_string(),
            "unix_millis" => {
                let ms = now.unix_timestamp() * 1000 + i64::from(now.millisecond());
                ms.to_string()
            }
            "custom" => match time::format_description::parse_borrowed::<2>(&custom_fmt_string) {
                Ok(desc) => match now.format(&desc) {
                    Ok(s) => s,
                    Err(e) => {
                        return (timer.failed(format!("time format error: {e}")), None);
                    }
                },
                Err(e) => {
                    return (
                        timer.failed(format!("invalid custom_format_string: {e}")),
                        None,
                    );
                }
            },
            _ => match now.format(&Rfc3339) {
                Ok(s) => s,
                Err(e) => return (timer.failed(format!("time format error: {e}")), None),
            },
        };

        let new_stack = ctx
            .arg_stack
            .clone()
            .set(into_var, Variant::Datetime(now))
            .set("time.formatted".to_owned(), Variant::String(formatted))
            .set(
                "time.unix_seconds".to_owned(),
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
    async fn now_iso8601_outputs_datetime_formatted_and_unix_are_mutually_consistent() {
        let (outcome, out) = run(&cfg("iso8601", "", "captured")).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        let out = out.unwrap();
        // into_var routes the datetime (not the hardcoded default key).
        let dt = *out.get("captured").and_then(|v| v.as_datetime()).unwrap();
        let formatted = out.get("time.formatted").and_then(|v| v.as_str()).unwrap();
        let unix = out
            .get("time.unix_seconds")
            .and_then(|v| v.as_int())
            .unwrap();
        // All three documented outputs describe the same instant.
        assert_eq!(OffsetDateTime::parse(formatted, &Rfc3339).unwrap(), dt);
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
