mod delay;
mod delete_global;
mod get_global;
mod increment_global;
mod log;
mod send_chat;
mod set_global;

use forge_storage::{DataProvider, GlobalsRepo};
use forge_types::{ArgStack, EventId, SubActionSpec, SubActionTelemetry};

use crate::EventBus;

pub(crate) async fn interpolate_with_globals(
    template: &str,
    arg_stack: &ArgStack,
    dp: &dyn DataProvider,
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
        match GlobalsRepo::get(dp, &key).await {
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

pub async fn dispatch(
    spec: &SubActionSpec,
    arg_stack: &ArgStack,
    index: usize,
    parent_event_id: EventId,
    bus: &EventBus,
    dp: &dyn DataProvider,
) -> (SubActionTelemetry, Option<ArgStack>) {
    match spec {
        SubActionSpec::Log { message, .. } => {
            let interpolated = interpolate_with_globals(message, arg_stack, dp).await;
            (log::run(spec, index, &interpolated), None)
        }
        SubActionSpec::SendChat { .. } => {
            let t = send_chat::run(spec, arg_stack, index, parent_event_id, bus, dp).await;
            (t, None)
        }
        SubActionSpec::Delay { .. } => (delay::run(spec, index).await, None),
        SubActionSpec::SetGlobal { .. } => {
            let t = set_global::run(spec, arg_stack, index, parent_event_id, bus, dp).await;
            (t, None)
        }
        SubActionSpec::GetGlobal { .. } => get_global::run(spec, arg_stack, index, dp).await,
        SubActionSpec::IncrementGlobal { .. } => {
            let t = increment_global::run(spec, arg_stack, index, parent_event_id, bus, dp).await;
            (t, None)
        }
        SubActionSpec::DeleteGlobal { .. } => {
            let t = delete_global::run(spec, arg_stack, index, parent_event_id, bus, dp).await;
            (t, None)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::EventBus;
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
        let bus = EventBus::new();
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
            dp.as_ref(),
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
        let bus = EventBus::new();
        let spec = SubActionSpec::Log {
            level: LogLevel::Info,
            message: "hello %user%".to_string(),
        };
        let stack = ArgStack::new().set("user".to_string(), Variant::String("alice".to_string()));
        let (telemetry, _) = dispatch(&spec, &stack, 0, EventId::new(), &bus, dp.as_ref()).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
    }

    #[tokio::test]
    async fn send_chat_publishes_chat_send_request_with_correct_caused_by() {
        let dp = make_dp().await;
        let bus = EventBus::new();
        let mut sub = bus.subscribe();
        let parent_id = EventId::new();
        let spec = SubActionSpec::SendChat {
            message: "hello %user%".to_string(),
            target: "twitch".to_string(),
        };
        let stack = ArgStack::new().set("user".to_string(), Variant::String("alice".to_string()));
        dispatch(&spec, &stack, 0, parent_id, &bus, dp.as_ref()).await;
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
        let bus = EventBus::new();
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
            dp.as_ref(),
        )
        .await;
        let val = GlobalsRepo::get(dp.as_ref(), "counter").await.unwrap();
        assert!(matches!(val, Some(Variant::Int(42))));
    }

    #[tokio::test]
    async fn set_global_interpolates_value_from_arg_stack() {
        let dp = make_dp().await;
        let bus = EventBus::new();
        let spec = SubActionSpec::SetGlobal {
            name: "greeting".to_string(),
            value: "%user%".to_string(),
        };
        let stack = ArgStack::new().set("user".to_string(), Variant::String("alice".to_string()));
        dispatch(&spec, &stack, 0, EventId::new(), &bus, dp.as_ref()).await;
        let val = GlobalsRepo::get(dp.as_ref(), "greeting").await.unwrap();
        assert!(matches!(val, Some(Variant::String(s)) if s == "alice"));
    }

    #[tokio::test]
    async fn set_global_emits_global_set_event() {
        let dp = make_dp().await;
        let bus = EventBus::new();
        let mut sub = bus.subscribe();
        let parent_id = EventId::new();
        let spec = SubActionSpec::SetGlobal {
            name: "x".to_string(),
            value: "100".to_string(),
        };
        dispatch(&spec, &ArgStack::new(), 0, parent_id, &bus, dp.as_ref()).await;
        let event = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.kind, "global.set");
        assert_eq!(event.caused_by, Some(parent_id));
        assert_eq!(event.payload["name"].as_str(), Some("x"));
    }

    #[tokio::test]
    async fn get_global_dispatch_returns_updated_stack() {
        let dp = make_dp().await;
        GlobalsRepo::set(dp.as_ref(), "counter", Variant::Int(7), false)
            .await
            .unwrap();
        let bus = EventBus::new();
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
            dp.as_ref(),
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
        let result =
            interpolate_with_globals("Hello %user%, count is %counter%", &stack, dp.as_ref()).await;
        assert_eq!(result, "Hello alice, count is 7");
    }

    #[tokio::test]
    async fn interpolate_with_globals_unresolved_remains_verbatim() {
        let dp = make_dp().await;
        let stack = ArgStack::new();
        let result = interpolate_with_globals("%ghost%", &stack, dp.as_ref()).await;
        assert_eq!(result, "%ghost%");
    }

    #[tokio::test]
    async fn interpolate_with_globals_arg_stack_takes_priority() {
        let dp = make_dp().await;
        GlobalsRepo::set(dp.as_ref(), "x", Variant::Int(99), false)
            .await
            .unwrap();
        let stack = ArgStack::new().set("x".to_string(), Variant::String("from_stack".to_string()));
        let result = interpolate_with_globals("%x%", &stack, dp.as_ref()).await;
        assert_eq!(result, "from_stack");
    }
}
