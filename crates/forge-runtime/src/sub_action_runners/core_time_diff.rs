use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
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
        "Time — Time Difference"
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
            FormField::Text {
                key: "from",
                label: "From (datetime or %var%)",
                placeholder: "%start_time%",
            },
            FormField::Text {
                key: "to",
                label: "To (datetime or %var%)",
                placeholder: "%now%",
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

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let from_v = config
            .get("from")
            .cloned()
            .unwrap_or_else(|| Variant::String(String::new()));
        let from_str = ctx.arg_stack.interpolate(from_v.as_str().unwrap_or(""));
        let from_dt = match resolve_datetime(&from_v, &from_str) {
            Ok(dt) => dt,
            Err(e) => return fail(started_at, ctx.index, format!("'from': {e}")),
        };

        let to_v = config
            .get("to")
            .cloned()
            .unwrap_or_else(|| Variant::String(String::new()));
        let to_str = ctx.arg_stack.interpolate(to_v.as_str().unwrap_or(""));
        let to_dt = match resolve_datetime(&to_v, &to_str) {
            Ok(dt) => dt,
            Err(e) => return fail(started_at, ctx.index, format!("'to': {e}")),
        };

        let unit = config
            .get("unit")
            .and_then(|v| v.as_str())
            .unwrap_or("seconds");
        let into_var = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("time.diff_value")
            .to_owned();

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

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.time.diff".to_owned(),
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
            kind: "core.time.diff".to_owned(),
            started_at,
            duration_ms,
            outcome: SubActionOutcome::Failed(msg),
        },
        None,
    )
}
