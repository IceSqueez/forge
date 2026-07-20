use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{
    ArgStack, LogLevel, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant,
};
use time::OffsetDateTime;
use tracing::{debug, error, info, trace, warn};

pub struct CoreLogWriteRunner;

#[async_trait]
impl SubActionRunner for CoreLogWriteRunner {
    fn id(&self) -> &str {
        "core.log.write"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Write Log"
    }

    fn summary(&self) -> &str {
        "Emit a log message at a chosen severity level"
    }

    fn search_text(&self) -> &str {
        "log write message debug info warn error trace"
    }

    fn icon_name(&self) -> &str {
        "terminal"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("level".to_owned(), Variant::String("info".to_owned()));
        cfg.insert("message".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Select {
                key: "level",
                label: "Level",
                options: &["trace", "debug", "info", "warn", "error"],
            },
            FormField::TextArea {
                key: "message",
                label: "Message",
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

        let level_str = config
            .get("level")
            .and_then(|v| v.as_str())
            .unwrap_or("info");
        let message_template = config
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        let level = parse_level(level_str);
        let message = ctx.arg_stack.interpolate(message_template);

        match level {
            LogLevel::Trace => trace!(target: "forge::action", message = message.as_str()),
            LogLevel::Debug => debug!(target: "forge::action", message = message.as_str()),
            LogLevel::Info => info!(target: "forge::action", message = message.as_str()),
            LogLevel::Warn => warn!(target: "forge::action", message = message.as_str()),
            LogLevel::Error => error!(target: "forge::action", message = message.as_str()),
        }

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                index: ctx.index,
                kind: "core.log.write".to_owned(),
                started_at,
                duration_ms,
                outcome: SubActionOutcome::Success,
            },
            None,
        )
    }
}

fn parse_level(s: &str) -> LogLevel {
    match s {
        "trace" => LogLevel::Trace,
        "debug" => LogLevel::Debug,
        "warn" => LogLevel::Warn,
        "error" => LogLevel::Error,
        _ => LogLevel::Info,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_level_all_variants() {
        assert!(matches!(parse_level("trace"), LogLevel::Trace));
        assert!(matches!(parse_level("debug"), LogLevel::Debug));
        assert!(matches!(parse_level("info"), LogLevel::Info));
        assert!(matches!(parse_level("warn"), LogLevel::Warn));
        assert!(matches!(parse_level("error"), LogLevel::Error));
        assert!(matches!(parse_level("unknown"), LogLevel::Info));
    }
}
