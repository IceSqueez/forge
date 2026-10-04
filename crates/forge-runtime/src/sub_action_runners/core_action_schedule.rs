use std::time::Duration;

use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_storage::MissedRunPolicy;
use forge_types::{
    ActionId, ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind,
    strip_var_decoration,
};

use super::datetime_input::resolve_datetime;
use crate::scheduled_runs::{
    MAX_LATE_TOLERANCE, MAX_SCHEDULE_DELAY, MIN_LATE_TOLERANCE, ScheduleDue, ScheduleError,
    ScheduleIntent, ScheduleRequest, ScheduledRunsCell, SchedulingContext, skip_if_late_by_minutes,
};

pub const SCHEDULE_ACTION_KIND_ID: &str = "core.action.schedule";

pub const UNIT_MINUTES: &str = "minutes";
pub const UNIT_HOURS: &str = "hours";
pub const UNIT_DAYS: &str = "days";

pub const POLICY_RUN_LATE_ONCE: &str = "run late once";
pub const POLICY_SKIP_IF_LATE: &str = "skip if late";

pub const DEFAULT_DUE_AT_VARIABLE: &str = "schedule.due_at";
pub const DEFAULT_SCHEDULED_ID_VARIABLE: &str = "schedule.id";

const ACTION_KEY: &str = "action_id";
const DELAY_AMOUNT_KEY: &str = "delay_amount";
const DELAY_UNIT_KEY: &str = "delay_unit";
const USE_DUE_AT_KEY: &str = "use_due_at";
const DUE_AT_KEY: &str = "due_at";
const SCHEDULE_KEY_KEY: &str = "key";
const INHERIT_ARGS_KEY: &str = "inherit_args";
const MISSED_POLICY_KEY: &str = "missed_policy";
const LATE_TOLERANCE_MINUTES_KEY: &str = "late_tolerance_minutes";
const DUE_AT_INTO_KEY: &str = "due_at_into_var";
const ID_INTO_KEY: &str = "id_into_var";

const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = 60 * SECONDS_PER_MINUTE;
const SECONDS_PER_DAY: u64 = 24 * SECONDS_PER_HOUR;
const DEFAULT_DELAY_MINUTES: i64 = 10;
const DEFAULT_LATE_TOLERANCE_MINUTES: i64 = 10;

pub struct CoreActionScheduleRunner {
    scheduled_runs: ScheduledRunsCell,
}

impl CoreActionScheduleRunner {
    pub fn new(scheduled_runs: ScheduledRunsCell) -> Self {
        Self { scheduled_runs }
    }

    fn request(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> Result<ScheduleRequest, String> {
        let raw_action = ctx
            .arg_stack
            .interpolate(config.str(ACTION_KEY).unwrap_or(""));
        let target_action_id = raw_action
            .trim()
            .parse::<ActionId>()
            .map_err(|_| "no valid action is selected".to_owned())?;
        let key = ctx
            .arg_stack
            .interpolate(config.str(SCHEDULE_KEY_KEY).unwrap_or(""));
        Ok(SchedulingContext::of_run(ctx).request(ScheduleIntent {
            target_action_id,
            due: due(config, ctx)?,
            key: Some(key),
            missed_run_policy: missed_run_policy(config),
            inherit_args: config.bool(INHERIT_ARGS_KEY).unwrap_or(true),
        }))
    }
}

fn due(config: &SubActionConfig, ctx: &RunContext<'_>) -> Result<ScheduleDue, String> {
    if config.bool(USE_DUE_AT_KEY).unwrap_or(false) {
        let raw = config
            .get(DUE_AT_KEY)
            .cloned()
            .unwrap_or_else(|| Variant::String(String::new()));
        let interpolated = ctx.arg_stack.interpolate(raw.as_str().unwrap_or(""));
        let instant =
            resolve_datetime(&raw, &interpolated).map_err(|e| format!("due date-time: {e}"))?;
        return Ok(ScheduleDue::At(instant));
    }
    let amount = u64::try_from(config.int(DELAY_AMOUNT_KEY).unwrap_or(0)).unwrap_or(0);
    let unit_seconds = match config.str(DELAY_UNIT_KEY).unwrap_or(UNIT_MINUTES) {
        UNIT_DAYS => SECONDS_PER_DAY,
        UNIT_HOURS => SECONDS_PER_HOUR,
        _ => SECONDS_PER_MINUTE,
    };
    Ok(ScheduleDue::After(Duration::from_secs(
        amount.saturating_mul(unit_seconds),
    )))
}

fn missed_run_policy(config: &SubActionConfig) -> MissedRunPolicy {
    if config.str(MISSED_POLICY_KEY) != Some(POLICY_SKIP_IF_LATE) {
        return MissedRunPolicy::RunLateOnce;
    }
    skip_if_late_by_minutes(config.int(LATE_TOLERANCE_MINUTES_KEY).unwrap_or(0))
}

fn whole_minutes(duration: Duration) -> i64 {
    i64::try_from(duration.as_secs() / SECONDS_PER_MINUTE).unwrap_or(i64::MAX)
}

#[async_trait]
impl SubActionRunner for CoreActionScheduleRunner {
    fn id(&self) -> &str {
        SCHEDULE_ACTION_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Delay
    }

