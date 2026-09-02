use std::collections::HashMap;
use std::time::Instant;

use forge_events::{Event, EventSource};
use forge_types::EventId;

use crate::backend::HotkeyId;
use crate::client::HotkeyClient;
use crate::payload_fields;

pub(crate) struct Hold {
    press_event_id: EventId,
    combo: String,
    started_at: Instant,
}

pub(crate) type HoldMap = HashMap<HotkeyId, Hold>;

/// Publishes the press and opens the hold, unless the registration is already held; the caller
/// suppresses everything downstream (health included) when this reports `false`.
pub(crate) fn open(client: &HotkeyClient, id: HotkeyId, combo: String, timestamp_us: u64) -> bool {
    // The guard spans the publish so a concurrent close cannot drain the map between the press
    // reaching the bus and the hold existing, which would orphan the press with no release.
    let mut holds = client.holds.lock().unwrap_or_else(|p| p.into_inner());
    if holds.contains_key(&id) {
        return false;
    }

    let event = Event::new(
        EventSource::Hotkey,
        "hotkey.global.pressed",
        serde_json::json!({
            (payload_fields::COMBO): combo,
            (payload_fields::ID): id.0,
            (payload_fields::TIMESTAMP_US): timestamp_us,
        }),
    );
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
        let mut holds = client.holds.lock().unwrap_or_else(|p| p.into_inner());
        holds.remove(&id)
    };
    if let Some(hold) = held {
        publish_release(client, id, hold, timestamp_us, false);
    }
}

pub(crate) fn close_synthesized(client: &HotkeyClient, id: HotkeyId) {
    let held = {
        let mut holds = client.holds.lock().unwrap_or_else(|p| p.into_inner());
        holds.remove(&id)
    };
    if let Some(hold) = held {
        publish_release(client, id, hold, now_us(), true);
    }
}

pub(crate) fn close_all_synthesized(client: &HotkeyClient) {
    let held: Vec<(HotkeyId, Hold)> = {
        let mut holds = client.holds.lock().unwrap_or_else(|p| p.into_inner());
        holds.drain().collect()
    };
    let timestamp_us = now_us();
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
        let mut holds = client.holds.lock().unwrap_or_else(|p| p.into_inner());
        let ids: Vec<HotkeyId> = holds
            .iter()
            .filter(|(_, hold)| now.saturating_duration_since(hold.started_at) >= ceiling)
            .map(|(&id, _)| id)
            .collect();
        ids.into_iter()
            .filter_map(|id| holds.remove(&id).map(|hold| (id, hold)))
            .collect()
    };

    let timestamp_us = now_us();
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
    let hold_ms = Instant::now()
        .saturating_duration_since(hold.started_at)
        .as_millis() as u64;

    client.publisher.publish(Event::caused_by(
        EventSource::Hotkey,
        "hotkey.global.released",
        serde_json::json!({
            (payload_fields::COMBO): hold.combo,
            (payload_fields::ID): id.0,
            (payload_fields::TIMESTAMP_US): timestamp_us,
            (payload_fields::HOLD_MS): hold_ms,
            (payload_fields::SYNTHESIZED): synthesized,
        }),
        hold.press_event_id,
    ));

    if synthesized {
        let mut snap = client
            .health_state
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        snap.record_synthesized_release();
    }
}

fn now_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use forge_events::EventPublisher;

    use super::*;
    use crate::backend::tests::MockPortalBackend;
    use crate::client::tests::{RecordingPublisher, start_supervised};

    fn backdate(client: &HotkeyClient, id: HotkeyId, secs: u64) {
        if secs == 0 {
            return;
        }
        let mut holds = client.holds.lock().unwrap();
        let hold = holds.get_mut(&id).unwrap();
        hold.started_at = hold
            .started_at
            .checked_sub(Duration::from_secs(secs))
            .expect("monotonic clock too young to backdate this hold");
    }

    #[tokio::test]
    async fn a_hold_is_closed_by_the_ceiling_only_once_it_has_reached_it() {
        for (age_secs, ceiling_secs, should_close) in [
            (0, Some(1), false),
            (1, Some(1), true),
            (3, Some(1), true),
            (3, None, false),
        ] {
            let publisher = RecordingPublisher::new();
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
                publisher.has_kind("hotkey.global.released"),
                should_close,
                "wrong verdict for {case}"
            );
            if should_close {
                let ev = publisher.find_kind("hotkey.global.released").unwrap();
                assert_eq!(ev.payload["synthesized"], true, "flag for {case}");
                assert!(
                    ev.payload["hold_ms"].as_u64().unwrap() >= age_secs * 1000,
                    "hold_ms understates {case}"
                );
            }
        }
    }
}
