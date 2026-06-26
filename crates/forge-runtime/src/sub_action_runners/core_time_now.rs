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
