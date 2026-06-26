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
