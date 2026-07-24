use crate::subscriptions::{SubStatus, SubscriptionRecord, SubscriptionTracker};
use forge_events::{Event, EventPublisher, EventSource};
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

fn condition_moderator(broadcaster_id: &str, user_id: &str) -> serde_json::Value {
    serde_json::json!({
        "broadcaster_user_id": broadcaster_id,
        "moderator_user_id": user_id,
    })
}

fn condition_raid(broadcaster_id: &str, _user_id: &str) -> serde_json::Value {
    serde_json::json!({ "to_broadcaster_user_id": broadcaster_id })
}

fn condition_raid_sent(broadcaster_id: &str, _user_id: &str) -> serde_json::Value {
    serde_json::json!({ "from_broadcaster_user_id": broadcaster_id })
}

fn condition_user(_broadcaster_id: &str, user_id: &str) -> serde_json::Value {
    serde_json::json!({ "user_id": user_id })
}

/// `channel.raid` is subscribed twice under different condition keys (incoming vs sent);
/// this disambiguates the two tracker rows since both otherwise share the same kind string.
fn display_kind(topic: &TopicSpec) -> String {
    if std::ptr::fn_addr_eq(
        topic.condition_fn,
        condition_raid as fn(&str, &str) -> serde_json::Value,
    ) {
        format!("{} (incoming)", topic.kind)
    } else if std::ptr::fn_addr_eq(
        topic.condition_fn,
        condition_raid_sent as fn(&str, &str) -> serde_json::Value,
    ) {
        format!("{} (outgoing)", topic.kind)
    } else {
        topic.kind.to_owned()
    }
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
        condition_fn: condition_moderator,
    },
    TopicSpec {
        kind: "channel.raid",
        version: "1",
        condition_fn: condition_raid,
    },
    // channel.raid accepts from_broadcaster_user_id OR to_broadcaster_user_id, not both - a second subscription covers sent raids.
    TopicSpec {
        kind: "channel.raid",
        version: "1",
        condition_fn: condition_raid_sent,
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
        version: "2",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.hype_train.progress",
        version: "2",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.hype_train.end",
        version: "2",
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
        kind: "channel.vip.add",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.vip.remove",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.unban_request.create",
        version: "1",
        condition_fn: condition_moderator,
    },
    TopicSpec {
        kind: "channel.unban_request.resolve",
        version: "1",
        condition_fn: condition_moderator,
    },
    TopicSpec {
        kind: "channel.shield_mode.begin",
        version: "1",
        condition_fn: condition_moderator,
    },
    TopicSpec {
        kind: "channel.shield_mode.end",
        version: "1",
        condition_fn: condition_moderator,
    },
    TopicSpec {
        kind: "channel.shoutout.create",
        version: "1",
        condition_fn: condition_moderator,
    },
    TopicSpec {
        kind: "channel.shoutout.receive",
        version: "1",
        condition_fn: condition_moderator,
    },
    TopicSpec {
        kind: "channel.suspicious_user.message",
        version: "1",
        condition_fn: condition_moderator,
    },
    TopicSpec {
        kind: "channel.warning.acknowledge",
        version: "1",
        condition_fn: condition_moderator,
    },
    TopicSpec {
        kind: "channel.warning.send",
        version: "1",
        condition_fn: condition_moderator,
    },
    TopicSpec {
        kind: "channel.poll.begin",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.poll.progress",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.poll.end",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.prediction.begin",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.prediction.progress",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.prediction.lock",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.prediction.end",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.goal.begin",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.goal.progress",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.goal.end",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.channel_points_custom_reward.add",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.channel_points_custom_reward.update",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.channel_points_custom_reward.remove",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.channel_points_custom_reward_redemption.update",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "automod.message.hold",
        version: "2",
        condition_fn: condition_moderator,
    },
    TopicSpec {
        kind: "channel.chat_settings.update",
        version: "1",
        condition_fn: condition_chat,
    },
    TopicSpec {
        kind: "channel.guest_star_session.begin",
        version: "beta",
        condition_fn: condition_moderator,
    },
    TopicSpec {
        kind: "channel.guest_star_session.end",
        version: "beta",
        condition_fn: condition_moderator,
    },
    TopicSpec {
        kind: "channel.guest_star_settings.update",
        version: "beta",
        condition_fn: condition_moderator,
    },
    TopicSpec {
        kind: "channel.guest_star_guest.update",
        version: "beta",
        condition_fn: condition_moderator,
    },
    TopicSpec {
        kind: "automod.settings.update",
        version: "1",
        condition_fn: condition_moderator,
    },
    TopicSpec {
        kind: "automod.terms.update",
        version: "1",
        condition_fn: condition_moderator,
    },
    TopicSpec {
        kind: "automod.message.update",
        version: "2",
        condition_fn: condition_moderator,
    },
    TopicSpec {
        kind: "channel.shared_chat.begin",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.shared_chat.update",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.shared_chat.end",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.update",
        version: "2",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.ad_break.begin",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "channel.channel_points_automatic_reward_redemption.add",
        version: "1",
        condition_fn: condition_broadcaster,
    },
    TopicSpec {
        kind: "user.whisper.message",
        version: "1",
        condition_fn: condition_user,
    },
    TopicSpec {
        kind: "user.update",
        version: "1",
        condition_fn: condition_user,
    },
];

