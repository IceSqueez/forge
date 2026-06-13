use crate::subscriptions::{SubStatus, SubscriptionRecord, SubscriptionTracker};
use forge_events::{Event, EventSource};
use forge_runtime::EventBus;
use forge_types::OAuthToken;
use serde::Deserialize;
use std::sync::Arc;
use thiserror::Error;
use tracing::warn;

const EVENTSUB_URL: &str = "https://api.twitch.tv/helix/eventsub/subscriptions";
const EVENTSUB_PATH: &str = "/helix/eventsub/subscriptions";

#[derive(Debug, Error)]
pub(crate) enum SubscribeError {
    #[error("scope missing; re-authentication required")]
    ScopeMissing,
}

#[derive(Debug, Deserialize)]
struct SubscribeResponse {
    data: Vec<SubscriptionData>,
}

#[derive(Debug, Deserialize)]
struct SubscriptionData {
    id: String,
    #[serde(rename = "type")]
    subscription_type: String,
    condition: serde_json::Value,
}

struct TopicSpec {
    kind: &'static str,
    version: &'static str,
    condition_fn: fn(&str, &str) -> serde_json::Value,
}

fn condition_broadcaster(broadcaster_id: &str, _user_id: &str) -> serde_json::Value {
    serde_json::json!({ "broadcaster_user_id": broadcaster_id })
}

fn condition_chat(broadcaster_id: &str, user_id: &str) -> serde_json::Value {
    serde_json::json!({
        "broadcaster_user_id": broadcaster_id,
        "user_id": user_id,
    })
}

fn condition_follow(broadcaster_id: &str, user_id: &str) -> serde_json::Value {
    serde_json::json!({
        "broadcaster_user_id": broadcaster_id,
        "moderator_user_id": user_id,
    })
}

fn condition_raid(broadcaster_id: &str, _user_id: &str) -> serde_json::Value {
    serde_json::json!({ "to_broadcaster_user_id": broadcaster_id })
}

const TOPICS: &[TopicSpec] = &[
    TopicSpec {
        kind: "channel.chat.message",
        version: "1",
        condition_fn: condition_chat,
    },
    TopicSpec {
        kind: "channel.subscribe",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.subscription.gift",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.subscription.message",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.cheer",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.follow",
        version: "2",
        condition_fn: condition_follow,
    },
    TopicSpec {
        kind: "channel.raid",
        version: "1",
        condition_fn: condition_raid,
    },
    TopicSpec {
        kind: "stream.online",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "stream.offline",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.channel_points_custom_reward_redemption.add",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.chat.message_delete",
        version: "1",
        condition_fn: condition_chat,
    },
    TopicSpec {
        kind: "channel.chat.clear",
        version: "1",
        condition_fn: condition_chat,
    },
    TopicSpec {
        kind: "channel.hype_train.begin",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.hype_train.progress",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.hype_train.end",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.charity_campaign.donate",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.charity_campaign.start",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.charity_campaign.progress",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.charity_campaign.stop",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.ban",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.unban",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.moderator.add",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.moderator.remove",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.shield_mode.begin",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.shield_mode.end",
        version: "1",
        condition_fn: condition_broadcaster,
    },
];

pub(crate) async fn subscribe_all(
    token: &OAuthToken,
    client_id: &str,
    session_id: &str,
    broadcaster_id: &str,
    user_id: &str,
    bus: &Arc<EventBus>,
    tracker: &SubscriptionTracker,
) -> Result<(), SubscribeError> {
    {
        let mut records = tracker.write().unwrap_or_else(|p| p.into_inner());
        records.clear();
        for topic in TOPICS {
            records.push(SubscriptionRecord {
                kind: topic.kind.to_owned(),
                version: topic.version.to_owned(),
                status: SubStatus::Pending,
                subscription_id: None,
            });
        }
    }

    let http = reqwest::Client::new();

    for (i, topic) in TOPICS.iter().enumerate() {
        let condition = (topic.condition_fn)(broadcaster_id, user_id);
        let body = serde_json::json!({
            "type": topic.kind,
            "version": topic.version,
            "condition": condition,
            "transport": {
                "method": "websocket",
                "session_id": session_id,
            }
        });

        let result = http
            .post(EVENTSUB_URL)
            .header("Authorization", format!("Bearer {}", token.expose()))
            .header("Client-Id", client_id)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await;

        match result {
            Err(e) => {
                let reason = e.without_url().to_string();
                warn!(kind = topic.kind, error = %reason, "eventsub subscription network error");
                set_tracker_status(tracker, i, SubStatus::Failed(reason));
            }
            Ok(resp) => {
                let status = resp.status().as_u16();

                if status == 401 {
                    bus.publish(Event::new(
                        EventSource::Twitch,
                        "request.fail",
                        serde_json::json!({
                            "endpoint": EVENTSUB_PATH,
                            "status_code": status,
                            "body_snippet": "unauthorized",
                            "retry_after_secs": null,
                        }),
                    ));
                    set_tracker_status(tracker, i, SubStatus::Failed("unauthorized".to_owned()));
                    return Err(SubscribeError::ScopeMissing);
                }

                if !resp.status().is_success() {
                    let retry_after = extract_retry_after(&resp);
                    let body_text = resp.text().await.unwrap_or_default();
                    let body_snippet: String = body_text.chars().take(200).collect();
                    warn!(
                        kind = topic.kind,
                        status,
                        snippet = %body_snippet,
                        "eventsub subscription failed; continuing"
                    );
                    bus.publish(Event::new(
                        EventSource::Twitch,
                        "request.fail",
                        serde_json::json!({
                            "endpoint": EVENTSUB_PATH,
                            "status_code": status,
                            "body_snippet": body_snippet,
                            "retry_after_secs": retry_after,
                        }),
                    ));
                    set_tracker_status(tracker, i, SubStatus::Failed(format!("HTTP {status}")));
                    continue;
                }

                let body_text = resp.text().await.unwrap_or_default();
                if let Ok(parsed) = serde_json::from_str::<SubscribeResponse>(&body_text)
                    && let Some(sub) = parsed.data.first()
                {
                    bus.publish(Event::new(
                        EventSource::Twitch,
                        "eventsub.subscription.created",
                        serde_json::json!({
                            "type": sub.subscription_type,
                            "subscription_id": sub.id,
                            "condition": sub.condition,
                        }),
                    ));
                    let sub_id = sub.id.clone();
                    let mut records = tracker.write().unwrap_or_else(|p| p.into_inner());
                    if let Some(rec) = records.get_mut(i) {
                        rec.status = SubStatus::Active;
                        rec.subscription_id = Some(sub_id);
                    }
                } else {
                    set_tracker_status(
                        tracker,
                        i,
                        SubStatus::Failed("unreadable response".to_owned()),
                    );
                }
            }
        }
    }

    Ok(())
}

fn set_tracker_status(tracker: &SubscriptionTracker, index: usize, status: SubStatus) {
    let mut records = tracker.write().unwrap_or_else(|p| p.into_inner());
    if let Some(rec) = records.get_mut(index) {
        rec.status = status;
    }
}

fn extract_retry_after(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {

    #[tokio::test]
    async fn subscribe_network_error_strips_url() {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(1))
            .build()
            .unwrap();
        let err = client
            .post("https://192.0.2.1/helix/eventsub/subscriptions")
            .send()
            .await
            .unwrap_err();
        assert!(!err.without_url().to_string().contains("192.0.2.1"));
    }
}
