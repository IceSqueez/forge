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
