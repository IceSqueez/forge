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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use forge_events::{Event, EventPublisher};
    use forge_storage::GlobalsRepo;
    use forge_types::EventId;
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;

    use super::*;
    use crate::ForgeApi;
    use crate::ScriptError;
    use crate::engine::{Engine, EngineConfig};
    use crate::schedule::ScriptScheduleError;
    use crate::test_support::sandboxed_backend;

    const DUE_UNIX_SECS: i64 = 1_791_118_800;
    const PLACED_ID: i64 = 42;

    struct NullPublisher;

    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    struct RecordingScheduler {
        requests: Mutex<Vec<ScriptScheduleRequest>>,
        cancelled_keys: Mutex<Vec<String>>,
        outcome: Result<(), ScriptScheduleError>,
        key_was_pending: bool,
    }

    impl RecordingScheduler {
        fn answering(outcome: Result<(), ScriptScheduleError>, key_was_pending: bool) -> Arc<Self> {
            Arc::new(Self {
                requests: Mutex::new(Vec::new()),
                cancelled_keys: Mutex::new(Vec::new()),
                outcome,
                key_was_pending,
            })
        }

        fn placing() -> Arc<Self> {
            Self::answering(Ok(()), true)
        }

        fn requests(&self) -> Vec<ScriptScheduleRequest> {
            self.requests.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl ActionScheduler for RecordingScheduler {
        async fn schedule(
            &self,
            request: ScriptScheduleRequest,
        ) -> Result<ScriptSchedulePlacement, ScriptScheduleError> {
            self.requests.lock().unwrap().push(request);
            self.outcome.clone().map(|()| ScriptSchedulePlacement {
                id: PLACED_ID,
                due_at: due(),
            })
        }

        async fn cancel_by_key(&self, key: &str) -> Result<bool, ScriptScheduleError> {
            self.cancelled_keys.lock().unwrap().push(key.to_owned());
            self.outcome.clone().map(|()| self.key_was_pending)
        }
    }

    fn due() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(DUE_UNIX_SECS).unwrap()
    }

    fn due_text() -> String {
        due().format(&Rfc3339).unwrap()
    }

    fn defaults(action: &str, due: ScriptScheduleDue) -> ScriptScheduleRequest {
        ScriptScheduleRequest {
            action_id_or_name: action.to_owned(),
            due,
            key: None,
            inherit_args: true,
            skip_if_late_minutes: None,
        }
    }

    async fn eval(
        scheduler: Option<Arc<RecordingScheduler>>,
        script: String,
    ) -> Result<Dynamic, ScriptError> {
        let backend = sandboxed_backend([0xab; 32]).await.map(Arc::new);
        let mut api = ForgeApi::new(
            Arc::new(NullPublisher),
            Arc::clone(&backend) as Arc<dyn GlobalsRepo>,
            EventId::new(),
            Instant::now() + Duration::from_secs(10),
        );
        if let Some(scheduler) = scheduler {
            api = api.with_action_scheduler(scheduler as Arc<dyn ActionScheduler>);
        }
        let engine = Engine::with_api(EngineConfig::default(), api);
        tokio::task::spawn_blocking(move || engine.eval_script(&script))
            .await
            .unwrap()
    }

    async fn eval_ok(scheduler: Option<Arc<RecordingScheduler>>, script: String) {
        let result = eval(scheduler, script).await;
        assert!(result.is_ok(), "{result:?}");
    }

    fn runtime_reason(result: Result<Dynamic, ScriptError>) -> String {
        match result {
            Err(ScriptError::Runtime { reason, .. }) => reason,
            other => panic!("expected a runtime error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn after_without_options_asks_for_the_delay_inheriting_variables_with_no_key() {
        let scheduler = RecordingScheduler::placing();

        eval_ok(
            Some(Arc::clone(&scheduler)),
            r#"forge::schedule::after("Greet", 90)"#.to_owned(),
        )
        .await;

        assert_eq!(
            scheduler.requests(),
            [defaults("Greet", ScriptScheduleDue::AfterSeconds(90))]
        );
    }

    #[tokio::test]
    async fn each_option_sets_only_its_own_field_of_the_request() {
        let base = defaults("Greet", ScriptScheduleDue::AfterSeconds(90));
        for (options, expected) in [
            (
                r#"#{key: "raid-thanks"}"#,
                ScriptScheduleRequest {
                    key: Some("raid-thanks".to_owned()),
                    ..base.clone()
                },
            ),
            (
                "#{inherit_args: false}",
                ScriptScheduleRequest {
                    inherit_args: false,
                    ..base.clone()
                },
            ),
            (
                "#{skip_if_late_minutes: 5}",
                ScriptScheduleRequest {
                    skip_if_late_minutes: Some(5),
                    ..base.clone()
                },
            ),
            (
                r#"#{key: "k", inherit_args: false, skip_if_late_minutes: -1}"#,
                ScriptScheduleRequest {
                    key: Some("k".to_owned()),
                    inherit_args: false,
                    skip_if_late_minutes: Some(-1),
                    ..base.clone()
                },
            ),
            ("#{}", base.clone()),
        ] {
            let scheduler = RecordingScheduler::placing();

            eval_ok(
                Some(Arc::clone(&scheduler)),
                format!(r#"forge::schedule::after("Greet", 90, {options})"#),
            )
            .await;

            assert_eq!(scheduler.requests(), [expected], "{options}");
        }
    }

    #[tokio::test]
    async fn a_mistyped_or_unknown_option_fails_the_script_without_scheduling() {
        for (options, reason) in [
            ("#{key: 5}", "schedule option 'key' must be a string"),
            (
                r#"#{inherit_args: "no"}"#,
                "schedule option 'inherit_args' must be a bool",
            ),
            (
                "#{skip_if_late_minutes: 1.5}",
                "schedule option 'skip_if_late_minutes' must be an int",
            ),
            ("#{delay: 5}", "unknown schedule option 'delay'"),
        ] {
            let scheduler = RecordingScheduler::placing();

            let result = eval(
                Some(Arc::clone(&scheduler)),
                format!(r#"forge::schedule::after("Greet", 90, {options})"#),
            )
            .await;

            let failure = runtime_reason(result);
            assert!(failure.contains(reason), "{options}: {failure}");
            assert!(scheduler.requests().is_empty(), "{options}");
        }
    }

    #[tokio::test]
    async fn at_hands_the_due_time_over_as_text_or_unix_seconds_with_and_without_options() {
        let text = due_text();
        let as_text = ScriptScheduleDue::At(Variant::String(text.clone()));
        let as_unix = ScriptScheduleDue::At(Variant::Int(DUE_UNIX_SECS));
        for (call, expected) in [
            (
                format!(r#"("Greet", "{text}")"#),
                defaults("Greet", as_text.clone()),
            ),
            (
                format!(r#"("Greet", "{text}", #{{key: "k"}})"#),
                ScriptScheduleRequest {
                    key: Some("k".to_owned()),
                    ..defaults("Greet", as_text)
                },
            ),
            (
                format!(r#"("Greet", {DUE_UNIX_SECS})"#),
                defaults("Greet", as_unix.clone()),
            ),
            (
                format!(r#"("Greet", {DUE_UNIX_SECS}, #{{key: "k"}})"#),
                ScriptScheduleRequest {
                    key: Some("k".to_owned()),
                    ..defaults("Greet", as_unix)
                },
            ),
        ] {
            let scheduler = RecordingScheduler::placing();

            eval_ok(
                Some(Arc::clone(&scheduler)),
                format!("forge::schedule::at{call}"),
            )
            .await;

            assert_eq!(scheduler.requests(), [expected], "{call}");
        }
    }

    #[tokio::test]
    async fn a_placed_run_returns_its_id_and_due_time_as_rfc3339_text() {
        let placed = eval(
            Some(RecordingScheduler::placing()),
            r#"let r = forge::schedule::after("Greet", 90); `${r.id}|${r.due_at}`"#.to_owned(),
        )
        .await
        .unwrap();

        assert_eq!(
            placed.into_string().unwrap(),
            format!("{PLACED_ID}|{}", due_text())
        );
    }

    #[tokio::test]
    async fn cancel_passes_the_key_through_and_returns_whether_a_run_was_pending() {
        for key_was_pending in [true, false] {
            let scheduler = RecordingScheduler::answering(Ok(()), key_was_pending);

            let cancelled = eval(
                Some(Arc::clone(&scheduler)),
                r#"forge::schedule::cancel("raid-thanks")"#.to_owned(),
            )
            .await
            .unwrap();

            assert_eq!(cancelled.as_bool().unwrap(), key_was_pending);
            assert_eq!(
                *scheduler.cancelled_keys.lock().unwrap(),
                ["raid-thanks".to_owned()]
            );
        }
    }

    #[tokio::test]
    async fn a_scheduler_refusal_fails_the_script_with_its_message() {
        let refusal = ScriptScheduleError::AmbiguousAction {
            name: "Greet".to_owned(),
            count: 2,
        };
        for script in [
            r#"forge::schedule::after("Greet", 90)"#,
            r#"forge::schedule::cancel("raid-thanks")"#,
        ] {
            let scheduler = RecordingScheduler::answering(Err(refusal.clone()), false);

            let failure = runtime_reason(eval(Some(scheduler), script.to_owned()).await);

            assert!(
                failure.contains(&refusal.to_string()),
                "{script}: {failure}"
            );
        }
    }

    #[tokio::test]
    async fn every_schedule_function_fails_with_the_unavailable_reason_without_a_scheduler() {
        for script in [
            r#"forge::schedule::after("Greet", 90)"#,
            r#"forge::schedule::after("Greet", 90, #{})"#,
            r#"forge::schedule::at("Greet", "2026-10-04T12:00:00Z")"#,
            r#"forge::schedule::at("Greet", "2026-10-04T12:00:00Z", #{})"#,
            "forge::schedule::at(\"Greet\", 1791118800)",
            "forge::schedule::at(\"Greet\", 1791118800, #{})",
            r#"forge::schedule::cancel("raid-thanks")"#,
        ] {
            let failure = runtime_reason(eval(None, script.to_owned()).await);

            assert!(
                failure.contains(SCHEDULING_UNAVAILABLE_REASON),
                "{script}: {failure}"
            );
        }
    }
}
