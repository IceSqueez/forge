use async_trait::async_trait;
use forge_registry::{
    ChoiceField, ConfigChoice, ConfigProblem, ConfigSpec, DateTimeField, IntegerEditor,
    IntegerField, ProducedVariable, RunContext, StepTimer, SubActionCategory, SubActionIo,
    SubActionRunner, TypedField, ValueBounds, VariableField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, UserText, Variant, VariantKind};
use time::{Date, Duration, Month, OffsetDateTime};

const INTO_VAR_KEY: &str = "into_var";
const RESULT_VARIABLE: &str = "time.result";

const MONTHS_PER_YEAR: i64 = 12;

#[derive(Clone, Copy)]
enum TimeUnit {
    Seconds,
    Minutes,
    Hours,
    Days,
    Months,
    Years,
}

impl TimeUnit {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Seconds => "seconds",
            Self::Minutes => "minutes",
            Self::Hours => "hours",
            Self::Days => "days",
            Self::Months => "months",
            Self::Years => "years",
        }
    }

    fn shift(self, base: OffsetDateTime, amount: i64) -> Option<OffsetDateTime> {
        match self {
            Self::Seconds => add_fixed(base, amount, Duration::SECOND),
            Self::Minutes => add_fixed(base, amount, Duration::MINUTE),
            Self::Hours => add_fixed(base, amount, Duration::HOUR),
            Self::Days => add_fixed(base, amount, Duration::DAY),
            Self::Months => add_calendar_months(base, amount),
            Self::Years => amount
                .checked_mul(MONTHS_PER_YEAR)
                .and_then(|months| add_calendar_months(base, months)),
        }
    }
}

impl ConfigChoice for TimeUnit {
    const ALL: &'static [Self] = &[
        Self::Seconds,
        Self::Minutes,
        Self::Hours,
        Self::Days,
        Self::Months,
        Self::Years,
    ];
    const WIRE_NAMES: &'static [&'static str] = &[
        Self::Seconds.as_str(),
        Self::Minutes.as_str(),
        Self::Hours.as_str(),
        Self::Days.as_str(),
        Self::Months.as_str(),
        Self::Years.as_str(),
    ];

    fn wire_name(self) -> &'static str {
        self.as_str()
    }
}

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

const BASE: DateTimeField = DateTimeField {
    key: "base",
    label: "Base Datetime (or %var%)",
    default: "",
};

const ADD_AMOUNT: IntegerField = IntegerField {
    key: "add_amount",
    label: "Amount (negative to subtract)",
    editor: IntegerEditor::Number,
    default: 0,
    bounds: ValueBounds::UNBOUNDED,
};

const UNIT: ChoiceField<TimeUnit> = ChoiceField {
    key: "unit",
    label: "Unit",
    default: TimeUnit::Seconds,
};

const INTO_VAR: VariableField = VariableField {
    key: INTO_VAR_KEY,
    label: "Output Variable",
    placeholder: RESULT_VARIABLE,
    default: RESULT_VARIABLE,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&BASE, &ADD_AMOUNT, &UNIT, &INTO_VAR]);

struct AddConfig {
    base: OffsetDateTime,
    add_amount: i64,
    unit: TimeUnit,
    into_var: String,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<AddConfig, ConfigProblem> {
    Ok(AddConfig {
        base: BASE.read(config, args)?,
        add_amount: ADD_AMOUNT.read(config, args)?,
        unit: UNIT.read(config, args)?,
        into_var: INTO_VAR.read(config, args)?,
    })
}

pub struct CoreTimeAddRunner;

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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::Datetime,
                label: "Shifted datetime".to_owned(),
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
        let AddConfig {
            base,
            add_amount,
            unit,
            into_var,
        } = match read_config(config, ctx.arg_stack) {
            Ok(read) => read,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let Some(result) = unit.shift(base, add_amount) else {
            return (
                timer.failed(UserText::unlocalized(format!(
                    "adding {add_amount} {} to {base} leaves the supported date range",
                    unit.as_str()
                ))),
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
