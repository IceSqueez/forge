use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{
    ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind, strip_var_decoration,
};

use crate::scheduled_runs::{ScheduleError, ScheduledRunsCell};

pub const CANCEL_SCHEDULED_KIND_ID: &str = "core.action.cancel_scheduled";

pub const DEFAULT_CANCELLED_VARIABLE: &str = "schedule.cancelled";

const SCHEDULE_KEY_KEY: &str = "key";
const CANCELLED_INTO_KEY: &str = "into_var";

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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert(SCHEDULE_KEY_KEY.to_owned(), Variant::String(String::new()));
        cfg.insert(
            CANCELLED_INTO_KEY.to_owned(),
            Variant::String(DEFAULT_CANCELLED_VARIABLE.to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: SCHEDULE_KEY_KEY,
                label: "Key (exact match)",
                placeholder: "vip-removal:%user_id%",
            },
            FormField::Text {
                key: CANCELLED_INTO_KEY,
                label: "Output Variable (bool)",
                placeholder: DEFAULT_CANCELLED_VARIABLE,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str(SCHEDULE_KEY_KEY).map(|_| ())
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: CANCELLED_INTO_KEY.to_owned(),
                kind: VariantKind::Bool,
                label: "Scheduled run cancelled".to_owned(),
            }],
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, CANCEL_SCHEDULED_KIND_ID);
        let Some(scheduled_runs) = self.scheduled_runs.get() else {
            return (
                timer.failed(ScheduleError::SchedulerStopped.to_string()),
                None,
            );
        };
        let key = ctx
            .arg_stack
            .interpolate(config.str(SCHEDULE_KEY_KEY).unwrap_or(""));
        let cancelled = match scheduled_runs.cancel_by_key(&key).await {
            Ok(cancelled) => cancelled,
            Err(e) => return (timer.failed(e.to_string()), None),
        };
        let into_var = strip_var_decoration(
            config
                .str_nonempty(CANCELLED_INTO_KEY)
                .unwrap_or(DEFAULT_CANCELLED_VARIABLE),
        );
        let stack = ctx
            .arg_stack
            .clone()
            .set(into_var, Variant::Bool(cancelled));
        (timer.success(), Some(stack))
    }
}