pub(crate) async fn subscribe_all(
    token: &OAuthToken,
    client_id: &str,
    session_id: &str,
    broadcaster_id: &str,
    user_id: &str,
    bus: &Arc<dyn EventPublisher>,
    tracker: &SubscriptionTracker,
) -> Result<(), SubscribeError> {
    {
        let mut records = tracker.write().unwrap_or_else(|p| p.into_inner());
        records.clear();
        for topic in TOPICS {
            records.push(SubscriptionRecord {
                kind: display_kind(topic),
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
                        "twitch.eventsub.subscription.created",
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
    use super::{TOPICS, display_kind};

    #[test]
    fn display_kind_labels_raid_direction_and_passes_other_topics_through() {
        let mut incoming = 0;
        let mut outgoing = 0;
        for topic in TOPICS {
            let label = display_kind(topic);
            let condition = (topic.condition_fn)("B", "U");
            let is_raid = topic.kind == "channel.raid";
            if is_raid && condition.get("to_broadcaster_user_id").is_some() {
                assert_eq!(label, "channel.raid (incoming)");
                incoming += 1;
            } else if is_raid && condition.get("from_broadcaster_user_id").is_some() {
                assert_eq!(label, "channel.raid (outgoing)");
                outgoing += 1;
            } else {
                assert_eq!(
                    label, topic.kind,
                    "{} must pass through unchanged",
                    topic.kind
                );
            }
        }
        assert_eq!(
            (incoming, outgoing),
            (1, 1),
            "expected exactly one incoming and one outgoing raid topic"
        );
    }

    #[test]
    fn automod_subscription_types_omit_the_channel_prefix() {
        let automod: Vec<&str> = TOPICS
            .iter()
            .map(|t| t.kind)
            .filter(|k| k.contains("automod"))
            .collect();
        assert!(
            !automod.is_empty(),
            "expected automod subscriptions to be present"
        );
        for kind in automod {
            assert!(
                kind.starts_with("automod."),
                "automod subscription type {kind} must omit the channel. prefix Twitch rejects"
            );
        }
    }

    #[test]
    fn guest_star_slot_subscription_is_not_requested() {
        assert!(
            !TOPICS
                .iter()
                .any(|t| t.kind == "channel.guest_star_slot.update"),
            "channel.guest_star_slot.update is no longer a valid Twitch subscription type"
        );
    }

    #[test]
    fn moderator_scoped_types_send_both_broadcaster_and_moderator_ids() {
        let moderator_types = [
            "channel.shield_mode.begin",
            "channel.shield_mode.end",
            "channel.shoutout.create",
            "channel.shoutout.receive",
            "channel.suspicious_user.message",
            "channel.warning.acknowledge",
            "channel.warning.send",
            "automod.message.hold",
            "automod.message.update",
            "automod.settings.update",
            "automod.terms.update",
            "channel.guest_star_session.begin",
            "channel.guest_star_session.end",
            "channel.guest_star_settings.update",
            "channel.guest_star_guest.update",
        ];
        for kind in moderator_types {
            let topic = TOPICS.iter().find(|t| t.kind == kind).unwrap();
            let condition = (topic.condition_fn)("BROADCASTER", "SELF");
            assert_eq!(
                condition["broadcaster_user_id"], "BROADCASTER",
                "{kind} must send broadcaster_user_id"
            );
            assert_eq!(
                condition["moderator_user_id"], "SELF",
                "{kind} must send moderator_user_id"
            );
        }
    }

    #[test]
    fn version_pinned_types_request_version_two() {
        let version_two_types = [
            "channel.hype_train.begin",
            "channel.hype_train.progress",
            "channel.hype_train.end",
            "channel.follow",
            "automod.message.hold",
            "automod.message.update",
        ];
        for kind in version_two_types {
            let topic = TOPICS.iter().find(|t| t.kind == kind).unwrap();
            assert_eq!(topic.version, "2", "{kind} must request version 2");
        }
    }

    #[test]
    fn broadcaster_only_type_omits_moderator_user_id() {
        let topic = TOPICS
            .iter()
            .find(|t| t.kind == "channel.subscribe")
            .unwrap();
        let condition = (topic.condition_fn)("BROADCASTER", "SELF");
        assert!(
            condition.get("moderator_user_id").is_none(),
            "channel.subscribe must not carry moderator_user_id"
        );
        assert_eq!(condition["broadcaster_user_id"], "BROADCASTER");
    }

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
