use std::sync::Arc;

use forge_types::Variant;
use rhai::{Dynamic, EvalAltResult, INT, ImmutableString, Map, Module};
use tokio::runtime::Handle;

use crate::convert::variant_to_dynamic;
use crate::schedule::{
    ActionScheduler, ScriptScheduleDue, ScriptSchedulePlacement, ScriptScheduleRequest,
};

pub const SCHEDULE_KEY_OPTION: &str = "key";
pub const SCHEDULE_INHERIT_ARGS_OPTION: &str = "inherit_args";
pub const SCHEDULE_SKIP_IF_LATE_MINUTES_OPTION: &str = "skip_if_late_minutes";

pub const PLACEMENT_ID_FIELD: &str = "id";
pub const PLACEMENT_DUE_AT_FIELD: &str = "due_at";

pub const SCHEDULING_UNAVAILABLE_REASON: &str =
    "scheduled runs are not available where this script runs";

type Scheduler = Option<Arc<dyn ActionScheduler>>;
type Scripted<T> = Result<T, Box<EvalAltResult>>;

pub(crate) fn build_schedule_module(scheduler: Scheduler) -> Module {
    let mut m = Module::new();

    let after = scheduler.clone();
    m.set_native_fn(
        "after",
        move |action: ImmutableString, seconds: INT| -> Scripted<Map> {
            schedule(
                &after,
                &action,
                ScriptScheduleDue::AfterSeconds(seconds),
                Map::new(),
            )
        },
    );

    let after_with_options = scheduler.clone();
    m.set_native_fn(
        "after",
        move |action: ImmutableString, seconds: INT, options: Map| -> Scripted<Map> {
            schedule(
                &after_with_options,
                &action,
                ScriptScheduleDue::AfterSeconds(seconds),
                options,
            )
        },
    );

    let at_text = scheduler.clone();
    m.set_native_fn(
        "at",
        move |action: ImmutableString, when: ImmutableString| -> Scripted<Map> {
            schedule(&at_text, &action, due_at_text(&when), Map::new())
        },
    );

    let at_text_with_options = scheduler.clone();
    m.set_native_fn(
        "at",
        move |action: ImmutableString, when: ImmutableString, options: Map| -> Scripted<Map> {
            schedule(&at_text_with_options, &action, due_at_text(&when), options)
        },
    );

    let at_unix = scheduler.clone();
    m.set_native_fn(
        "at",
        move |action: ImmutableString, unix_seconds: INT| -> Scripted<Map> {
            schedule(&at_unix, &action, due_at_unix(unix_seconds), Map::new())
        },
    );

    let at_unix_with_options = scheduler.clone();
    m.set_native_fn(
        "at",
        move |action: ImmutableString, unix_seconds: INT, options: Map| -> Scripted<Map> {
            schedule(
                &at_unix_with_options,
                &action,
                due_at_unix(unix_seconds),
                options,
            )
        },
    );

    m.set_native_fn("cancel", move |key: ImmutableString| -> Scripted<bool> {
        let scheduler = available(&scheduler)?;
        Handle::current()
            .block_on(scheduler.cancel_by_key(key.as_str()))
            .map_err(|e| e.to_string().into())
    });

    m
}

fn due_at_text(when: &ImmutableString) -> ScriptScheduleDue {
    ScriptScheduleDue::At(Variant::String(when.to_string()))
}

fn due_at_unix(unix_seconds: INT) -> ScriptScheduleDue {
    ScriptScheduleDue::At(Variant::Int(unix_seconds))
}

fn available(scheduler: &Scheduler) -> Scripted<&Arc<dyn ActionScheduler>> {
    scheduler
        .as_ref()
        .ok_or_else(|| SCHEDULING_UNAVAILABLE_REASON.into())
}

fn schedule(
    scheduler: &Scheduler,
    action: &str,
    due: ScriptScheduleDue,
    options: Map,
) -> Scripted<Map> {
    let scheduler = available(scheduler)?;
    let request = schedule_request(action, due, options)?;
    let placement = Handle::current()
        .block_on(scheduler.schedule(request))
        .map_err(|e| -> Box<EvalAltResult> { e.to_string().into() })?;
    Ok(placement_map(placement))
}

fn schedule_request(
    action: &str,
    due: ScriptScheduleDue,
    options: Map,
) -> Scripted<ScriptScheduleRequest> {
    let mut request = ScriptScheduleRequest {
        action_id_or_name: action.to_owned(),
        due,
        key: None,
        inherit_args: true,
        skip_if_late_minutes: None,
    };
    for (name, value) in options {
        match name.as_str() {
            SCHEDULE_KEY_OPTION => request.key = Some(typed_option(&name, value, "a string")?),
            SCHEDULE_INHERIT_ARGS_OPTION => {
                request.inherit_args = typed_option(&name, value, "a bool")?
            }
            SCHEDULE_SKIP_IF_LATE_MINUTES_OPTION => {
                request.skip_if_late_minutes = Some(typed_option(&name, value, "an int")?)
            }
            other => return Err(format!("unknown schedule option '{other}'").into()),
        }
    }
    Ok(request)
}

fn typed_option<T: Clone + 'static>(name: &str, value: Dynamic, expected: &str) -> Scripted<T> {
    value
        .try_cast::<T>()
        .ok_or_else(|| format!("schedule option '{name}' must be {expected}").into())
}

fn placement_map(placement: ScriptSchedulePlacement) -> Map {
    let mut map = Map::new();
    map.insert(PLACEMENT_ID_FIELD.into(), Dynamic::from(placement.id));
    map.insert(
        PLACEMENT_DUE_AT_FIELD.into(),
        variant_to_dynamic(Variant::Datetime(placement.due_at)),
    );
    map
}
