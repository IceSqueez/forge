use forge_events::{Event, EventPublisher, EventSource};
use tracing::debug;

use crate::helix::{HelixError, HelixMethod, HelixRequest, HelixTransport};
use crate::payload_fields::goal as goal_fields;

const GOALS_PATH: &str = "/helix/goals";
const BROADCASTER_ID_PARAM: &str = "broadcaster_id";
const GOAL_PROGRESS_KIND: &str = "twitch.channel.goal.progress";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GoalProgressOrigin {
    Live,
    ConnectSync,
}

pub(crate) fn goal_progress_event(goal: &serde_json::Value, origin: GoalProgressOrigin) -> Event {
    let goal_id = goal
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned();
    let goal_type = goal
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned();
    let current_amount = goal
        .get("current_amount")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let target_amount = goal
        .get("target_amount")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let is_synced = origin == GoalProgressOrigin::ConnectSync;

    debug!(goal_id = %goal_id, current_amount, is_synced, "goal progress");

    Event::new(
        EventSource::Twitch,
        GOAL_PROGRESS_KIND,
        serde_json::json!({
            (goal_fields::GOAL): {
                (goal_fields::GOAL_ID): goal_id,
                (goal_fields::GOAL_TYPE): goal_type,
                (goal_fields::CURRENT_AMOUNT): current_amount,
                (goal_fields::TARGET_AMOUNT): target_amount,
                (goal_fields::IS_SYNCED): is_synced,
            },
        }),
    )
}

