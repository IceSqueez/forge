use async_trait::async_trait;
use forge_registry::{
    ChoiceField, ConfigChoice, ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory,
    SubActionRunner, TextEditor, TextField, TypedField,
};
use forge_types::{ArgStack, LogLevel, SCRIPT_LOG_TARGET, SubActionConfig, SubActionTelemetry};
use tracing::{debug, error, info, trace, warn};

#[derive(Clone, Copy)]
enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    fn log_level(self) -> LogLevel {
        match self {
            Self::Trace => LogLevel::Trace,
            Self::Debug => LogLevel::Debug,
            Self::Info => LogLevel::Info,
            Self::Warn => LogLevel::Warn,
            Self::Error => LogLevel::Error,
        }
    }
}

impl ConfigChoice for Level {
    const ALL: &'static [Self] = &[
        Self::Trace,
        Self::Debug,
        Self::Info,
        Self::Warn,
        Self::Error,
    ];
    const WIRE_NAMES: &'static [&'static str] = &["trace", "debug", "info", "warn", "error"];

    fn wire_name(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

const LEVEL: ChoiceField<Level> = ChoiceField {
    key: "level",
    label: "Level",
    default: Level::Info,
};

const MESSAGE: TextField = TextField {
    key: "message",
    label: "Message",
    editor: TextEditor::Area,
    default: "",
    required: false,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&LEVEL, &MESSAGE]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(LogLevel, String), ConfigProblem> {
    Ok((
        LEVEL.read(config, args)?.log_level(),
        MESSAGE.read(config, args)?,
    ))
}

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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let (level, message) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        match level {
            LogLevel::Trace => trace!(target: SCRIPT_LOG_TARGET, message = message.as_str()),
            LogLevel::Debug => debug!(target: SCRIPT_LOG_TARGET, message = message.as_str()),
            LogLevel::Info => info!(target: SCRIPT_LOG_TARGET, message = message.as_str()),
            LogLevel::Warn => warn!(target: SCRIPT_LOG_TARGET, message = message.as_str()),
            LogLevel::Error => error!(target: SCRIPT_LOG_TARGET, message = message.as_str()),
        }

        (timer.success(), None)
    }
}

#[cfg(test)]
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