    fn label(&self) -> &str {
        "Schedule Action"
    }

    fn summary(&self) -> &str {
        "Run an action later, even after a restart"
    }

    fn search_text(&self) -> &str {
        "schedule action later delay timer remind expire after days hours minutes date time key"
    }

    fn icon_name(&self) -> &str {
        "calendar"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert(ACTION_KEY.to_owned(), Variant::String(String::new()));
        cfg.insert(
            DELAY_AMOUNT_KEY.to_owned(),
            Variant::Int(DEFAULT_DELAY_MINUTES),
        );
        cfg.insert(
            DELAY_UNIT_KEY.to_owned(),
            Variant::String(UNIT_MINUTES.to_owned()),
        );
        cfg.insert(USE_DUE_AT_KEY.to_owned(), Variant::Bool(false));
        cfg.insert(DUE_AT_KEY.to_owned(), Variant::String(String::new()));
        cfg.insert(SCHEDULE_KEY_KEY.to_owned(), Variant::String(String::new()));
        cfg.insert(INHERIT_ARGS_KEY.to_owned(), Variant::Bool(true));
        cfg.insert(
            MISSED_POLICY_KEY.to_owned(),
            Variant::String(POLICY_RUN_LATE_ONCE.to_owned()),
        );
        cfg.insert(
            LATE_TOLERANCE_MINUTES_KEY.to_owned(),
            Variant::Int(DEFAULT_LATE_TOLERANCE_MINUTES),
        );
        cfg.insert(
            DUE_AT_INTO_KEY.to_owned(),
            Variant::String(DEFAULT_DUE_AT_VARIABLE.to_owned()),
        );
        cfg.insert(
            ID_INTO_KEY.to_owned(),
            Variant::String(DEFAULT_SCHEDULED_ID_VARIABLE.to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: ACTION_KEY,
                label: "Action",
                options_key: "action.ids",
            },
            FormField::Integer {
                key: DELAY_AMOUNT_KEY,
                label: "Run after",
                min: 1,
                max: whole_minutes(MAX_SCHEDULE_DELAY),
            },
            FormField::Select {
                key: DELAY_UNIT_KEY,
                label: "Unit",
                options: &[UNIT_MINUTES, UNIT_HOURS, UNIT_DAYS],
            },
            FormField::Optional {
                key: USE_DUE_AT_KEY,
                label: "Run at a date-time instead",
                inner: Box::new(FormField::DateTime {
                    key: DUE_AT_KEY,
                    label: "Date-time (or %var%)",
                }),
            },
            FormField::Text {
                key: SCHEDULE_KEY_KEY,
                label: "Key (optional, replaces a pending run with the same key)",
                placeholder: "vip-removal:%user_id%",
            },
            FormField::Toggle {
                key: INHERIT_ARGS_KEY,
                label: "Pass current variables",
            },
            FormField::Select {
                key: MISSED_POLICY_KEY,
                label: "If forge was closed at the due time",
                options: &[POLICY_RUN_LATE_ONCE, POLICY_SKIP_IF_LATE],
            },
            FormField::Integer {
                key: LATE_TOLERANCE_MINUTES_KEY,
                label: "Skip when late by more than (minutes)",
                min: whole_minutes(MIN_LATE_TOLERANCE),
                max: whole_minutes(MAX_LATE_TOLERANCE),
            },
            FormField::Text {
                key: DUE_AT_INTO_KEY,
                label: "Due time variable",
                placeholder: DEFAULT_DUE_AT_VARIABLE,
            },
            FormField::Text {
                key: ID_INTO_KEY,
                label: "Scheduled run id variable",
                placeholder: DEFAULT_SCHEDULED_ID_VARIABLE,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str(ACTION_KEY).map(|_| ())
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![
                ProducedVariable {
                    output_name_key: DUE_AT_INTO_KEY.to_owned(),
                    kind: VariantKind::Datetime,
                    label: "Due time".to_owned(),
                },
                ProducedVariable {
                    output_name_key: ID_INTO_KEY.to_owned(),
                    kind: VariantKind::Int,
                    label: "Scheduled run id".to_owned(),
                },
            ],
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, SCHEDULE_ACTION_KIND_ID);
        let request = match self.request(config, ctx) {
            Ok(request) => request,
            Err(reason) => return (timer.failed(reason), None),
        };
        let Some(scheduled_runs) = self.scheduled_runs.get() else {
            return (
                timer.failed(ScheduleError::SchedulerStopped.to_string()),
                None,
            );
        };
        let placement = match scheduled_runs.schedule(request).await {
            Ok(placement) => placement,
            Err(e) => return (timer.failed(e.to_string()), None),
        };
        let due_at_var = strip_var_decoration(
            config
                .str_nonempty(DUE_AT_INTO_KEY)
                .unwrap_or(DEFAULT_DUE_AT_VARIABLE),
        );
        let id_var = strip_var_decoration(
            config
                .str_nonempty(ID_INTO_KEY)
                .unwrap_or(DEFAULT_SCHEDULED_ID_VARIABLE),
        );
        let stack = ctx
            .arg_stack
            .clone()
            .set(due_at_var, Variant::Datetime(placement.due_at))
            .set(id_var, Variant::Int(placement.id.get()));
        (timer.success(), Some(stack))
    }
}
