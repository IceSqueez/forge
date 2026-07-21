use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub struct CoreTimeDiffRunner;

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
impl SubActionRunner for CoreTimeDiffRunner {
    fn id(&self) -> &str {
        "core.time.diff"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "Time - Time Difference"
    }

    fn summary(&self) -> &str {
        "Compute `to − from` as a signed float in the given unit (seconds, minutes, hours, or days)"
    }

    fn search_text(&self) -> &str {
        "time diff difference between subtract duration elapsed seconds minutes hours days"
    }

    fn icon_name(&self) -> &str {
        "clock-minus"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("from".to_owned(), Variant::String(String::new()));
        cfg.insert("to".to_owned(), Variant::String(String::new()));
        cfg.insert("unit".to_owned(), Variant::String("seconds".to_owned()));
        cfg.insert(
            "into_var".to_owned(),
            Variant::String("time.diff_value".to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DateTime {
                key: "from",
                label: "From (datetime or %var%)",
            },
            FormField::DateTime {
                key: "to",
                label: "To (datetime or %var%)",
            },
            FormField::Select {
                key: "unit",
                label: "Unit",
                options: &["seconds", "minutes", "hours", "days"],
            },
            FormField::Text {
                key: "into_var",
                label: "Output Variable",
                placeholder: "time.diff_value",
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
                kind: VariantKind::Float,
                label: "Time difference".to_owned(),
            }],
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.time.diff");

        let from_v = config
            .get("from")
            .cloned()
            .unwrap_or_else(|| Variant::String(String::new()));
        let from_str = ctx.arg_stack.interpolate(from_v.as_str().unwrap_or(""));
        let from_dt = match resolve_datetime(&from_v, &from_str) {
            Ok(dt) => dt,
            Err(e) => return (timer.failed(format!("'from': {e}")), None),
        };

        let to_v = config
            .get("to")
            .cloned()
            .unwrap_or_else(|| Variant::String(String::new()));
        let to_str = ctx.arg_stack.interpolate(to_v.as_str().unwrap_or(""));
        let to_dt = match resolve_datetime(&to_v, &to_str) {
            Ok(dt) => dt,
            Err(e) => return (timer.failed(format!("'to': {e}")), None),
        };

        let unit = config.str("unit").unwrap_or("seconds");
        let into_var = forge_types::strip_var_decoration(
            config.str_nonempty("into_var").unwrap_or("time.diff_value"),
        );

        // Nanosecond precision avoids float rounding from whole_seconds() when the
        // caller asks for fractional minutes/hours/days.
        let diff_ns = (to_dt - from_dt).whole_nanoseconds() as f64;
        let diff_value = match unit {
            "minutes" => diff_ns / 60_000_000_000.0,
            "hours" => diff_ns / 3_600_000_000_000.0,
            "days" => diff_ns / 86_400_000_000_000.0,
            _ => diff_ns / 1_000_000_000.0,
        };

        let new_stack = ctx
            .arg_stack
            .clone()
            .set(into_var, Variant::Float(diff_value));

        (timer.success(), Some(new_stack))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::{EventId, SubActionOutcome};
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
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let (t, out) = CoreTimeDiffRunner.execute(cfg, &ctx).await;
        (t.outcome, out)
    }

    fn cfg(from: Variant, to: Variant, unit: &str) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("from".to_owned(), from);
        c.insert("to".to_owned(), to);
        c.insert("unit".to_owned(), Variant::String(unit.to_owned()));
        c
    }

    #[tokio::test]
    async fn diff_computes_signed_fractional_value_per_unit() {
        // 90 seconds apart → 1.5 minutes: the fraction must survive, not truncate to 1.
        let from = utc(2024, Month::January, 1, 0, 0, 0);
        let to = utc(2024, Month::January, 1, 0, 1, 30);
        for (unit, expected) in [
            ("seconds", 90.0_f64),
            ("minutes", 1.5),
            ("hours", 90.0 / 3600.0),
            ("days", 90.0 / 86_400.0),
        ] {
            let (outcome, out) =
                run(&cfg(Variant::Datetime(from), Variant::Datetime(to), unit)).await;
            assert!(matches!(outcome, SubActionOutcome::Success), "unit {unit}");
            let v = out
                .unwrap()
                .get("time.diff_value")
                .and_then(|v| v.as_float())
                .unwrap();
            assert!(
                (v - expected).abs() < 1e-9,
                "unit {unit}: got {v}, want {expected}"
            );
        }
    }

    #[tokio::test]
    async fn diff_is_negative_when_to_precedes_from() {
        let from = utc(2024, Month::January, 1, 0, 1, 30);
        let to = utc(2024, Month::January, 1, 0, 0, 0);
        let (_, out) = run(&cfg(
            Variant::Datetime(from),
            Variant::Datetime(to),
            "seconds",
        ))
        .await;
        let v = out
            .unwrap()
            .get("time.diff_value")
            .and_then(|v| v.as_float())
            .unwrap();
        assert!((v - (-90.0)).abs() < 1e-9, "got {v}");
    }

    #[tokio::test]
    async fn diff_same_instant_is_zero() {
        let dt = utc(2024, Month::January, 1, 12, 0, 0);
        let (_, out) = run(&cfg(
            Variant::Datetime(dt),
            Variant::Datetime(dt),
            "seconds",
        ))
        .await;
        let v = out
            .unwrap()
            .get("time.diff_value")
            .and_then(|v| v.as_float())
            .unwrap();
        assert_eq!(v, 0.0);
    }

    #[tokio::test]
    async fn diff_unparseable_from_yields_failed() {
        let to = utc(2024, Month::January, 1, 0, 0, 0);
        let (outcome, out) = run(&cfg(
            Variant::String("garbage".to_owned()),
            Variant::Datetime(to),
            "seconds",
        ))
        .await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert!(out.is_none());
    }
}
