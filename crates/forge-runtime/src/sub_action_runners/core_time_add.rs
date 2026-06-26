use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::{Date, Duration, Month, OffsetDateTime, format_description::well_known::Rfc3339};

pub struct CoreTimeAddRunner;

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

/// Add calendar months to a datetime, clamping to the last valid day when the
/// original day exceeds the target month's length (e.g. Jan 31 + 1 month → Feb 28/29).
fn add_calendar_months(dt: OffsetDateTime, months: i64) -> OffsetDateTime {
    let date = dt.date();
    let total = date.year() as i64 * 12 + (date.month() as i64 - 1) + months;
    let new_year = total.div_euclid(12) as i32;
    let new_month_idx = (total.rem_euclid(12) + 1) as u8;
    let new_month = Month::try_from(new_month_idx).unwrap_or(Month::December);
    let day = date.day();
    let new_date = (1..=day)
        .rev()
        .find_map(|d| Date::from_calendar_date(new_year, new_month, d).ok())
        .unwrap_or(date);
    dt.replace_date(new_date)
}

#[async_trait]
impl SubActionRunner for CoreTimeAddRunner {
    fn id(&self) -> &str {
        "core.time.add"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "Time — Date/Time Arithmetic"
    }

    fn summary(&self) -> &str {
        "Add or subtract an amount from a UTC datetime; month/year addition clamps to the last valid day on overflow"
    }

    fn search_text(&self) -> &str {
        "time add subtract arithmetic shift date days months years seconds minutes hours"
    }

