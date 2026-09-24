use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};
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

const MONTHS_PER_YEAR: i64 = 12;

/// Clamps to the target month's last valid day when the original day exceeds it (Jan 31 + 1mo -> Feb 28/29); `None` when the result leaves the representable range.
fn add_calendar_months(dt: OffsetDateTime, months: i64) -> Option<OffsetDateTime> {
    let date = dt.date();
    let total = (i64::from(date.year()) * MONTHS_PER_YEAR + (date.month() as i64 - 1))
        .checked_add(months)?;
    let new_year = i32::try_from(total.div_euclid(MONTHS_PER_YEAR)).ok()?;
    let new_month = Month::try_from((total.rem_euclid(MONTHS_PER_YEAR) + 1) as u8).ok()?;
    let new_date = (1..=date.day())
        .rev()
        .find_map(|d| Date::from_calendar_date(new_year, new_month, d).ok())?;
    Some(dt.replace_date(new_date))
}

fn add_fixed(dt: OffsetDateTime, amount: i64, unit: Duration) -> Option<OffsetDateTime> {
    let seconds = amount.checked_mul(unit.whole_seconds())?;
    dt.checked_add(Duration::seconds(seconds))
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
        "Time - Date/Time Arithmetic"
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
            FormField::DateTime {
                key: "base",
                label: "Base Datetime (or %var%)",
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

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "into_var".to_owned(),
                kind: VariantKind::Datetime,
                label: "Shifted datetime".to_owned(),
            }],
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.time.add");

        let base_v = config
            .get("base")
            .cloned()
            .unwrap_or_else(|| Variant::String(String::new()));
        let base_str = ctx.arg_stack.interpolate(base_v.as_str().unwrap_or(""));
        let base_dt = match resolve_datetime(&base_v, &base_str) {
            Ok(dt) => dt,
            Err(e) => return (timer.failed(format!("'base': {e}")), None),
        };

        let add_amount = config.int("add_amount").unwrap_or(0);
        let unit = config.str("unit").unwrap_or("seconds");
        let into_var = forge_types::strip_var_decoration(
            config.str_nonempty("into_var").unwrap_or("time.result"),
        );

        let result = match unit {
            "months" => add_calendar_months(base_dt, add_amount),
            "years" => add_amount
                .checked_mul(MONTHS_PER_YEAR)
                .and_then(|months| add_calendar_months(base_dt, months)),
            "minutes" => add_fixed(base_dt, add_amount, Duration::MINUTE),
            "hours" => add_fixed(base_dt, add_amount, Duration::HOUR),
            "days" => add_fixed(base_dt, add_amount, Duration::DAY),
            _ => add_fixed(base_dt, add_amount, Duration::SECOND),
        };
        let Some(result) = result else {
            return (
                timer.failed(format!(
                    "adding {add_amount} {unit} to {base_dt} leaves the supported date range"
                )),
                None,
            );
        };

        let new_stack = ctx
            .arg_stack
            .clone()
            .set(into_var, Variant::Datetime(result));

        (timer.success(), Some(new_stack))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::{EventId, SubActionOutcome};
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
    async fn add_out_of_range_amount_fails_without_panic() {
        let base = Variant::Datetime(utc(2024, Month::January, 1, 0, 0, 0));
        for (amount, unit) in [
            (i64::MAX, "seconds"),
            (i64::MIN, "seconds"),
            (i64::MAX / 60 + 1, "minutes"),
            (i64::MAX, "hours"),
            (5_000_000, "days"),
            (-5_000_000, "days"),
            (i64::MAX, "months"),
            (200_000, "months"),
            (i64::MAX / 12 + 1, "years"),
            // Why: times 12 this wraps to exactly +8 months, so only a checked multiply rejects it.
            (1_537_228_672_809_129_302, "years"),
            (10_000, "years"),
        ] {
            let (outcome, out) = run(&cfg(base.clone(), amount, unit)).await;
            assert!(
                matches!(&outcome, SubActionOutcome::Failed(reason) if reason.contains("range")),
                "{amount} {unit} must fail with a range reason, got {outcome:?}"
            );
            assert!(
                out.is_none(),
                "{amount} {unit} must not produce a scope stack"
            );
        }
    }

    #[tokio::test]
    async fn add_reaching_the_last_representable_year_still_succeeds() {
        let base = Variant::Datetime(utc(2024, Month::January, 1, 0, 0, 0));
        let (outcome, out) = run(&cfg(base, 9999 - 2024, "years")).await;
        assert!(matches!(outcome, SubActionOutcome::Success), "{outcome:?}");
        assert_eq!(
            out.unwrap().get("time.result"),
            Some(&Variant::Datetime(utc(9999, Month::January, 1, 0, 0, 0)))
        );
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
