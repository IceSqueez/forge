use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
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
        "Time — Get Current Time"
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

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let format = config
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("iso8601");
        let custom_fmt_string = config
            .get("custom_format_string")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let into_var = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("now")
            .to_owned();

        let formatted = match format {
            "unix_seconds" => started_at.unix_timestamp().to_string(),
            "unix_millis" => {
                let ms = started_at.unix_timestamp() * 1000 + i64::from(started_at.millisecond());
                ms.to_string()
            }
            "custom" => match time::format_description::parse_borrowed::<2>(&custom_fmt_string) {
                Ok(desc) => match started_at.format(&desc) {
                    Ok(s) => s,
                    Err(e) => {
                        return fail(started_at, ctx.index, format!("time format error: {e}"));
                    }
                },
                Err(e) => {
                    return fail(
                        started_at,
                        ctx.index,
                        format!("invalid custom_format_string: {e}"),
                    );
                }
            },
            _ => match started_at.format(&Rfc3339) {
                Ok(s) => s,
                Err(e) => return fail(started_at, ctx.index, format!("time format error: {e}")),
            },
        };

        let new_stack = ctx
            .arg_stack
            .clone()
            .set(into_var, Variant::Datetime(started_at))
            .set("time.formatted".to_owned(), Variant::String(formatted))
            .set(
                "time.unix_seconds".to_owned(),
                Variant::Int(started_at.unix_timestamp()),
            );

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.time.now".to_owned(),
                started_at,
                duration_ms,
                outcome: SubActionOutcome::Success,
            },
            Some(new_stack),
        )
    }
}

fn fail(
    started_at: OffsetDateTime,
    index: usize,
    msg: String,
) -> (SubActionTelemetry, Option<ArgStack>) {
    let duration_ms = (OffsetDateTime::now_utc() - started_at)
        .whole_milliseconds()
        .max(0) as u64;
    (
        SubActionTelemetry {
            index,
            kind: "core.time.now".to_owned(),
            started_at,
            duration_ms,
            outcome: SubActionOutcome::Failed(msg),
        },
        None,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    async fn run(cfg: &SubActionConfig) -> (SubActionOutcome, Option<ArgStack>) {
        let stack = ArgStack::new();
        let ctx = RunContext {
            arg_stack: &stack,
            index: 0,
            parent_event_id: EventId::new(),
            publisher: &NullPublisher,
        };
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