    fn icon_name(&self) -> &str {
        "clock-plus"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("base".to_owned(), Variant::String(String::new()));
        cfg.insert("add_amount".to_owned(), Variant::Int(0));
        cfg.insert("unit".to_owned(), Variant::String("seconds".to_owned()));
        cfg.insert(
            "into_var".to_owned(),
            Variant::String("time.result".to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "base",
                label: "Base Datetime (or %var%)",
                placeholder: "%now%",
            },
            FormField::Integer {
                key: "add_amount",
                label: "Amount (negative to subtract)",
                min: i64::MIN,
                max: i64::MAX,
            },
            FormField::Select {
                key: "unit",
                label: "Unit",
                options: &["seconds", "minutes", "hours", "days", "months", "years"],
            },
            FormField::Text {
                key: "into_var",
                label: "Output Variable",
                placeholder: "time.result",
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

        let base_v = config
            .get("base")
            .cloned()
            .unwrap_or_else(|| Variant::String(String::new()));
        let base_str = ctx.arg_stack.interpolate(base_v.as_str().unwrap_or(""));
        let base_dt = match resolve_datetime(&base_v, &base_str) {
            Ok(dt) => dt,
            Err(e) => return fail(started_at, ctx.index, format!("'base': {e}")),
        };

        let add_amount = config
            .get("add_amount")
            .and_then(|v| v.as_int())
            .unwrap_or(0);
        let unit = config
            .get("unit")
            .and_then(|v| v.as_str())
            .unwrap_or("seconds");
        let into_var = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("time.result")
            .to_owned();

        let result = match unit {
            "months" => add_calendar_months(base_dt, add_amount),
            "years" => add_calendar_months(base_dt, add_amount * 12),
            other => {
                let dur = match other {
                    "minutes" => Duration::minutes(add_amount),
                    "hours" => Duration::hours(add_amount),
                    "days" => Duration::days(add_amount),
                    _ => Duration::seconds(add_amount),
                };
                base_dt + dur
            }
        };

        let new_stack = ctx
            .arg_stack
            .clone()
            .set(into_var, Variant::Datetime(result));

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.time.add".to_owned(),
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
            kind: "core.time.add".to_owned(),
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
    use time::Time;

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
        let (t, out) = CoreTimeAddRunner.execute(cfg, &ctx).await;
        (t.outcome, out)
    }

    fn cfg(base: Variant, amount: i64, unit: &str) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("base".to_owned(), base);
        c.insert("add_amount".to_owned(), Variant::Int(amount));
        c.insert("unit".to_owned(), Variant::String(unit.to_owned()));
        c
    }

    #[tokio::test]
    async fn add_calendar_month_and_year_arithmetic_clamps_to_valid_day() {
        // time-of-day must be preserved across calendar shifts; day clamps to the
        // target month's length when the source day would be invalid.
        let cases = [
            (
                (2024, Month::January, 31),
                1,
                "months",
                (2024, Month::February, 29),
            ), // leap clamp
            (
                (2023, Month::January, 31),
                1,
                "months",
                (2023, Month::February, 28),
            ), // non-leap clamp
            (
                (2023, Month::December, 15),
                1,
                "months",
                (2024, Month::January, 15),
            ), // year rollover fwd
            (
                (2024, Month::March, 31),
                -1,
                "months",
                (2024, Month::February, 29),
            ), // negative + leap clamp
            (
                (2024, Month::January, 15),
                -1,
                "months",
                (2023, Month::December, 15),
            ), // negative year rollback
            (
                (2024, Month::January, 1),
                12,
                "months",
                (2025, Month::January, 1),
            ), // rem_euclid wrap
            (
                (2024, Month::January, 31),
                1,
                "years",
                (2025, Month::January, 31),
            ), // years = months * 12
            (
                (2024, Month::February, 29),
                1,
                "years",
                (2025, Month::February, 28),
            ), // leap-day anniversary clamp
        ];
        for ((by, bm, bd), amount, unit, (ey, em, ed)) in cases {
            let base = utc(by, bm, bd, 8, 30, 15);
            let expected = utc(ey, em, ed, 8, 30, 15);
            let (outcome, out) = run(&cfg(Variant::Datetime(base), amount, unit)).await;
            assert!(
                matches!(outcome, SubActionOutcome::Success),
                "{by}-{bm:?}-{bd} {amount} {unit}"
            );
            let got = *out
                .unwrap()
                .get("time.result")
                .and_then(|v| v.as_datetime())
                .unwrap();
            assert_eq!(got, expected, "{by}-{bm:?}-{bd} {amount} {unit}");
        }
    }

    #[tokio::test]
    async fn add_duration_units_shift_by_exact_amount() {
        let base = utc(2024, Month::January, 1, 0, 0, 0);
        let cases = [
            (90, "seconds", utc(2024, Month::January, 1, 0, 1, 30)),
            (90, "minutes", utc(2024, Month::January, 1, 1, 30, 0)),
            (25, "hours", utc(2024, Month::January, 2, 1, 0, 0)),
            (1, "days", utc(2024, Month::January, 2, 0, 0, 0)),
            (-1, "days", utc(2023, Month::December, 31, 0, 0, 0)),
            // Unknown unit falls back to seconds.
            (5, "weeks", utc(2024, Month::January, 1, 0, 0, 5)),
        ];
        for (amount, unit, expected) in cases {
            let (outcome, out) = run(&cfg(Variant::Datetime(base), amount, unit)).await;
            assert!(
                matches!(outcome, SubActionOutcome::Success),
                "{amount} {unit}"
            );
            let got = *out
                .unwrap()
                .get("time.result")
                .and_then(|v| v.as_datetime())
                .unwrap();
            assert_eq!(got, expected, "{amount} {unit}");
        }
    }

    #[tokio::test]
    async fn add_writes_result_under_custom_into_var() {
        let base = utc(2024, Month::January, 1, 0, 0, 0);
        let mut c = cfg(Variant::Datetime(base), 1, "days");
        c.insert(
            "into_var".to_owned(),
            Variant::String("deadline".to_owned()),
        );
        let out = run(&c).await.1.unwrap();
        assert!(out.get("time.result").is_none());
        let got = *out.get("deadline").and_then(|v| v.as_datetime()).unwrap();
        assert_eq!(got, utc(2024, Month::January, 2, 0, 0, 0));
    }

    #[tokio::test]
    async fn add_unparseable_base_yields_failed() {
        let (outcome, out) = run(&cfg(
            Variant::String("not a datetime".to_owned()),
            1,
            "days",
        ))
        .await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert!(out.is_none());
    }
}