pub(crate) async fn publish_current_goals(
    transport: &dyn HelixTransport,
    bus: &dyn EventPublisher,
    broadcaster_id: &str,
) -> Result<usize, HelixError> {
    let request = HelixRequest::new(HelixMethod::Get, GOALS_PATH)
        .query(BROADCASTER_ID_PARAM, broadcaster_id.to_owned());
    let body = transport.execute(request).await?;
    let Some(goals) = body.get("data").and_then(|d| d.as_array()) else {
        return Ok(0);
    };
    for goal in goals {
        bus.publish(goal_progress_event(goal, GoalProgressOrigin::ConnectSync));
    }
    Ok(goals.len())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Mutex;

    use serde_json::{Value, json};

    use super::*;
    use crate::sub_actions::test_support::MockTransport;

    const BROADCASTER: &str = "4242";
    const UNAVAILABLE: u16 = reqwest::StatusCode::SERVICE_UNAVAILABLE.as_u16();

    #[derive(Default)]
    struct RecordingBus {
        events: Mutex<Vec<Event>>,
    }

    impl EventPublisher for RecordingBus {
        fn publish(&self, event: Event) {
            self.events.lock().unwrap().push(event);
        }
    }

    impl RecordingBus {
        fn taken(&self) -> Vec<Event> {
            std::mem::take(&mut *self.events.lock().unwrap())
        }
    }

    fn helix_row(id: &str, current: i64) -> Value {
        json!({
            "id": id,
            "broadcaster_id": BROADCASTER,
            "broadcaster_name": "Streamer",
            "broadcaster_login": "streamer",
            "type": "follower",
            "description": "Road to 1k",
            "current_amount": current,
            "target_amount": 1000,
            "created_at": "2026-10-01T12:00:00Z",
        })
    }

    #[test]
    fn goal_progress_event_maps_the_wire_fields_into_the_goal_object() {
        let event = goal_progress_event(&helix_row("goal-1", 250), GoalProgressOrigin::Live);

        assert_eq!(event.source, EventSource::Twitch);
        assert_eq!(event.kind, "twitch.channel.goal.progress");
        assert_eq!(
            event.payload,
            json!({
                "goal": {
                    "id": "goal-1",
                    "type": "follower",
                    "current_amount": 250,
                    "target_amount": 1000,
                    "is_synced": false,
                },
            })
        );
    }

    #[test]
    fn only_the_connect_sync_origin_marks_the_goal_as_synced() {
        for (origin, expected) in [
            (GoalProgressOrigin::Live, false),
            (GoalProgressOrigin::ConnectSync, true),
        ] {
            let event = goal_progress_event(&helix_row("goal-1", 1), origin);
            assert_eq!(
                event.payload["goal"]["is_synced"].as_bool(),
                Some(expected),
                "origin {origin:?}"
            );
        }
    }

    #[test]
    fn a_helix_row_and_an_eventsub_event_for_the_same_goal_publish_the_same_payload() {
        let eventsub_event = json!({
            "id": "goal-1",
            "broadcaster_user_id": BROADCASTER,
            "broadcaster_user_name": "Streamer",
            "broadcaster_user_login": "streamer",
            "type": "follower",
            "description": "Road to 1k",
            "current_amount": 250,
            "target_amount": 1000,
            "started_at": "2026-10-01T12:00:00Z",
        });
        let origin = GoalProgressOrigin::ConnectSync;

        let from_helix = goal_progress_event(&helix_row("goal-1", 250), origin);
        let from_eventsub = goal_progress_event(&eventsub_event, origin);

        assert_eq!(from_helix.payload, from_eventsub.payload);
    }

    #[test]
    fn missing_or_mistyped_goal_fields_fall_back_to_empty_text_and_zero() {
        for input in [
            json!({}),
            Value::Null,
            json!({ "id": 7, "type": null, "current_amount": "12", "target_amount": 1.5 }),
        ] {
            let event = goal_progress_event(&input, GoalProgressOrigin::Live);
            let goal = &event.payload["goal"];
            assert_eq!(goal["id"], "", "input {input}");
            assert_eq!(goal["type"], "", "input {input}");
            assert_eq!(goal["current_amount"], 0, "input {input}");
            assert_eq!(goal["target_amount"], 0, "input {input}");
        }
    }

    #[tokio::test]
    async fn current_goals_are_requested_for_the_broadcaster() {
        let transport = MockTransport::returning(Ok(json!({ "data": [] })));

        publish_current_goals(&transport, &RecordingBus::default(), BROADCASTER)
            .await
            .unwrap();

        let request = transport.last_request();
        assert_eq!(request.method, HelixMethod::Get);
        assert_eq!(request.path, "/helix/goals");
        assert_eq!(
            request.query,
            vec![("broadcaster_id".to_owned(), BROADCASTER.to_owned())]
        );
    }

    #[tokio::test]
    async fn every_active_goal_is_published_once_as_a_synced_progress_event() {
        for rows in [
            vec![],
            vec![helix_row("goal-a", 3)],
            vec![helix_row("goal-a", 3), helix_row("goal-b", 9)],
        ] {
            let transport = MockTransport::returning(Ok(json!({ "data": rows })));
            let bus = RecordingBus::default();

            let synced = publish_current_goals(&transport, &bus, BROADCASTER)
                .await
                .unwrap();

            let events = bus.taken();
            let published: Vec<(&str, bool)> = events
                .iter()
                .map(|event| {
                    (
                        event.payload["goal"]["id"].as_str().unwrap(),
                        event.payload["goal"]["is_synced"].as_bool().unwrap(),
                    )
                })
                .collect();
            let expected: Vec<(&str, bool)> = rows
                .iter()
                .map(|row| (row["id"].as_str().unwrap(), true))
                .collect();
            assert_eq!(published, expected);
            assert_eq!(synced, rows.len());
        }
    }

    #[tokio::test]
    async fn a_body_without_a_data_array_publishes_nothing() {
        for body in [
            Value::Null,
            json!({}),
            json!({ "data": null }),
            json!({ "data": { "id": "goal-a" } }),
        ] {
            let transport = MockTransport::returning(Ok(body.clone()));
            let bus = RecordingBus::default();

            let synced = publish_current_goals(&transport, &bus, BROADCASTER)
                .await
                .unwrap();

            assert_eq!(synced, 0, "body {body}");
            assert!(bus.taken().is_empty(), "body {body}");
        }
    }

    #[tokio::test]
    async fn a_failed_goals_request_returns_the_error_and_publishes_nothing() {
        let failures: [fn() -> HelixError; 4] = [
            || HelixError::ReauthRequired,
            || HelixError::RateLimited,
            || HelixError::Http {
                status: UNAVAILABLE,
                body: String::new(),
            },
            || HelixError::Transport("connection reset".to_owned()),
        ];
        for failure in failures {
            let transport = MockTransport::returning(Err(failure()));
            let bus = RecordingBus::default();

            let error = publish_current_goals(&transport, &bus, BROADCASTER)
                .await
                .unwrap_err();

            assert_eq!(
                std::mem::discriminant(&error),
                std::mem::discriminant(&failure()),
                "unexpected error {error:?}"
            );
            assert!(bus.taken().is_empty(), "events published after {error:?}");
        }
    }
}
