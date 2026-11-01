use forge_types::{EventId, RecoverMutex};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use std::time::Instant;

use forge_events::{Event, EventSource, TypedEvent};

use crate::backend::{HotkeyId, wall_clock_us};
use crate::client::HotkeyClient;
use crate::vocabulary::fields;
use crate::vocabulary::{HotkeyPressed, event_kinds};

pub(crate) struct Hold {
    press_event_id: EventId,
    combo: String,
    started_at: Instant,
}

pub(crate) type HoldMap = HashMap<HotkeyId, Hold>;

pub(crate) struct HoldLedger {
    pub(crate) open: Mutex<HoldMap>,
    pub(crate) unreleased: Mutex<HashSet<HotkeyId>>,
    pub(crate) ceiling_secs: AtomicU64,
}

impl HoldLedger {
    pub(crate) fn new(ceiling_secs: Option<u64>) -> Self {
        Self {
            open: Mutex::new(HoldMap::new()),
            unreleased: Mutex::new(HashSet::new()),
            ceiling_secs: AtomicU64::new(ceiling_secs.unwrap_or(0)),
        }
    }
}

pub(crate) fn open(client: &HotkeyClient, id: HotkeyId, combo: String, timestamp_us: u64) -> bool {
    let mut holds = client.hold.open.lock_recover();
    if holds.contains_key(&id) {
        return false;
    }

    let event = HotkeyPressed {
        combo: combo.clone(),
        id: id.0,
        timestamp_us,
    }
    .into_event(EventSource::Hotkey, None);
    let press_event_id = event.id;
    client.publisher.publish(event);

    holds.insert(
        id,
        Hold {
            press_event_id,
            combo,
            started_at: Instant::now(),
        },
    );
    true
}

pub(crate) fn close_observed(client: &HotkeyClient, id: HotkeyId, timestamp_us: u64) {
    let held = {
        let mut holds = client.hold.open.lock_recover();
        holds.remove(&id)
    };
    if let Some(hold) = held {
        publish_release(client, id, hold, timestamp_us, false);
    }
}

pub(crate) fn close_synthesized(client: &HotkeyClient, id: HotkeyId) {
    let held = {
        let mut holds = client.hold.open.lock_recover();
        holds.remove(&id)
    };
    if let Some(hold) = held {
        publish_release(client, id, hold, wall_clock_us(), true);
    }
}

pub(crate) fn close_all_synthesized(client: &HotkeyClient) {
    let held: Vec<(HotkeyId, Hold)> = {
        let mut holds = client.hold.open.lock_recover();
        holds.drain().collect()
    };
    let timestamp_us = wall_clock_us();
    for (id, hold) in held {
        publish_release(client, id, hold, timestamp_us, true);
    }
}

pub(crate) fn close_expired(client: &HotkeyClient) {
    let Some(ceiling) = client.hold_ceiling() else {
        return;
    };

    let now = Instant::now();
    let expired: Vec<(HotkeyId, Hold)> = {
        let mut holds = client.hold.open.lock_recover();
        let ids: Vec<HotkeyId> = holds
            .iter()
            .filter(|(_, hold)| now.saturating_duration_since(hold.started_at) >= ceiling)
            .map(|(&id, _)| id)
            .collect();
        ids.into_iter()
            .filter_map(|id| holds.remove(&id).map(|hold| (id, hold)))
            .collect()
    };

    let timestamp_us = wall_clock_us();
    for (id, hold) in expired {
        publish_release(client, id, hold, timestamp_us, true);
    }
}

fn publish_release(
    client: &HotkeyClient,
    id: HotkeyId,
    hold: Hold,
    timestamp_us: u64,
    synthesized: bool,
) {
    let hold_ms = u64::try_from(
        Instant::now()
            .saturating_duration_since(hold.started_at)
            .as_millis(),
    )
    .unwrap_or(u64::MAX);

    client.publisher.publish(Event::child_of(
        EventSource::Hotkey,
        event_kinds::GLOBAL_RELEASED,
        serde_json::json!({
            (fields::COMBO): hold.combo,
            (fields::ID): id.0,
            (fields::TIMESTAMP_US): timestamp_us,
            (fields::HOLD_MS): hold_ms,
            (fields::SYNTHESIZED): synthesized,
        }),
        hold.press_event_id,
    ));

    if synthesized {
        let mut snap = client.health.state.lock_recover();
        snap.record_synthesized_release();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use forge_events::EventPublisher;

    use super::*;
    use crate::backend::tests::MockPortalBackend;
    use crate::client::tests::{CapturingPublisher, start_supervised};

    fn backdate(client: &HotkeyClient, id: HotkeyId, secs: u64) {
        if secs == 0 {
            return;
        }
        let mut holds = client.hold.open.lock().unwrap();
        let hold = holds.get_mut(&id).unwrap();
        hold.started_at = hold
            .started_at
            .checked_sub(Duration::from_secs(secs))
            .expect("monotonic clock too young to backdate this hold");
    }

    #[tokio::test]
    async fn a_hold_is_closed_by_the_ceiling_only_once_it_has_reached_it() {
        for (age_secs, ceiling_secs, should_close) in [
            (0, Some(3_600), false),
            (1, Some(1), true),
            (3, Some(1), true),
            (3, None, false),
        ] {
            let publisher = CapturingPublisher::shared();
            let (backend, _tx) = MockPortalBackend::new();
            let client =
                start_supervised(backend, Arc::clone(&publisher) as Arc<dyn EventPublisher>);
            client.set_hold_ceiling(ceiling_secs);

            let id = HotkeyId(1);
            assert!(open(&client, id, "Ctrl+F1".to_owned(), 0));
            backdate(&client, id, age_secs);

            close_expired(&client);

            let case = format!("{age_secs}s held against a {ceiling_secs:?}s ceiling");
            assert_eq!(
                publisher.has_kind(event_kinds::GLOBAL_RELEASED),
                should_close,
                "wrong verdict for {case}"
            );
            if should_close {
                let ev = publisher.find_kind(event_kinds::GLOBAL_RELEASED).unwrap();
                assert_eq!(ev.payload["synthesized"], true, "flag for {case}");
                assert!(
                    ev.payload["hold_ms"].as_u64().unwrap() >= age_secs * 1000,
                    "hold_ms understates {case}"
                );
            }
        }
    }
}
