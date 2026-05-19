use forge_events::{Event, EventSource};
use forge_storage::{DataProvider, GlobalsRepo};
use forge_types::{ArgStack, EventId, SubActionOutcome, SubActionSpec, SubActionTelemetry};
use time::OffsetDateTime;

use crate::EventBus;

pub(super) async fn run(
    spec: &SubActionSpec,
    arg_stack: &ArgStack,
    index: usize,
    parent_event_id: EventId,
    bus: &EventBus,
    dp: &dyn DataProvider,
) -> SubActionTelemetry {
    let started_at = OffsetDateTime::now_utc();

    let SubActionSpec::DeleteGlobal { name } = spec else {
        unreachable!()
    };

    let resolved_name = super::interpolate_with_globals(name, arg_stack, dp).await;

    let outcome = match GlobalsRepo::delete(dp, &resolved_name).await {
        Ok(existed) => {
            bus.publish(Event::caused_by(
                EventSource::Core,
                "global.del",
                serde_json::json!({
                    "name": resolved_name,
                    "existed": existed,
                }),
                parent_event_id,
            ));
            SubActionOutcome::Success
        }
        Err(e) => SubActionOutcome::Failed(e.to_string()),
    };

    let finished_at = OffsetDateTime::now_utc();
    let duration_ms = (finished_at - started_at).whole_milliseconds().max(0) as u64;

    SubActionTelemetry {
        index,
        kind: "DeleteGlobal".to_string(),
        started_at,
        duration_ms,
        outcome,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{EventBus, NullEventLogRepo};
    use forge_storage::GlobalsRepo;
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
    async fn delete_global_existing_key_emits_existed_true() {
        let dp = make_dp().await;
        GlobalsRepo::set(dp.as_ref(), "counter", Variant::Int(7), false)
            .await
            .unwrap();

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let parent_id = EventId::new();
        let spec = SubActionSpec::DeleteGlobal {
            name: "counter".to_string(),
        };

        let telemetry = run(&spec, &ArgStack::new(), 0, parent_id, &bus, dp.as_ref()).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));

        let stored = GlobalsRepo::get(dp.as_ref(), "counter").await.unwrap();
        assert!(stored.is_none());

        let event = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.kind, "global.del");
        assert_eq!(event.caused_by, Some(parent_id));
        assert_eq!(event.payload["existed"].as_bool(), Some(true));
    }

    #[tokio::test]
    async fn delete_global_nonexistent_key_emits_existed_false() {
        let dp = make_dp().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let spec = SubActionSpec::DeleteGlobal {
            name: "missing".to_string(),
        };

        let telemetry = run(
            &spec,
            &ArgStack::new(),
            0,
            EventId::new(),
            &bus,
            dp.as_ref(),
        )
        .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));

        let event = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.payload["existed"].as_bool(), Some(false));
    }

    #[tokio::test]
    async fn delete_global_returns_correct_kind_and_index() {
        let dp = make_dp().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let spec = SubActionSpec::DeleteGlobal {
            name: "x".to_string(),
        };
        let telemetry = run(
            &spec,
            &ArgStack::new(),
            2,
            EventId::new(),
            &bus,
            dp.as_ref(),
        )
        .await;
        assert_eq!(telemetry.kind, "DeleteGlobal");
        assert_eq!(telemetry.index, 2);
    }
}
