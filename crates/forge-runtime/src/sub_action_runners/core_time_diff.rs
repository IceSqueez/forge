use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};
use time::Duration;

use super::config_choice::{ConfigChoice, read_choice};
use super::datetime_input::resolve_datetime;

const UNIT_KEY: &str = "unit";

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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("from".to_owned(), Variant::String(String::new()));
        cfg.insert("to".to_owned(), Variant::String(String::new()));
        cfg.insert(
            UNIT_KEY.to_owned(),
            Variant::String(DiffUnit::Seconds.as_str().to_owned()),
        );
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
                key: UNIT_KEY,
                label: "Unit",
                options: DiffUnit::WIRE_NAMES,
            },
            FormField::Text {
                key: "into_var",
                label: "Output Variable",
                placeholder: "time.diff_value",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        read_choice(config, UNIT_KEY, DiffUnit::Seconds).map(|_| ())
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "into_var".to_owned(),
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

        let unit = match read_choice(config, UNIT_KEY, DiffUnit::Seconds) {
            Ok(unit) => unit,
            Err(e) => return (timer.failed(e.to_string()), None),
        };
        let into_var = forge_types::strip_var_decoration(
            config.str_nonempty("into_var").unwrap_or("time.diff_value"),
        );

        let diff_ns = (to_dt - from_dt).whole_nanoseconds() as f64;
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
    use forge_events::{Event, EventPublisher};
    use forge_types::{EventId, SubActionOutcome};
    use time::{Date, Month, OffsetDateTime, Time};

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
