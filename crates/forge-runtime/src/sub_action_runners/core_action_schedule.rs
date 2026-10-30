use std::time::Duration;

use async_trait::async_trait;
use forge_registry::{
    AmountUnit, ConfigProblem, ConfigSpec, DateTimeField, DurationBounds, DurationField,
    OptionalField, OptionalSwitch, ProducedVariable, RunContext, StepTimer, SubActionCategory,
    SubActionIo, SubActionRunner, TextEditor, TextField, ToggleField, TypedField, UnitAmountBounds,
    UnitAmountField, VariableField,
};
use forge_storage::MissedRunPolicy;
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, UserText, Variant, VariantKind};

use super::core_action_shared::ACTION;
use crate::scheduled_runs::{
    MAX_LATE_TOLERANCE, MAX_SCHEDULE_DELAY, MIN_LATE_TOLERANCE, ScheduleDue, ScheduleError,
    ScheduleIntent, ScheduleRequest, ScheduledRunsCell, SchedulingContext,
};

pub const SCHEDULE_ACTION_KIND_ID: &str = "core.action.schedule";

pub const UNIT_MINUTES: &str = "minutes";
pub const UNIT_HOURS: &str = "hours";
pub const UNIT_DAYS: &str = "days";

pub const DEFAULT_DUE_AT_VARIABLE: &str = "schedule.due_at";
pub const DEFAULT_SCHEDULED_ID_VARIABLE: &str = "schedule.id";

const DUE_AT_INTO_KEY: &str = "due_at_into_var";
const ID_INTO_KEY: &str = "id_into_var";

const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = 60 * SECONDS_PER_MINUTE;
const SECONDS_PER_DAY: u64 = 24 * SECONDS_PER_HOUR;
const DEFAULT_DELAY_MINUTES: i64 = 10;
const MIN_DELAY_AMOUNT: i64 = 1;
const DEFAULT_LATE_TOLERANCE_MINUTES: i64 = 10;

const DELAY_BOUNDS: UnitAmountBounds = UnitAmountBounds {
    min: MIN_DELAY_AMOUNT,
    max_base_units: MAX_SCHEDULE_DELAY.as_secs(),
    units: &[
        AmountUnit {
            value: UNIT_MINUTES,
            base_units: SECONDS_PER_MINUTE,
        },
        AmountUnit {
            value: UNIT_HOURS,
            base_units: SECONDS_PER_HOUR,
        },
        AmountUnit {
            value: UNIT_DAYS,
            base_units: SECONDS_PER_DAY,
        },
    ],
};

const DELAY: UnitAmountField = UnitAmountField {
    key: "delay_amount",
    unit_key: "delay_unit",
    label: "Run after",
    bounds: DELAY_BOUNDS,
    default_amount: DEFAULT_DELAY_MINUTES,
    default_unit: UNIT_MINUTES,
};

const DUE_AT: OptionalField<DateTimeField> = OptionalField {
    switch: OptionalSwitch::Toggle {
        key: "use_due_at",
        label: "Run at a date-time instead",
        default_on: false,
    },
    field: DateTimeField {
        key: "due_at",
        label: "Date-time (or %var%)",
        default: "",
    },
};

const SCHEDULE_KEY: TextField = TextField {
    key: "key",
    label: "Key (optional, replaces a pending run with the same key)",
    editor: TextEditor::Line {
        placeholder: "vip-removal:%user_id%",
    },
    default: "",
    required: false,
};

const INHERIT_ARGS: ToggleField = ToggleField {
    key: "inherit_args",
    label: "Pass current variables",
    default: true,
};

const SKIP_IF_LATE: OptionalField<DurationField> = OptionalField {
    switch: OptionalSwitch::Toggle {
        key: "skip_if_late",
        label: "Skip instead of running late if forge was closed",
        default_on: false,
    },
    field: DurationField {
        key: "late_tolerance_minutes",
        label: "Skip when late by more than",
        default: DEFAULT_LATE_TOLERANCE_MINUTES,
        bounds: DurationBounds::in_minutes(
            whole_minutes(MIN_LATE_TOLERANCE),
            whole_minutes(MAX_LATE_TOLERANCE),
        ),
    },
};

const DUE_AT_INTO: VariableField = VariableField {
    key: DUE_AT_INTO_KEY,
    label: "Due time variable",
    placeholder: DEFAULT_DUE_AT_VARIABLE,
    default: DEFAULT_DUE_AT_VARIABLE,
};

const ID_INTO: VariableField = VariableField {
    key: ID_INTO_KEY,
    label: "Scheduled run id variable",
    placeholder: DEFAULT_SCHEDULED_ID_VARIABLE,
    default: DEFAULT_SCHEDULED_ID_VARIABLE,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[
    &ACTION,
    &DELAY,
    &DUE_AT,
    &SCHEDULE_KEY,
    &INHERIT_ARGS,
    &SKIP_IF_LATE,
    &DUE_AT_INTO,
    &ID_INTO,
]);

const fn whole_minutes(duration: Duration) -> i64 {
    let minutes = duration.as_secs() / SECONDS_PER_MINUTE;
    if minutes > i64::MAX.unsigned_abs() {
        i64::MAX
    } else {
        minutes.cast_signed()
    }
}

struct ScheduleConfig {
    request: ScheduleRequest,
    due_at_var: String,
    id_var: String,
}

fn read_config(
    config: &SubActionConfig,
    ctx: &RunContext<'_>,
) -> Result<ScheduleConfig, ConfigProblem> {
    let args = ctx.arg_stack;
    let target_action_id = ACTION.read(config, args)?;
    let due = match DUE_AT.read(config, args)? {
        Some(instant) => ScheduleDue::At(instant),
        None => ScheduleDue::After(Duration::from_secs(DELAY.read(config, args)?)),
    };
    let missed_run_policy = SKIP_IF_LATE
        .read(config, args)?
        .map_or(MissedRunPolicy::RunLateOnce, MissedRunPolicy::SkipIfLateBy);
    let request = SchedulingContext::of_run(ctx).request(ScheduleIntent {
        target_action_id,
        due,
        key: Some(SCHEDULE_KEY.read(config, args)?),
        missed_run_policy,
        inherit_args: INHERIT_ARGS.read(config, args)?,
    });
    Ok(ScheduleConfig {
        request,
        due_at_var: DUE_AT_INTO.read(config, args)?,
        id_var: ID_INTO.read(config, args)?,
    })
}

pub struct CoreActionScheduleRunner {
    scheduled_runs: ScheduledRunsCell,
}

impl CoreActionScheduleRunner {
    pub fn new(scheduled_runs: ScheduledRunsCell) -> Self {
        Self { scheduled_runs }
    }
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
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
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, SCHEDULE_ACTION_KIND_ID);
        let ScheduleConfig {
            request,
            due_at_var,
            id_var,
        } = match read_config(config, ctx) {
            Ok(read) => read,
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
        let placement = match scheduled_runs.schedule(request).await {
            Ok(placement) => placement,
            Err(e) => return (timer.failed(UserText::unlocalized(e.to_string())), None),
        };
        let stack = ctx
            .arg_stack
            .clone()
            .set(due_at_var, Variant::Datetime(placement.due_at))
            .set(id_var, Variant::Int(placement.id.get()));
        (timer.success(), Some(stack))
    }
}
