use forge_events::{Event, EventSource};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, EventId, SubActionOutcome, SubActionSpec, SubActionTelemetry};
use time::OffsetDateTime;

use crate::EventBus;

pub(super) async fn run(
    spec: &SubActionSpec,
    arg_stack: &ArgStack,
    index: usize,
    parent_event_id: EventId,
    bus: &EventBus,
    globals: &dyn GlobalsRepo,
) -> SubActionTelemetry {
    let started_at = OffsetDateTime::now_utc();

    let SubActionSpec::SendChat { message, target } = spec else {
        unreachable!()
    };

    let message = super::interpolate_with_globals(message, arg_stack, globals).await;
    let target = super::interpolate_with_globals(target, arg_stack, globals).await;

    bus.publish(Event::caused_by(
        EventSource::Core,
        "chat.send.request",
        serde_json::json!({
            "target": target,
            "message": message,
        }),
        parent_event_id,
    ));

    let finished_at = OffsetDateTime::now_utc();
    let duration_ms = (finished_at - started_at).whole_milliseconds().max(0) as u64;

    SubActionTelemetry {
        index,
        kind: "SendChat".to_string(),
        started_at,
        duration_ms,
        outcome: SubActionOutcome::Success,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{EventBus, NullEventLogRepo};
    use forge_storage::{DataProvider, GlobalsRepo};
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{ArgStack, EventId, SubActionSpec, Variant};
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
    async fn send_chat_publishes_event_with_correct_kind_and_caused_by() {
        let dp = make_dp().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let parent_id = EventId::new();
        let spec = SubActionSpec::SendChat {
            message: "hello".to_string(),
            target: "twitch".to_string(),
        };
        run(
            &spec,
            &ArgStack::new(),
            0,
            parent_id,
            &bus,
            dp.as_ref() as &dyn GlobalsRepo,
        )
        .await;
        let event = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.kind, "chat.send.request");
        assert_eq!(event.caused_by, Some(parent_id));
    }

    #[tokio::test]
    async fn send_chat_interpolates_message_and_target_in_payload() {
        let dp = make_dp().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let stack = ArgStack::new().set("user".to_string(), Variant::String("alice".to_string()));
        let spec = SubActionSpec::SendChat {
            message: "hi %user%".to_string(),
            target: "%platform%".to_string(),
        };
        let stack = stack.set(
            "platform".to_string(),
            Variant::String("twitch".to_string()),
        );
        run(
            &spec,
            &stack,
            0,
            EventId::new(),
            &bus,
            dp.as_ref() as &dyn GlobalsRepo,
        )
        .await;
        let event = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.payload["message"].as_str(), Some("hi alice"));
        assert_eq!(event.payload["target"].as_str(), Some("twitch"));
    }

    #[tokio::test]
    async fn send_chat_returns_success_telemetry() {
        let dp = make_dp().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let _ = bus.subscribe();
        let spec = SubActionSpec::SendChat {
            message: "test".to_string(),
            target: "twitch".to_string(),
        };
        let telemetry = run(
            &spec,
            &ArgStack::new(),
            2,
            EventId::new(),
            &bus,
            dp.as_ref() as &dyn GlobalsRepo,
        )
        .await;
        assert_eq!(telemetry.kind, "SendChat");
        assert_eq!(telemetry.index, 2);
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
    }
}
