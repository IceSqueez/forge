use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, ProducedVariable, RunContext, StepTimer, SubActionCategory,
    SubActionIo, SubActionRunner, TextEditor, TextField, TypedField, VariableField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, UserText, Variant, VariantKind};

use crate::scheduled_runs::{ScheduleError, ScheduledRunsCell};

pub const CANCEL_SCHEDULED_KIND_ID: &str = "core.action.cancel_scheduled";

pub const DEFAULT_CANCELLED_VARIABLE: &str = "schedule.cancelled";

const CANCELLED_INTO_KEY: &str = "into_var";

const SCHEDULE_KEY: TextField = TextField {
    key: "key",
    label: "Key (exact match)",
    editor: TextEditor::Line {
        placeholder: "vip-removal:%user_id%",
    },
    default: "",
    required: true,
};

const CANCELLED_INTO: VariableField = VariableField {
    key: CANCELLED_INTO_KEY,
    label: "Output Variable (bool)",
    placeholder: DEFAULT_CANCELLED_VARIABLE,
    default: DEFAULT_CANCELLED_VARIABLE,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SCHEDULE_KEY, &CANCELLED_INTO]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, String), ConfigProblem> {
    Ok((
        SCHEDULE_KEY.read(config, args)?,
        CANCELLED_INTO.read(config, args)?,
    ))
}

pub struct CoreActionCancelScheduledRunner {
    scheduled_runs: ScheduledRunsCell,
}

impl CoreActionCancelScheduledRunner {
    pub fn new(scheduled_runs: ScheduledRunsCell) -> Self {
        Self { scheduled_runs }
    }
}

#[async_trait]
impl SubActionRunner for CoreActionCancelScheduledRunner {
    fn id(&self) -> &str {
        CANCEL_SCHEDULED_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Delay
    }

    fn label(&self) -> &str {
        "Cancel Scheduled Action"
    }

    fn summary(&self) -> &str {
        "Cancel the pending scheduled run with this exact key"
    }

    fn search_text(&self) -> &str {
        "cancel scheduled action unschedule remove pending key delay timer"
    }

    fn icon_name(&self) -> &str {
        "circle-x"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: CANCELLED_INTO_KEY.to_owned(),
                kind: VariantKind::Bool,
                label: "Scheduled run cancelled".to_owned(),
            }],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, CANCEL_SCHEDULED_KIND_ID);
        let (key, into_var) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let Some(scheduled_runs) = self.scheduled_runs.get() else {
            return (
                timer.failed(UserText::unlocalized(
                    ScheduleError::SchedulerStopped.to_string(),
                )),
                None,
            );
        };
        let cancelled = match scheduled_runs.cancel_by_key(&key).await {
            Ok(cancelled) => cancelled,
            Err(e) => return (timer.failed(UserText::unlocalized(e.to_string())), None),
        };
        let stack = ctx
            .arg_stack
            .clone()
            .set(into_var, Variant::Bool(cancelled));
        (timer.success(), Some(stack))
    }
}
