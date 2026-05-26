mod delay;
mod delete_global;
mod get_global;
mod increment_global;
mod log;
mod obs;
mod play_sound;
mod random_int;
mod read_file;
mod run_script;
mod send_chat;
mod set_global;
mod speak;

use std::sync::Arc;

use forge_obs::ObsSink;
use forge_storage::{DataProvider, GlobalsRepo};
use forge_types::{ArgStack, EventId, SubActionOutcome, SubActionSpec, SubActionTelemetry};
use time::OffsetDateTime;

use crate::EventBus;
use crate::script_registry::ScriptRegistry;
use crate::sound_player::SoundPlayer;
use crate::speak_dispatcher::SpeakDispatcher;

pub(crate) async fn interpolate_with_globals(
    template: &str,
    arg_stack: &ArgStack,
    globals: &dyn GlobalsRepo,
) -> String {
    let after_args = arg_stack.interpolate(template);
    if !after_args.contains('%') {
        return after_args;
    }
    let mut result = String::with_capacity(after_args.len());
    let mut chars = after_args.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            result.push(ch);
            continue;
        }
        let token_start = result.len();
        result.push('%');
        let mut key = String::new();
        let mut closed = false;
        for inner in chars.by_ref() {
            if inner == '%' {
                closed = true;
                break;
            }
            key.push(inner);
        }
        if !closed {
            continue;
        }
        match globals.get(&key).await {
            Ok(Some(value)) => {
                result.truncate(token_start);
                result.push_str(&value.to_string());
            }
            _ => {
                result.push_str(&key);
                result.push('%');
            }
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
pub async fn dispatch(
    spec: &SubActionSpec,
    arg_stack: &ArgStack,
    index: usize,
    parent_event_id: EventId,
    bus: &Arc<EventBus>,
    dp: Arc<dyn DataProvider>,
    registry: Option<&ScriptRegistry>,
    obs_sink: Option<Arc<dyn ObsSink>>,
    sound_player: Option<&Arc<dyn SoundPlayer>>,
    speak_dispatcher: Option<&Arc<dyn SpeakDispatcher>>,
) -> (SubActionTelemetry, Option<ArgStack>) {
    match spec {
        SubActionSpec::Log { message, .. } => {
            let interpolated =
                interpolate_with_globals(message, arg_stack, dp.as_ref() as &dyn GlobalsRepo).await;
            (log::run(spec, index, &interpolated), None)
        }
        SubActionSpec::SendChat { .. } => {
            let t = send_chat::run(
                spec,
                arg_stack,
                index,
                parent_event_id,
                bus,
                dp.as_ref() as &dyn GlobalsRepo,
            )
            .await;
            (t, None)
        }
        SubActionSpec::Delay { .. } => (delay::run(spec, index).await, None),
        SubActionSpec::SetGlobal { .. } => {
            let t = set_global::run(
                spec,
                arg_stack,
                index,
                parent_event_id,
                bus,
                dp.as_ref() as &dyn GlobalsRepo,
            )
            .await;
            (t, None)
        }
        SubActionSpec::GetGlobal { .. } => {
            get_global::run(spec, arg_stack, index, dp.as_ref() as &dyn GlobalsRepo).await
        }
        SubActionSpec::IncrementGlobal { .. } => {
            let t = increment_global::run(
                spec,
                arg_stack,
                index,
                parent_event_id,
                bus,
                dp.as_ref() as &dyn GlobalsRepo,
            )
            .await;
            (t, None)
        }
        SubActionSpec::DeleteGlobal { .. } => {
            let t = delete_global::run(
                spec,
                arg_stack,
                index,
                parent_event_id,
                bus,
                dp.as_ref() as &dyn GlobalsRepo,
            )
            .await;
            (t, None)
        }
        SubActionSpec::RunScript { script_name } => {
            let Some(reg) = registry else {
                return (
                    SubActionTelemetry {
                        kind: "RunScript".to_string(),
                        started_at: OffsetDateTime::now_utc(),
                        duration_ms: 0,
                        outcome: SubActionOutcome::Skipped(
                            "script registry unavailable".to_string(),
                        ),
                        index,
                    },
                    None,
                );
            };
            run_script::run(script_name, arg_stack, index, parent_event_id, bus, dp, reg).await
        }
        SubActionSpec::ObsSetScene { .. }
        | SubActionSpec::ObsSetSourceVisible { .. }
        | SubActionSpec::ObsSetInputMute { .. }
        | SubActionSpec::ObsStartRecord
        | SubActionSpec::ObsStopRecord
        | SubActionSpec::ObsStartStream
        | SubActionSpec::ObsStopStream
        | SubActionSpec::ObsRaw { .. } => obs::run(spec, index, obs_sink).await,
        SubActionSpec::PlaySound { .. } => play_sound::run(spec, index, sound_player).await,
        SubActionSpec::Speak { .. } => speak::run(spec, index, speak_dispatcher).await,
        SubActionSpec::ReadFile { .. } => {
            let t = read_file::run(
                spec,
                arg_stack,
                index,
                parent_event_id,
                bus,
                dp.as_ref() as &dyn GlobalsRepo,
            )
            .await;
            (t, None)
        }
        SubActionSpec::RandomInt { .. } => {
            let t = random_int::run(
                spec,
                arg_stack,
                index,
                parent_event_id,
                bus,
                dp.as_ref() as &dyn GlobalsRepo,
            )
            .await;
            (t, None)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{EventBus, NullEventLogRepo};
    use forge_storage::DataProvider;
    use forge_storage::GlobalsRepo;
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{ArgStack, EventId, LogLevel, SubActionOutcome, SubActionSpec, Variant};
    use std::sync::Arc;
    use std::time::Duration;

    async fn make_dp() -> Arc<dyn DataProvider> {
        Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        )
    }

    #[tokio::test]
    async fn log_dispatch_returns_success_telemetry() {
        let dp = make_dp().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let spec = SubActionSpec::Log {
            level: LogLevel::Info,
            message: "hello".to_string(),
        };
        let (telemetry, updated) = dispatch(
            &spec,
            &ArgStack::new(),
            0,
            EventId::new(),
            &bus,
            Arc::clone(&dp),
            None,
            None,
            None,
            None,
        )
        .await;
        assert_eq!(telemetry.kind, "Log");
        assert_eq!(telemetry.index, 0);
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert!(updated.is_none());
    }

    #[tokio::test]
    async fn log_dispatch_interpolates_message() {
        let dp = make_dp().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let spec = SubActionSpec::Log {
            level: LogLevel::Info,
            message: "hello %user%".to_string(),
        };
        let stack = ArgStack::new().set("user".to_string(), Variant::String("alice".to_string()));
        let (telemetry, _) = dispatch(
            &spec,
            &stack,
            0,
            EventId::new(),
            &bus,
            Arc::clone(&dp),
            None,
            None,
            None,
            None,
        )
        .await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
    }

    #[tokio::test]
    async fn send_chat_publishes_chat_send_request_with_correct_caused_by() {
        let dp = make_dp().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let parent_id = EventId::new();
        let spec = SubActionSpec::SendChat {
            message: "hello %user%".to_string(),
            target: "twitch".to_string(),
        };
        let stack = ArgStack::new().set("user".to_string(), Variant::String("alice".to_string()));
        dispatch(
            &spec,
            &stack,
            0,
            parent_id,
            &bus,
            Arc::clone(&dp),
            None,
            None,
            None,
            None,
        )
        .await;
        let event = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.kind, "chat.send.request");
        assert_eq!(event.caused_by, Some(parent_id));
        assert_eq!(event.payload["message"].as_str(), Some("hello alice"));
        assert_eq!(event.payload["target"].as_str(), Some("twitch"));
    }

    #[tokio::test]
    async fn set_global_stores_integer_value() {
        let dp = make_dp().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let spec = SubActionSpec::SetGlobal {
            name: "counter".to_string(),
            value: "42".to_string(),
        };
        dispatch(
            &spec,
            &ArgStack::new(),
            0,
            EventId::new(),
            &bus,
            Arc::clone(&dp),
            None,
            None,
            None,
            None,
        )
        .await;
        let val = GlobalsRepo::get(dp.as_ref(), "counter").await.unwrap();
        assert!(matches!(val, Some(Variant::Int(42))));
    }

    #[tokio::test]
    async fn set_global_interpolates_value_from_arg_stack() {
        let dp = make_dp().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let spec = SubActionSpec::SetGlobal {
            name: "greeting".to_string(),
            value: "%user%".to_string(),
        };
        let stack = ArgStack::new().set("user".to_string(), Variant::String("alice".to_string()));
        dispatch(
            &spec,
            &stack,
            0,
            EventId::new(),
            &bus,
            Arc::clone(&dp),
            None,
            None,
            None,
            None,
        )
        .await;
        let val = GlobalsRepo::get(dp.as_ref(), "greeting").await.unwrap();
        assert!(matches!(val, Some(Variant::String(s)) if s == "alice"));
    }

    #[tokio::test]
    async fn set_global_emits_global_set_event_with_key_field() {
        let dp = make_dp().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let parent_id = EventId::new();
        let spec = SubActionSpec::SetGlobal {
            name: "x".to_string(),
            value: "100".to_string(),
        };
        dispatch(
            &spec,
            &ArgStack::new(),
            0,
            parent_id,
            &bus,
            Arc::clone(&dp),
            None,
            None,
            None,
            None,
        )
        .await;
        let event = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.kind, "global.set");
        assert_eq!(event.caused_by, Some(parent_id));
        assert_eq!(event.payload["key"].as_str(), Some("x"));
        assert_eq!(event.payload["new_value"].as_str(), Some("100"));
        assert!(event.payload.get("prev_value").is_none());
    }

    #[tokio::test]
    async fn set_global_emits_prev_value_when_key_existed() {
        let dp = make_dp().await;
        GlobalsRepo::set(dp.as_ref(), "score", Variant::Int(10), false)
            .await
            .unwrap();
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let spec = SubActionSpec::SetGlobal {
            name: "score".to_string(),
            value: "20".to_string(),
        };
        dispatch(
            &spec,
            &ArgStack::new(),
            0,
            EventId::new(),
            &bus,
            Arc::clone(&dp),
            None,
            None,
            None,
            None,
        )
        .await;
        let event = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.kind, "global.set");
        assert_eq!(event.payload["key"].as_str(), Some("score"));
        assert_eq!(event.payload["new_value"].as_str(), Some("20"));
        assert_eq!(event.payload["prev_value"].as_str(), Some("10"));
    }

    #[tokio::test]
    async fn increment_global_via_dispatch_emits_global_incr_with_key_and_delta() {
        let dp = make_dp().await;
        GlobalsRepo::set(dp.as_ref(), "hits", Variant::Int(5), false)
            .await
            .unwrap();
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let parent_id = EventId::new();
        let spec = SubActionSpec::IncrementGlobal {
            name: "hits".to_string(),
            amount: 2,
        };
        dispatch(
            &spec,
            &ArgStack::new(),
            0,
            parent_id,
            &bus,
            Arc::clone(&dp),
            None,
            None,
            None,
            None,
        )
        .await;
        let event = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.kind, "global.incr");
        assert_eq!(event.caused_by, Some(parent_id));
        assert_eq!(event.payload["key"].as_str(), Some("hits"));
        assert_eq!(event.payload["delta"].as_i64(), Some(2));
        assert_eq!(event.payload["new_value"].as_i64(), Some(7));
    }

    #[tokio::test]
    async fn delete_global_via_dispatch_emits_global_del_with_key() {
        let dp = make_dp().await;
        GlobalsRepo::set(dp.as_ref(), "temp", Variant::Int(1), false)
            .await
            .unwrap();
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let parent_id = EventId::new();
        let spec = SubActionSpec::DeleteGlobal {
            name: "temp".to_string(),
        };
        dispatch(
            &spec,
            &ArgStack::new(),
            0,
            parent_id,
            &bus,
            Arc::clone(&dp),
            None,
            None,
            None,
            None,
        )
        .await;
        let event = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.kind, "global.del");
        assert_eq!(event.caused_by, Some(parent_id));
        assert_eq!(event.payload["key"].as_str(), Some("temp"));
    }

    #[tokio::test]
    async fn get_global_dispatch_returns_updated_stack() {
        let dp = make_dp().await;
        GlobalsRepo::set(dp.as_ref(), "counter", Variant::Int(7), false)
            .await
            .unwrap();
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let spec = SubActionSpec::GetGlobal {
            name: "counter".to_string(),
            into_arg: "x".to_string(),
        };
        let (telemetry, updated) = dispatch(
            &spec,
            &ArgStack::new(),
            0,
            EventId::new(),
            &bus,
            Arc::clone(&dp),
            None,
            None,
            None,
            None,
        )
        .await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        let new_stack = updated.unwrap();
        assert_eq!(new_stack.get("x"), Some(&Variant::Int(7)));
    }

    #[tokio::test]
    async fn interpolate_with_globals_falls_back_to_global() {
        let dp = make_dp().await;
        GlobalsRepo::set(dp.as_ref(), "counter", Variant::Int(7), false)
            .await
            .unwrap();
        let stack = ArgStack::new().set("user".to_string(), Variant::String("alice".to_string()));
        let result = interpolate_with_globals(
            "Hello %user%, count is %counter%",
            &stack,
            dp.as_ref() as &dyn GlobalsRepo,
        )
        .await;
        assert_eq!(result, "Hello alice, count is 7");
    }

    #[tokio::test]
    async fn interpolate_with_globals_unresolved_remains_verbatim() {
        let dp = make_dp().await;
        let stack = ArgStack::new();
        let result =
            interpolate_with_globals("%ghost%", &stack, dp.as_ref() as &dyn GlobalsRepo).await;
        assert_eq!(result, "%ghost%");
    }

    #[tokio::test]
    async fn interpolate_with_globals_arg_stack_takes_priority() {
        let dp = make_dp().await;
        GlobalsRepo::set(dp.as_ref(), "x", Variant::Int(99), false)
            .await
            .unwrap();
        let stack = ArgStack::new().set("x".to_string(), Variant::String("from_stack".to_string()));
        let result = interpolate_with_globals("%x%", &stack, dp.as_ref() as &dyn GlobalsRepo).await;
        assert_eq!(result, "from_stack");
    }

    // Regression: exit criterion #4 — reads counter must increment when a %var% token is
    // resolved via GlobalsRepo::get inside interpolate_with_globals.
    #[tokio::test]
    async fn interpolation_increments_reads_counter() {
        let dp = make_dp().await;
        GlobalsRepo::set(dp.as_ref(), "score", Variant::Int(42), false)
            .await
            .unwrap();

        let _ = interpolate_with_globals(
            "Player score: %score%",
            &ArgStack::new(),
            dp.as_ref() as &dyn GlobalsRepo,
        )
        .await;
        let _ = interpolate_with_globals(
            "%score% points",
            &ArgStack::new(),
            dp.as_ref() as &dyn GlobalsRepo,
        )
        .await;

        let entries = GlobalsRepo::list(dp.as_ref()).await.unwrap();
        let entry = entries.iter().find(|e| e.name == "score").unwrap();
        assert_eq!(
            entry.reads, 2,
            "two interpolations of %score% must yield reads == 2"
        );
    }

    #[tokio::test]
    async fn run_script_skipped_when_registry_is_none() {
        let dp = make_dp().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let spec = SubActionSpec::RunScript {
            script_name: "anything".to_string(),
        };
        let (telemetry, updated) = dispatch(
            &spec,
            &ArgStack::new(),
            0,
            EventId::new(),
            &bus,
            Arc::clone(&dp),
            None,
            None,
            None,
            None,
        )
        .await;
        assert!(
            matches!(telemetry.outcome, SubActionOutcome::Skipped(_)),
            "RunScript with None registry must yield Skipped"
        );
        assert!(updated.is_none());
    }

    #[tokio::test]
    async fn run_script_not_found_returns_failed() {
        use crate::ScriptRegistry;

        let dp = make_dp().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let registry = ScriptRegistry::new();
        let spec = SubActionSpec::RunScript {
            script_name: "no_such_script".to_string(),
        };
        let (telemetry, _) = dispatch(
            &spec,
            &ArgStack::new(),
            0,
            EventId::new(),
            &bus,
            Arc::clone(&dp),
            Some(&registry),
            None,
            None,
            None,
        )
        .await;
        assert!(
            matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
            "RunScript with unknown name must yield Failed"
        );
    }

    #[tokio::test]
    async fn run_script_valid_script_publishes_script_exec_event() {
        use crate::ScriptRegistry;
        use forge_storage::ScriptRepo;
        use forge_storage_sqlite::SqliteBackend;
        use forge_types::{ScriptContract, ScriptId};
        use std::time::Duration;
        use time::OffsetDateTime;

        let dp: Arc<dyn DataProvider> = Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        );
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();

        let ts = OffsetDateTime::now_utc();
        let script_id = ScriptId::new();
        let record = forge_storage::ScriptRecord {
            id: script_id,
            name: "hello_script".to_owned(),
            body: "let x = 1 + 1; x".to_owned(),
            contract: ScriptContract::default(),
            body_hash: "h1".to_owned(),
            enabled: true,
            created_at: ts,
            last_modified: ts,
        };
        ScriptRepo::save(dp.as_ref(), record).await.unwrap();

        let registry = ScriptRegistry::new();
        registry.load_all(dp.as_ref()).await.unwrap();

        let parent_id = EventId::new();
        let spec = SubActionSpec::RunScript {
            script_name: "hello_script".to_string(),
        };
        let (telemetry, _) = dispatch(
            &spec,
            &ArgStack::new(),
            0,
            parent_id,
            &bus,
            Arc::clone(&dp),
            Some(&registry),
            None,
            None,
            None,
        )
        .await;

        assert!(
            matches!(telemetry.outcome, SubActionOutcome::Success),
            "valid script must return Success, got: {:?}",
            telemetry.outcome
        );

        let event = tokio::time::timeout(Duration::from_millis(500), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.kind, "script.exec");
        assert_eq!(event.caused_by, Some(parent_id));
        assert_eq!(
            event.payload["script_id"].as_str(),
            Some(script_id.to_string().as_str())
        );
    }

    #[tokio::test]
    async fn run_script_erroring_script_publishes_exec_then_error_with_causation_chain() {
        use crate::ScriptRegistry;
        use forge_storage::ScriptRepo;
        use forge_storage_sqlite::SqliteBackend;
        use forge_types::{ScriptContract, ScriptId};
        use std::time::Duration;
        use time::OffsetDateTime;

        let dp: Arc<dyn DataProvider> = Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        );
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();

        let ts = OffsetDateTime::now_utc();
        let record = forge_storage::ScriptRecord {
            id: ScriptId::new(),
            name: "crashing_script".to_owned(),
            body: r#"throw "intentional error";"#.to_owned(),
            contract: ScriptContract::default(),
            body_hash: "h2".to_owned(),
            enabled: true,
            created_at: ts,
            last_modified: ts,
        };
        ScriptRepo::save(dp.as_ref(), record).await.unwrap();

        let registry = ScriptRegistry::new();
        registry.load_all(dp.as_ref()).await.unwrap();

        let spec = SubActionSpec::RunScript {
            script_name: "crashing_script".to_string(),
        };
        let (telemetry, _) = dispatch(
            &spec,
            &ArgStack::new(),
            0,
            EventId::new(),
            &bus,
            Arc::clone(&dp),
            Some(&registry),
            None,
            None,
            None,
        )
        .await;

        assert!(
            matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
            "erroring script must return Failed"
        );

        let exec_event = tokio::time::timeout(Duration::from_millis(500), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(exec_event.kind, "script.exec");
        let exec_event_id = exec_event.id;

        let error_event = tokio::time::timeout(Duration::from_millis(500), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(error_event.kind, "script.error");
        assert_eq!(
            error_event.caused_by,
            Some(exec_event_id),
            "script.error.caused_by must point to script.exec event"
        );
        assert!(
            error_event.payload.get("error_type").is_some(),
            "script.error payload must include error_type"
        );
    }

    #[tokio::test]
    async fn run_script_globals_write_visible_after_execution() {
        use crate::ScriptRegistry;
        use forge_storage::ScriptRepo;
        use forge_storage_sqlite::SqliteBackend;
        use forge_types::{ScriptContract, ScriptId};
        use time::OffsetDateTime;

        let dp: Arc<dyn DataProvider> = Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        );
        let bus = EventBus::new(Arc::new(NullEventLogRepo));

        let ts = OffsetDateTime::now_utc();
        let record = forge_storage::ScriptRecord {
            id: ScriptId::new(),
            name: "writer_script".to_owned(),
            body: r#"forge::globals::set("result", 77, false);"#.to_owned(),
            contract: ScriptContract::default(),
            body_hash: "h3".to_owned(),
            enabled: true,
            created_at: ts,
            last_modified: ts,
        };
        ScriptRepo::save(dp.as_ref(), record).await.unwrap();

        let registry = ScriptRegistry::new();
        registry.load_all(dp.as_ref()).await.unwrap();

        let spec = SubActionSpec::RunScript {
            script_name: "writer_script".to_string(),
        };
        let (telemetry, _) = dispatch(
            &spec,
            &ArgStack::new(),
            0,
            EventId::new(),
            &bus,
            Arc::clone(&dp),
            Some(&registry),
            None,
            None,
            None,
        )
        .await;

        assert!(
            matches!(telemetry.outcome, SubActionOutcome::Success),
            "globals write script must succeed"
        );

        let stored = GlobalsRepo::get(dp.as_ref(), "result").await.unwrap();
        assert_eq!(stored, Some(Variant::Int(77)));
    }
}
