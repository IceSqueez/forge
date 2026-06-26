use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub struct CoreTimeFormatRunner;

/// Resolve a config variant to an `OffsetDateTime`.
///
/// `stack_interp` is the ArgStack-interpolated form of the string value; only
/// consulted when `v` is a string variant.
fn resolve_datetime(v: &Variant, stack_interp: &str) -> Result<OffsetDateTime, String> {
    if let Some(dt) = v.as_datetime() {
        return Ok(*dt);
    }
    if let Some(secs) = v.as_int() {
        return OffsetDateTime::from_unix_timestamp(secs)
            .map_err(|e| format!("unix timestamp out of range: {e}"));
    }
    let s = stack_interp.trim();
    OffsetDateTime::parse(s, &Rfc3339).or_else(|_| {
        s.parse::<i64>()
            .ok()
            .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok())
            .ok_or_else(|| format!("cannot parse '{s}' as a datetime or unix timestamp"))
    })
}

#[async_trait]
impl SubActionRunner for CoreTimeFormatRunner {
    fn id(&self) -> &str {
        "core.time.format"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "Time — Format Datetime"
    }

    fn summary(&self) -> &str {
        "Format a datetime as a string using a time-crate format description (e.g. [year]-[month]-[day]); always UTC"
    }

    fn search_text(&self) -> &str {
        "time format datetime string timestamp display"
    }

    fn icon_name(&self) -> &str {
        "clock-check"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String(String::new()));
        cfg.insert(
            "format_string".to_owned(),
            Variant::String("[year]-[month]-[day] [hour]:[minute]:[second]".to_owned()),
        );
        cfg.insert(
            "into_var".to_owned(),
            Variant::String("time.formatted".to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "source",
                label: "Source (datetime or %var%)",
                placeholder: "%now%",
            },
            FormField::Text {
                key: "format_string",
                label: "Format Description",
                placeholder: "[year]-[month]-[day] [hour]:[minute]:[second]",
            },
            FormField::Text {
                key: "into_var",
                label: "Output Variable",
                placeholder: "time.formatted",
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

        let source_v = config
            .get("source")
            .cloned()
            .unwrap_or_else(|| Variant::String(String::new()));
        let source_str = ctx.arg_stack.interpolate(source_v.as_str().unwrap_or(""));

        let dt = match resolve_datetime(&source_v, &source_str) {
            Ok(dt) => dt,
            Err(e) => return fail(started_at, ctx.index, e),
        };

        let fmt_str = config
            .get("format_string")
            .and_then(|v| v.as_str())
            .unwrap_or("[year]-[month]-[day] [hour]:[minute]:[second]")
            .to_owned();
        let into_var = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("time.formatted")
            .to_owned();

        let formatted = match time::format_description::parse_borrowed::<2>(&fmt_str) {
            Ok(desc) => match dt.format(&desc) {
                Ok(s) => s,
                Err(e) => return fail(started_at, ctx.index, format!("format error: {e}")),
            },
            Err(e) => return fail(started_at, ctx.index, format!("invalid format_string: {e}")),
        };

        let new_stack = ctx
            .arg_stack
            .clone()
            .set(into_var, Variant::String(formatted));

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.time.format".to_owned(),
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
            kind: "core.time.format".to_owned(),
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

    async fn run(cfg: &SubActionConfig) -> (SubActionOutcome, Option<ArgStack>) {
        let stack = ArgStack::new();
        let ctx = RunContext {
            arg_stack: &stack,
            index: 0,
            parent_event_id: EventId::new(),
            publisher: &NullPublisher,
        };
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
    async fn format_resolves_unix_timestamp_source() {
        let (outcome, out) = run(&cfg(Variant::Int(0), "[year]-[month]-[day]")).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(
            out.unwrap().get("time.formatted").and_then(|v| v.as_str()),
            Some("1970-01-01")
        );
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
