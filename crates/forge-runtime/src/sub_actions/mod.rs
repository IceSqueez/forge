mod delay;
mod log;
mod send_chat;
mod set_global;

use forge_storage::DataProvider;
use forge_types::{ArgStack, EventId, SubActionSpec, SubActionTelemetry};

use crate::EventBus;

pub async fn dispatch(
    spec: &SubActionSpec,
    arg_stack: &ArgStack,
    index: usize,
    parent_event_id: EventId,
    bus: &EventBus,
    dp: &dyn DataProvider,
) -> SubActionTelemetry {
    match spec {
        SubActionSpec::Log { message, .. } => {
            let interpolated = arg_stack.interpolate(message);
            log::run(spec, index, &interpolated)
        }
        SubActionSpec::SendChat { .. } => {
            send_chat::run(spec, arg_stack, index, parent_event_id, bus).await
        }
        SubActionSpec::Delay { .. } => delay::run(spec, index).await,
        SubActionSpec::SetGlobal { .. } => set_global::run(spec, arg_stack, index, dp).await,
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
        let telemetry = dispatch(
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
        let telemetry = dispatch(&spec, &stack, 0, EventId::new(), &bus, dp.as_ref()).await;
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
}
