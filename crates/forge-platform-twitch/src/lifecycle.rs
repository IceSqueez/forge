use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use forge_platform_core::QuickActionLiveness;

use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

/// Twitch's fixed raid countdown - the upper bound on how long a locally recorded pending
/// raid can still be real.
const RAID_COUNTDOWN: Duration = Duration::from_secs(90);

const POLLS_PATH: &str = "/helix/polls";
const PREDICTIONS_PATH: &str = "/helix/predictions";
const STATUS_ACTIVE: &str = "ACTIVE";
const STATUS_LOCKED: &str = "LOCKED";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum PollPhase {
    #[default]
    Unknown,
    Absent,
    Active,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum PredictionPhase {
    #[default]
    Unknown,
    Absent,
    Active,
    Locked,
}

#[derive(Debug, Clone, Copy, Default)]
struct ChannelEntities {
    poll: PollPhase,
    prediction: PredictionPhase,
    raid_until: Option<Instant>,
}

/// Shared by the EventSub session that writes phases, the raid runners that record a local
/// pending raid, and the bundle that reads both while building quick-action descriptors.
#[derive(Clone, Default)]
pub struct TwitchLifecycle {
    entities: Arc<RwLock<ChannelEntities>>,
}

impl TwitchLifecycle {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn snapshot(&self) -> LifecycleSnapshot {
        let entities = *self.entities.read().unwrap_or_else(|p| p.into_inner());
        LifecycleSnapshot {
            poll: entities.poll,
            prediction: entities.prediction,
            raid_pending: entities
                .raid_until
                .is_some_and(|until| Instant::now() < until),
        }
    }

    pub(crate) fn apply_notification(
        &self,
        subscription_type: &str,
        event: &serde_json::Value,
        self_broadcaster_id: &str,
    ) {
        let mut entities = self.entities.write().unwrap_or_else(|p| p.into_inner());
        match subscription_type {
            "channel.poll.begin" | "channel.poll.progress" => entities.poll = PollPhase::Active,
            "channel.poll.end" => entities.poll = PollPhase::Absent,
            "channel.prediction.begin" => entities.prediction = PredictionPhase::Active,
            "channel.prediction.progress" => {
                if entities.prediction != PredictionPhase::Locked {
                    entities.prediction = PredictionPhase::Active;
                }
            }
            "channel.prediction.lock" => entities.prediction = PredictionPhase::Locked,
            "channel.prediction.end" => entities.prediction = PredictionPhase::Absent,
            "channel.raid"
                if !self_broadcaster_id.is_empty()
                    && raid_is_outgoing(event, self_broadcaster_id) =>
            {
                entities.raid_until = None;
            }
            _ => {}
        }
    }

    pub(crate) fn raid_started(&self) {
        let mut entities = self.entities.write().unwrap_or_else(|p| p.into_inner());
        entities.raid_until = Some(Instant::now() + RAID_COUNTDOWN);
    }

    pub(crate) fn raid_canceled(&self) {
        let mut entities = self.entities.write().unwrap_or_else(|p| p.into_inner());
        entities.raid_until = None;
    }

    /// The pending raid survives: it is a local countdown rather than a phase the notification
    /// stream could have corrected while it was down.
    pub(crate) fn forget_phases(&self) {
        let mut entities = self.entities.write().unwrap_or_else(|p| p.into_inner());
        entities.poll = PollPhase::Unknown;
        entities.prediction = PredictionPhase::Unknown;
    }

    /// A failed or unreadable response leaves the phase untouched, so a seeding outage stays
    /// fail-open instead of claiming the entity is gone.
    pub(crate) async fn seed_from_helix(
        &self,
        transport: &dyn HelixTransport,
        broadcaster_id: &str,
    ) {
        let polls = HelixRequest::new(HelixMethod::Get, POLLS_PATH)
            .query("broadcaster_id", broadcaster_id.to_owned());
        if let Ok(body) = transport.execute(polls).await
            && let Some(rows) = entity_rows(&body)
        {
            let phase = if rows_have_status(rows, STATUS_ACTIVE) {
                PollPhase::Active
            } else {
                PollPhase::Absent
            };
            let mut entities = self.entities.write().unwrap_or_else(|p| p.into_inner());
            entities.poll = phase;
        }

        let predictions = HelixRequest::new(HelixMethod::Get, PREDICTIONS_PATH)
            .query("broadcaster_id", broadcaster_id.to_owned());
        if let Ok(body) = transport.execute(predictions).await
            && let Some(rows) = entity_rows(&body)
        {
            let phase = if rows_have_status(rows, STATUS_ACTIVE) {
                PredictionPhase::Active
            } else if rows_have_status(rows, STATUS_LOCKED) {
                PredictionPhase::Locked
            } else {
                PredictionPhase::Absent
            };
            let mut entities = self.entities.write().unwrap_or_else(|p| p.into_inner());
            entities.prediction = phase;
        }
    }
}

fn raid_is_outgoing(event: &serde_json::Value, self_broadcaster_id: &str) -> bool {
    event
        .get("from_broadcaster_user_id")
        .and_then(|v| v.as_str())
        .is_some_and(|from| from == self_broadcaster_id)
}

fn entity_rows(body: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    body.get("data").and_then(|d| d.as_array())
}

fn rows_have_status(rows: &[serde_json::Value], wanted: &str) -> bool {
    rows.iter()
        .any(|row| row.get("status").and_then(|s| s.as_str()) == Some(wanted))
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LifecycleSnapshot {
    poll: PollPhase,
    prediction: PredictionPhase,
    raid_pending: bool,
}

impl LifecycleSnapshot {
    pub(crate) fn poll_in_flight(self) -> QuickActionLiveness {
        match self.poll {
            PollPhase::Active => QuickActionLiveness::Live,
            PollPhase::Absent => QuickActionLiveness::Absent,
            PollPhase::Unknown => QuickActionLiveness::Unknown,
        }
    }

    pub(crate) fn poll_slot_free(self) -> QuickActionLiveness {
        match self.poll {
            PollPhase::Active => QuickActionLiveness::Absent,
            PollPhase::Absent | PollPhase::Unknown => QuickActionLiveness::Unknown,
        }
    }

    pub(crate) fn prediction_lockable(self) -> QuickActionLiveness {
        match self.prediction {
            PredictionPhase::Active => QuickActionLiveness::Live,
            PredictionPhase::Absent | PredictionPhase::Locked => QuickActionLiveness::Absent,
            PredictionPhase::Unknown => QuickActionLiveness::Unknown,
        }
    }

    pub(crate) fn prediction_settleable(self) -> QuickActionLiveness {
        match self.prediction {
            PredictionPhase::Active | PredictionPhase::Locked => QuickActionLiveness::Live,
            PredictionPhase::Absent => QuickActionLiveness::Absent,
            PredictionPhase::Unknown => QuickActionLiveness::Unknown,
        }
    }

    pub(crate) fn prediction_slot_free(self) -> QuickActionLiveness {
        match self.prediction {
            PredictionPhase::Active | PredictionPhase::Locked => QuickActionLiveness::Absent,
            PredictionPhase::Absent | PredictionPhase::Unknown => QuickActionLiveness::Unknown,
        }
    }

    /// Never `Absent`: a raid can be started outside forge, so the absence of a local pending
    /// raid is not knowledge that none is running.
    pub(crate) fn raid_in_flight(self) -> QuickActionLiveness {
        if self.raid_pending {
            QuickActionLiveness::Live
        } else {
            QuickActionLiveness::Unknown
        }
    }

    pub(crate) fn raid_slot_free(self) -> QuickActionLiveness {
        if self.raid_pending {
            QuickActionLiveness::Absent
        } else {
            QuickActionLiveness::Unknown
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::MockTransport;
    use serde_json::{Value, json};

    const SELF_ID: &str = "1234";

    fn rows(statuses: &[&str]) -> Value {
        json!({
            "data": statuses
                .iter()
                .map(|s| json!({ "id": "abc", "status": s }))
                .collect::<Vec<_>>(),
        })
    }

    #[test]
    fn lifecycle_topics_move_the_poll_and_prediction_phases() {
        let cases = [
            (
                "channel.poll.begin",
                PollPhase::Active,
                PredictionPhase::Unknown,
            ),
            (
                "channel.poll.progress",
                PollPhase::Active,
                PredictionPhase::Unknown,
            ),
            (
                "channel.poll.end",
                PollPhase::Absent,
                PredictionPhase::Unknown,
            ),
            (
                "channel.prediction.begin",
                PollPhase::Unknown,
                PredictionPhase::Active,
            ),
            (
                "channel.prediction.progress",
                PollPhase::Unknown,
                PredictionPhase::Active,
            ),
            (
                "channel.prediction.lock",
                PollPhase::Unknown,
                PredictionPhase::Locked,
            ),
            (
                "channel.prediction.end",
                PollPhase::Unknown,
                PredictionPhase::Absent,
            ),
            (
                "channel.chat.message",
                PollPhase::Unknown,
                PredictionPhase::Unknown,
            ),
            (
                "channel.shield_mode.begin",
                PollPhase::Unknown,
                PredictionPhase::Unknown,
            ),
        ];
        for (topic, poll, prediction) in cases {
            let lifecycle = TwitchLifecycle::new();
            lifecycle.apply_notification(topic, &Value::Null, SELF_ID);
            let snapshot = lifecycle.snapshot();
            assert_eq!(snapshot.poll, poll, "poll phase after {topic}");
            assert_eq!(
                snapshot.prediction, prediction,
                "prediction phase after {topic}"
            );
        }
    }

    #[test]
    fn prediction_progress_does_not_demote_a_locked_prediction() {
        let lifecycle = TwitchLifecycle::new();
        lifecycle.apply_notification("channel.prediction.lock", &Value::Null, SELF_ID);

        lifecycle.apply_notification("channel.prediction.progress", &Value::Null, SELF_ID);

        assert_eq!(lifecycle.snapshot().prediction, PredictionPhase::Locked);
    }

    #[test]
    fn a_started_raid_stays_pending_until_it_is_canceled() {
        let lifecycle = TwitchLifecycle::new();
        assert!(!lifecycle.snapshot().raid_pending);

        lifecycle.raid_started();
        assert!(lifecycle.snapshot().raid_pending);

        lifecycle.raid_canceled();
        assert!(!lifecycle.snapshot().raid_pending);
    }

    #[test]
    fn our_own_outgoing_raid_notification_clears_the_pending_raid() {
        let lifecycle = TwitchLifecycle::new();
        lifecycle.raid_started();

        lifecycle.apply_notification(
            "channel.raid",
            &json!({ "from_broadcaster_user_id": SELF_ID }),
            SELF_ID,
        );

        assert!(!lifecycle.snapshot().raid_pending);
    }

    #[test]
    fn a_raid_notification_that_is_not_ours_leaves_the_pending_raid() {
        let cases = [
            (
                "incoming raid from another channel",
                json!({ "from_broadcaster_user_id": "9999" }),
                SELF_ID,
            ),
            (
                "payload without a from-broadcaster",
                json!({ "to_broadcaster_user_id": SELF_ID }),
                SELF_ID,
            ),
            (
                "unresolved self id must not match a blank sender",
                json!({ "from_broadcaster_user_id": "" }),
                "",
            ),
        ];
        for (case, event, self_id) in cases {
            let lifecycle = TwitchLifecycle::new();
            lifecycle.raid_started();

            lifecycle.apply_notification("channel.raid", &event, self_id);

            assert!(lifecycle.snapshot().raid_pending, "cleared for {case}");
        }
    }

    #[test]
    fn forgetting_phases_keeps_the_locally_recorded_pending_raid() {
        let lifecycle = TwitchLifecycle::new();
        lifecycle.apply_notification("channel.poll.begin", &Value::Null, SELF_ID);
        lifecycle.apply_notification("channel.prediction.lock", &Value::Null, SELF_ID);
        lifecycle.raid_started();

        lifecycle.forget_phases();

        let snapshot = lifecycle.snapshot();
        assert_eq!(snapshot.poll, PollPhase::Unknown);
        assert_eq!(snapshot.prediction, PredictionPhase::Unknown);
        assert!(
            snapshot.raid_pending,
            "a local countdown is not something the notification stream could have corrected"
        );
    }

    #[tokio::test]
    async fn seeding_queries_polls_and_predictions_scoped_to_the_broadcaster() {
        let transport = MockTransport::returning_sequence(vec![Ok(rows(&[])), Ok(rows(&[]))]);
        let lifecycle = TwitchLifecycle::new();

        lifecycle.seed_from_helix(&transport, "4242").await;

        assert_eq!(transport.call_count(), 2);
        for (index, path) in [(0, "/helix/polls"), (1, "/helix/predictions")] {
            let request = transport.request(index);
            assert_eq!(request.method, HelixMethod::Get);
            assert_eq!(request.path, path);
            assert_eq!(
                request.query,
                vec![("broadcaster_id".to_owned(), "4242".to_owned())],
                "{path} must be scoped to our own broadcaster"
            );
        }
    }

    #[tokio::test]
    async fn seeding_maps_helix_statuses_to_phases() {
        let cases = [
            (
                rows(&["ACTIVE"]),
                rows(&["ACTIVE"]),
                PollPhase::Active,
                PredictionPhase::Active,
            ),
            (
                rows(&[]),
                rows(&[]),
                PollPhase::Absent,
                PredictionPhase::Absent,
            ),
            (
                rows(&["COMPLETED"]),
                rows(&["RESOLVED", "LOCKED"]),
                PollPhase::Absent,
                PredictionPhase::Locked,
            ),
            (
                rows(&["TERMINATED", "ACTIVE"]),
                rows(&["LOCKED", "ACTIVE"]),
                PollPhase::Active,
                PredictionPhase::Active,
            ),
        ];
        for (polls, predictions, poll_phase, prediction_phase) in cases {
            let transport =
                MockTransport::returning_sequence(vec![Ok(polls.clone()), Ok(predictions.clone())]);
            let lifecycle = TwitchLifecycle::new();

            lifecycle.seed_from_helix(&transport, SELF_ID).await;

            let snapshot = lifecycle.snapshot();
            assert_eq!(snapshot.poll, poll_phase, "polls {polls}");
            assert_eq!(
                snapshot.prediction, prediction_phase,
                "predictions {predictions}"
            );
        }
    }

    #[tokio::test]
    async fn a_failed_seed_leaves_the_known_phases_untouched() {
        let lifecycle = TwitchLifecycle::new();
        lifecycle.apply_notification("channel.poll.begin", &Value::Null, SELF_ID);
        lifecycle.apply_notification("channel.prediction.lock", &Value::Null, SELF_ID);
        let transport = MockTransport::returning_sequence(vec![
            Err(HelixError::RateLimited),
            Err(HelixError::Http {
                status: 401,
                body: "unauthorized".to_owned(),
            }),
        ]);

        lifecycle.seed_from_helix(&transport, SELF_ID).await;

        let snapshot = lifecycle.snapshot();
        assert_eq!(snapshot.poll, PollPhase::Active);
        assert_eq!(snapshot.prediction, PredictionPhase::Locked);
    }

    #[tokio::test]
    async fn a_seed_body_without_a_readable_row_list_leaves_the_known_phases_untouched() {
        let unreadable = [
            json!({}),
            json!({ "data": { "id": "abc", "status": "ACTIVE" } }),
            json!({ "error": "Unauthorized", "status": 401 }),
            Value::Null,
        ];
        for body in unreadable {
            let lifecycle = TwitchLifecycle::new();
            lifecycle.apply_notification("channel.poll.begin", &Value::Null, SELF_ID);
            lifecycle.apply_notification("channel.prediction.lock", &Value::Null, SELF_ID);
            let transport =
                MockTransport::returning_sequence(vec![Ok(body.clone()), Ok(body.clone())]);

            lifecycle.seed_from_helix(&transport, SELF_ID).await;

            let snapshot = lifecycle.snapshot();
            assert_eq!(snapshot.poll, PollPhase::Active, "poll for {body}");
            assert_eq!(
                snapshot.prediction,
                PredictionPhase::Locked,
                "prediction for {body}"
            );
        }
    }

    #[tokio::test]
    async fn a_readable_but_empty_row_list_still_retires_a_known_phase() {
        let lifecycle = TwitchLifecycle::new();
        lifecycle.apply_notification("channel.poll.begin", &Value::Null, SELF_ID);
        lifecycle.apply_notification("channel.prediction.lock", &Value::Null, SELF_ID);
        let transport = MockTransport::returning_sequence(vec![Ok(rows(&[])), Ok(rows(&[]))]);

        lifecycle.seed_from_helix(&transport, SELF_ID).await;

        let snapshot = lifecycle.snapshot();
        assert_eq!(snapshot.poll, PollPhase::Absent);
        assert_eq!(snapshot.prediction, PredictionPhase::Absent);
    }

    #[test]
    fn snapshot_maps_phases_to_the_liveness_of_every_gated_action() {
        use QuickActionLiveness::{Absent, Live, Unknown};

        // poll_in_flight, poll_slot_free, prediction_lockable, prediction_settleable,
        // prediction_slot_free, raid_in_flight, raid_slot_free
        let cases = [
            (
                PollPhase::Unknown,
                PredictionPhase::Unknown,
                false,
                [
                    Unknown, Unknown, Unknown, Unknown, Unknown, Unknown, Unknown,
                ],
            ),
            (
                PollPhase::Active,
                PredictionPhase::Active,
                false,
                [Live, Absent, Live, Live, Absent, Unknown, Unknown],
            ),
            (
                PollPhase::Absent,
                PredictionPhase::Locked,
                true,
                [Absent, Unknown, Absent, Live, Absent, Live, Absent],
            ),
            (
                PollPhase::Absent,
                PredictionPhase::Absent,
                false,
                [Absent, Unknown, Absent, Absent, Unknown, Unknown, Unknown],
            ),
        ];
        for (poll, prediction, raid_pending, expected) in cases {
            let snapshot = LifecycleSnapshot {
                poll,
                prediction,
                raid_pending,
            };
            let actual = [
                snapshot.poll_in_flight(),
                snapshot.poll_slot_free(),
                snapshot.prediction_lockable(),
                snapshot.prediction_settleable(),
                snapshot.prediction_slot_free(),
                snapshot.raid_in_flight(),
                snapshot.raid_slot_free(),
            ];
            assert_eq!(
                actual, expected,
                "{poll:?} / {prediction:?} / raid_pending={raid_pending}"
            );
        }
    }
}
