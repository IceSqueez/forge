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

    let SubActionSpec::IncrementGlobal { name, amount } = spec else {
        unreachable!()
    };

    let resolved_name = super::interpolate_with_globals(name, arg_stack, dp).await;

    let outcome = match GlobalsRepo::incr(dp, &resolved_name, *amount).await {
        Ok(new_value) => {
            bus.publish(Event::caused_by(
                EventSource::Core,
                "global.incr",
                serde_json::json!({
                    "name": resolved_name,
                    "amount": amount,
                    "new_value": new_value.to_string(),
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
        kind: "IncrementGlobal".to_string(),
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
    async fn increment_global_updates_value_and_emits_event() {
        let dp = make_dp().await;
        GlobalsRepo::set(dp.as_ref(), "counter", Variant::Int(7), false)
            .await
            .unwrap();

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let parent_id = EventId::new();
        let spec = SubActionSpec::IncrementGlobal {
            name: "counter".to_string(),
            amount: 3,
        };

        let telemetry = run(&spec, &ArgStack::new(), 0, parent_id, &bus, dp.as_ref()).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));

        let stored = GlobalsRepo::get(dp.as_ref(), "counter").await.unwrap();
        assert_eq!(stored, Some(Variant::Int(10)));

        let event = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.kind, "global.incr");
        assert_eq!(event.caused_by, Some(parent_id));
        assert_eq!(event.payload["name"].as_str(), Some("counter"));
        assert_eq!(event.payload["amount"].as_i64(), Some(3));
    }

    #[tokio::test]
    async fn increment_global_nonexistent_returns_failed() {
        let dp = make_dp().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let spec = SubActionSpec::IncrementGlobal {
            name: "missing".to_string(),
            amount: 1,
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
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
    }

    #[tokio::test]
    async fn increment_global_returns_correct_kind_and_index() {
        let dp = make_dp().await;
        GlobalsRepo::set(dp.as_ref(), "c", Variant::Int(0), false)
            .await
            .unwrap();
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let spec = SubActionSpec::IncrementGlobal {
            name: "c".to_string(),
            amount: 1,
        };
        let telemetry = run(
            &spec,
            &ArgStack::new(),
            5,
            EventId::new(),
            &bus,
            dp.as_ref(),
        )
        .await;
        assert_eq!(telemetry.kind, "IncrementGlobal");
        assert_eq!(telemetry.index, 5);
    }
}
