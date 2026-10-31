use async_trait::async_trait;
use forge_registry::{
    ChoiceField, ConfigChoice, ConfigProblem, ConfigSpec, DateTimeField, ProducedVariable,
    RunContext, StepTimer, SubActionCategory, SubActionIo, SubActionRunner, TypedField,
    VariableField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};
use time::{Duration, OffsetDateTime};

const INTO_VAR_KEY: &str = "into_var";
const DIFF_VARIABLE: &str = "time.diff_value";

#[derive(Clone, Copy)]
enum DiffUnit {
    Seconds,
    Minutes,
    Hours,
    Days,
}

impl DiffUnit {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Seconds => "seconds",
            Self::Minutes => "minutes",
            Self::Hours => "hours",
            Self::Days => "days",
        }
    }

    const fn span(self) -> Duration {
        match self {
            Self::Seconds => Duration::SECOND,
            Self::Minutes => Duration::MINUTE,
            Self::Hours => Duration::HOUR,
            Self::Days => Duration::DAY,
        }
    }
}

impl ConfigChoice for DiffUnit {
    const ALL: &'static [Self] = &[Self::Seconds, Self::Minutes, Self::Hours, Self::Days];
    const WIRE_NAMES: &'static [&'static str] = &[
        Self::Seconds.as_str(),
        Self::Minutes.as_str(),
        Self::Hours.as_str(),
        Self::Days.as_str(),
    ];

    fn wire_name(self) -> &'static str {
        self.as_str()
    }
}

const FROM: DateTimeField = DateTimeField {
    key: "from",
    label: "From (datetime or %var%)",
    default: "",
};

const TO: DateTimeField = DateTimeField {
    key: "to",
    label: "To (datetime or %var%)",
    default: "",
};

const UNIT: ChoiceField<DiffUnit> = ChoiceField {
    key: "unit",
    label: "Unit",
    default: DiffUnit::Seconds,
};

const INTO_VAR: VariableField = VariableField {
    key: INTO_VAR_KEY,
    label: "Output Variable",
    placeholder: DIFF_VARIABLE,
    default: DIFF_VARIABLE,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&FROM, &TO, &UNIT, &INTO_VAR]);

struct DiffConfig {
    from: OffsetDateTime,
    to: OffsetDateTime,
    unit: DiffUnit,
    into_var: String,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<DiffConfig, ConfigProblem> {
    Ok(DiffConfig {
        from: FROM.read(config, args)?,
        to: TO.read(config, args)?,
        unit: UNIT.read(config, args)?,
        into_var: INTO_VAR.read(config, args)?,
    })
}

pub struct CoreTimeDiffRunner;

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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::Float,
                label: "Time difference".to_owned(),
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
        let DiffConfig {
            from,
            to,
            unit,
            into_var,
        } = match read_config(config, ctx.arg_stack) {
            Ok(read) => read,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let diff_ns = (to - from).whole_nanoseconds() as f64;
        let diff_value = diff_ns / unit.span().whole_nanoseconds() as f64;

        let new_stack = ctx
            .arg_stack
            .clone()
            .set(into_var, Variant::Float(diff_value));

        (timer.success(), Some(new_stack))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::test_support::NullPublisher;

    use forge_types::{EventId, SubActionOutcome};
    use time::{Date, Month, OffsetDateTime, Time};

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
