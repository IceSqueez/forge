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
    const WIRE_NAMES: &'static [&'static str] =
        &["trace", "debug", LOG_DEFAULT_LEVEL, "warn", "error"];

    fn wire_name(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => LOG_DEFAULT_LEVEL,
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

pub const LOG_WRITE_KIND_ID: &str = "core.log.write";
pub const LOG_LEVEL_KEY: &str = "level";
pub const LOG_MESSAGE_KEY: &str = "message";
pub const LOG_DEFAULT_LEVEL: &str = "info";

const LEVEL: ChoiceField<Level> = ChoiceField {
    key: LOG_LEVEL_KEY,
    label: "Level",
    default: Level::Info,
};

const MESSAGE: TextField = TextField {
    key: LOG_MESSAGE_KEY,
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
        LOG_WRITE_KIND_ID
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
    use forge_registry::ConfigReason;
    use forge_types::Variant;

    fn level_config(level: &str) -> SubActionConfig {
        SubActionConfig::from([(LEVEL.key.to_owned(), Variant::String(level.to_owned()))])
    }

    #[test]
    fn each_stored_level_maps_to_its_log_level() {
        for (stored, expected) in [
            ("trace", LogLevel::Trace),
            ("debug", LogLevel::Debug),
            ("info", LogLevel::Info),
            ("warn", LogLevel::Warn),
            ("error", LogLevel::Error),
        ] {
            let (level, _) = read_config(&level_config(stored), &ArgStack::new()).unwrap();
            assert_eq!(level, expected, "{stored}");
        }
    }

    #[test]
    fn unknown_level_fails_instead_of_logging_at_info() {
        let problem = read_config(&level_config("loud"), &ArgStack::new()).unwrap_err();
        assert_eq!(problem.field.as_deref(), Some(LEVEL.key));
        assert!(matches!(problem.reason, ConfigReason::InvalidChoice(_)));
    }
}
