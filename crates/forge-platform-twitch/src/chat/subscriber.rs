use crate::event_kinds;
use crate::helix::{CONFLICT, HelixError, HelixMethod, HelixRequest, HelixTransport};
use crate::subscriptions::{SubStatus, SubscriptionRecord, SubscriptionTracker};
use forge_events::{Event, EventPublisher, EventSource};
use futures_util::stream::{self, StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, warn};

const EVENTSUB_PATH: &str = "/helix/eventsub/subscriptions";
const SUBSCRIBE_CONCURRENCY: usize = 10;
const UNREADABLE_RESPONSE: &str = "unreadable response";
const FORBIDDEN: u16 = reqwest::StatusCode::FORBIDDEN.as_u16();

#[derive(Debug, Error)]
pub(crate) enum SubscribeError {
    #[error("scope missing; re-authentication required")]
    ScopeMissing,
    #[error("access token rejected")]
    Unauthorized,
}

enum TopicOutcome {
    Active,
    AlreadyExists,
    Failed,
    ScopeRejected,
    Unauthorized,
}

#[derive(Default)]
pub(crate) struct SubscribePass {
    active: usize,
    already_exists: usize,
    failed: usize,
}

impl SubscribePass {
    pub(crate) fn any_live(&self) -> bool {
        self.active > 0
    }
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

#[derive(Clone)]
struct SubscribeCtx {
    helix: Arc<dyn HelixTransport>,
    session_id: String,
    broadcaster_id: String,
    user_id: String,
    bus: Arc<dyn EventPublisher>,
}

pub(crate) async fn subscribe_all(
    helix: &Arc<dyn HelixTransport>,
    session_id: &str,
    broadcaster_id: &str,
    user_id: &str,
    bus: &Arc<dyn EventPublisher>,
    tracker: &SubscriptionTracker,
) -> Result<SubscribePass, SubscribeError> {
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

    let ctx = SubscribeCtx {
        helix: Arc::clone(helix),
        session_id: session_id.to_owned(),
        broadcaster_id: broadcaster_id.to_owned(),
        user_id: user_id.to_owned(),
        bus: Arc::clone(bus),
    };

    let pending: Vec<_> = TOPICS
        .iter()
        .enumerate()
        .map(|(index, topic)| {
            let ctx = ctx.clone();
            let tracker = Arc::clone(tracker);
            async move { subscribe_one(ctx, tracker, index, topic).await }
        })
        .collect();

    debug!(
        topics = TOPICS.len(),
        concurrency = SUBSCRIBE_CONCURRENCY,
        "requesting eventsub subscriptions"
    );

    let mut outcomes = stream::iter(pending).buffer_unordered(SUBSCRIBE_CONCURRENCY);

    let mut pass = SubscribePass::default();
    let mut scope_rejected = 0usize;
    let mut unauthorized = 0usize;
    while let Some(outcome) = outcomes.next().await {
        match outcome {
            TopicOutcome::Active => pass.active += 1,
            TopicOutcome::AlreadyExists => pass.already_exists += 1,
            TopicOutcome::Failed => pass.failed += 1,
            TopicOutcome::ScopeRejected => scope_rejected += 1,
            TopicOutcome::Unauthorized => unauthorized += 1,
        }
    }

    debug!(
        topics = TOPICS.len(),
        active = pass.active,
        already_exists = pass.already_exists,
        failed = pass.failed,
        scope_rejected,
        unauthorized,
        "eventsub subscription pass complete"
    );

    if scope_rejected > 0 {
        return Err(SubscribeError::ScopeMissing);
    }
    if unauthorized > 0 {
        return Err(SubscribeError::Unauthorized);
    }

    Ok(pass)
}

async fn subscribe_one(
    ctx: SubscribeCtx,
    tracker: SubscriptionTracker,
    index: usize,
    topic: &TopicSpec,
) -> TopicOutcome {
    let condition = (topic.condition_fn)(&ctx.broadcaster_id, &ctx.user_id);
    let body = serde_json::json!({
        "type": topic.kind,
        "version": topic.version,
        "condition": condition,
        "transport": {
            "method": "websocket",
            "session_id": ctx.session_id,
        }
    });

    let request = HelixRequest::new(HelixMethod::Post, EVENTSUB_PATH)
        .body(body)
        .expect_status(CONFLICT);
    match ctx.helix.execute(request).await {
        Ok(response) => record_created(&ctx, &tracker, index, response),
        Err(error) => {
            let (reason, outcome) = classify_failure(&error);
            if matches!(outcome, TopicOutcome::Failed | TopicOutcome::AlreadyExists) {
                warn!(kind = topic.kind, error = %error, "eventsub subscription failed; continuing");
            }
            set_tracker_status(&tracker, index, SubStatus::Failed(reason));
            outcome
        }
    }
}

fn classify_failure(error: &HelixError) -> (String, TopicOutcome) {
    match error {
        HelixError::ReauthRequired => ("unauthorized".to_owned(), TopicOutcome::Unauthorized),
        HelixError::Http { status, .. } if *status == FORBIDDEN => {
            ("missing scope".to_owned(), TopicOutcome::ScopeRejected)
        }
        already if already.is_already_exists() => (
            "already exists on another session".to_owned(),
            TopicOutcome::AlreadyExists,
        ),
        HelixError::Http { status, .. } => (format!("HTTP {status}"), TopicOutcome::Failed),
        HelixError::UndecodableResponse(_) => {
            (UNREADABLE_RESPONSE.to_owned(), TopicOutcome::Failed)
        }
        HelixError::RateLimited | HelixError::Credentials(_) | HelixError::Transport(_) => {
            (error.to_string(), TopicOutcome::Failed)
        }
    }
}

fn record_created(
    ctx: &SubscribeCtx,
    tracker: &SubscriptionTracker,
    index: usize,
    response: serde_json::Value,
) -> TopicOutcome {
    let created = serde_json::from_value::<SubscribeResponse>(response)
        .ok()
        .and_then(|parsed| parsed.data.into_iter().next());
    let Some(sub) = created else {
        set_tracker_status(
            tracker,
            index,
            SubStatus::Failed(UNREADABLE_RESPONSE.to_owned()),
        );
        return TopicOutcome::Failed;
    };
    ctx.bus.publish(Event::new(
        EventSource::Twitch,
        event_kinds::EVENTSUB_SUBSCRIPTION_CREATED,
        serde_json::json!({
            "type": sub.subscription_type,
            "subscription_id": sub.id,
            "condition": sub.condition,
        }),
    ));
    let mut records = tracker.write().unwrap_or_else(|p| p.into_inner());
    if let Some(rec) = records.get_mut(index) {
        rec.status = SubStatus::Active;
        rec.subscription_id = Some(sub.id);
    }
    TopicOutcome::Active
}

fn set_tracker_status(tracker: &SubscriptionTracker, index: usize, status: SubStatus) {
    let mut records = tracker.write().unwrap_or_else(|p| p.into_inner());
    if let Some(rec) = records.get_mut(index) {
        rec.status = status;
    }
}

pub(crate) fn mark_revoked(
    tracker: &SubscriptionTracker,
    subscription_id: &str,
    kind: &str,
    status: &str,
) {
    let mut records = tracker.write().unwrap_or_else(|p| p.into_inner());
    let by_id = records
        .iter()
        .position(|rec| rec.subscription_id.as_deref() == Some(subscription_id));
    let index = by_id.or_else(|| records.iter().position(|rec| rec.kind == kind));
    if let Some(rec) = index.and_then(|i| records.get_mut(i)) {
        rec.status = SubStatus::Failed(format!("revoked: {status}"));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
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

    use super::{
        EVENTSUB_PATH, SubStatus, SubscribeError, SubscribePass, SubscriptionTracker, subscribe_all,
    };
    use crate::bot_account::test_support::RecordingBus;
    use crate::helix::{FixedTokenSource, HelixHttpTransport, HelixTransport};
    use forge_events::EventPublisher;
    use forge_net::RateLimiter;
    use forge_net::test_support::RecordingRateLimiter;
    use forge_platform_core::EndpointSurface;
    use forge_types::OAuthToken;
    use reqwest::StatusCode;
    use std::sync::Arc;
    use std::time::Duration;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TOKEN_SENTINEL: &str = "FAKE_EVENTSUB_TOKEN_SENTINEL_k41";
    const PROBED_TOPIC: &str = "stream.online";

    struct PassRun {
        result: Result<SubscribePass, SubscribeError>,
        tracker: SubscriptionTracker,
        bus: Arc<RecordingBus>,
        limiter: Arc<RecordingRateLimiter>,
    }

    impl PassRun {
        fn status_of(&self, kind: &str) -> SubStatus {
            record_status(&self.tracker, kind).0
        }

        fn request_fails(&self) -> Vec<forge_events::Event> {
            self.bus.of_kind("request.fail")
        }
    }

    async fn pass_against(api_url: &str, limiter: RecordingRateLimiter) -> PassRun {
        let bus = Arc::new(RecordingBus::default());
        let limiter = Arc::new(limiter);
        let publisher: Arc<dyn EventPublisher> = bus.clone();
        let endpoints = crate::sub_actions::test_support::endpoints_with(&[(
            EndpointSurface::TwitchApi,
            api_url,
        )]);
        let gateway = crate::helix_gateway::helix_gateway(
            &endpoints,
            Arc::clone(&limiter) as Arc<dyn RateLimiter>,
            Arc::clone(&publisher),
        )
        .unwrap();
        let helix: Arc<dyn HelixTransport> = Arc::new(HelixHttpTransport::new(
            gateway,
            "client_id".to_owned(),
            Arc::new(FixedTokenSource::new(OAuthToken::new(TOKEN_SENTINEL))),
        ));
        let tracker = SubscriptionTracker::default();
        let result = subscribe_all(
            &helix,
            "session_id",
            "broadcaster_id",
            "user_id",
            &publisher,
            &tracker,
        )
        .await;
        PassRun {
            result,
            tracker,
            bus,
            limiter,
        }
    }

    async fn run_subscribe(server: &MockServer) -> PassRun {
        pass_against(&server.uri(), RecordingRateLimiter::granting()).await
    }

    async fn mount_success(server: &MockServer, kind: &str, id: &str) {
        Mock::given(method("POST"))
            .and(path(EVENTSUB_PATH))
            .and(body_string_contains(format!(r#""type":"{kind}""#)))
            .respond_with(
                ResponseTemplate::new(StatusCode::OK.as_u16()).set_body_json(serde_json::json!({
                    "data": [{ "id": id, "type": kind, "condition": {} }]
                })),
            )
            .with_priority(1)
            .mount(server)
            .await;
    }

    async fn mount_reply(server: &MockServer, kind: &str, reply: ResponseTemplate) {
        Mock::given(method("POST"))
            .and(path(EVENTSUB_PATH))
            .and(body_string_contains(format!(r#""type":"{kind}""#)))
            .respond_with(reply)
            .with_priority(1)
            .mount(server)
            .await;
    }

    async fn mount_status(server: &MockServer, kind: &str, status: StatusCode) {
        mount_reply(
            server,
            kind,
            ResponseTemplate::new(status.as_u16()).set_body_string("rejected"),
        )
        .await;
    }

    async fn mount_catch_all_success(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path(EVENTSUB_PATH))
            .respond_with(
                ResponseTemplate::new(StatusCode::OK.as_u16()).set_body_json(serde_json::json!({
                    "data": [{ "id": "sub-generic", "type": "generic", "condition": {} }]
                })),
            )
            .mount(server)
            .await;
    }

    async fn mount_catch_all_conflict(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path(EVENTSUB_PATH))
            .respond_with(
                ResponseTemplate::new(StatusCode::CONFLICT.as_u16()).set_body_json(
                    serde_json::json!({
                        "error": "Conflict",
                        "message": "subscription already exists",
                        "status": StatusCode::CONFLICT.as_u16(),
                    }),
                ),
            )
            .mount(server)
            .await;
    }

    async fn pass_with_probed_topic_replying(reply: ResponseTemplate) -> PassRun {
        let server = MockServer::start().await;
        mount_reply(&server, PROBED_TOPIC, reply).await;
        mount_catch_all_success(&server).await;
        run_subscribe(&server).await
    }

    fn record_status(tracker: &SubscriptionTracker, kind: &str) -> (SubStatus, Option<String>) {
        let records = tracker.read().unwrap_or_else(|p| p.into_inner());
        let rec = records.iter().find(|r| r.kind == kind).unwrap();
        (rec.status.clone(), rec.subscription_id.clone())
    }

    fn failed(reason: &str) -> SubStatus {
        SubStatus::Failed(reason.to_owned())
    }

    #[tokio::test]
    async fn every_topic_post_debits_the_shared_broadcaster_limiter() {
        let server = MockServer::start().await;
        mount_catch_all_success(&server).await;

        let run = run_subscribe(&server).await;

        assert_eq!(run.limiter.acquire_count(), super::TOPICS.len());
    }

    #[tokio::test]
    async fn mixed_outcomes_surface_scope_missing_yet_map_each_result_to_its_own_record() {
        let server = MockServer::start().await;
        mount_success(&server, "channel.chat.message", "sub-chat").await;
        mount_success(&server, "channel.follow", "sub-follow").await;
        mount_success(&server, "channel.cheer", "sub-cheer").await;
        mount_status(&server, "channel.ban", StatusCode::UNAUTHORIZED).await;
        mount_status(&server, "channel.subscribe", StatusCode::FORBIDDEN).await;
        mount_status(&server, "stream.online", StatusCode::INTERNAL_SERVER_ERROR).await;
        mount_catch_all_success(&server).await;

        let run = run_subscribe(&server).await;

        assert!(
            matches!(run.result, Err(SubscribeError::ScopeMissing)),
            "a scope rejection among the topics must surface as ScopeMissing after all futures finish"
        );

        for (kind, expected_id) in [
            ("channel.chat.message", "sub-chat"),
            ("channel.follow", "sub-follow"),
            ("channel.cheer", "sub-cheer"),
        ] {
            let (status, id) = record_status(&run.tracker, kind);
            assert_eq!(status, SubStatus::Active, "{kind} must be recorded Active");
            assert_eq!(
                id.as_deref(),
                Some(expected_id),
                "{kind} must keep the subscription_id from its own response, not a neighbour's"
            );
        }

        assert_eq!(run.status_of("channel.ban"), failed("unauthorized"));
        assert_eq!(run.status_of("channel.subscribe"), failed("missing scope"));
        assert_eq!(run.status_of("stream.online"), failed("HTTP 500"));
    }

    #[tokio::test]
    async fn a_topic_failing_without_an_auth_rejection_records_its_reason_and_the_pass_stands() {
        let cases = [
            (
                ResponseTemplate::new(StatusCode::INTERNAL_SERVER_ERROR.as_u16()),
                failed("HTTP 500"),
            ),
            (
                ResponseTemplate::new(StatusCode::TOO_MANY_REQUESTS.as_u16())
                    .insert_header("Retry-After", "7"),
                failed("rate limited"),
            ),
            (
                ResponseTemplate::new(StatusCode::SERVICE_UNAVAILABLE.as_u16())
                    .insert_header("Retry-After", "4"),
                failed("HTTP 503"),
            ),
            (
                ResponseTemplate::new(StatusCode::SERVICE_UNAVAILABLE.as_u16()),
                failed("HTTP 503"),
            ),
            (
                ResponseTemplate::new(StatusCode::CONFLICT.as_u16()),
                failed("already exists on another session"),
            ),
            (
                ResponseTemplate::new(StatusCode::ACCEPTED.as_u16()),
                failed("unreadable response"),
            ),
            (
                ResponseTemplate::new(StatusCode::ACCEPTED.as_u16())
                    .set_body_json(serde_json::json!({ "data": [] })),
                failed("unreadable response"),
            ),
            (
                ResponseTemplate::new(StatusCode::ACCEPTED.as_u16()).set_body_string("not-json"),
                failed("unreadable response"),
            ),
        ];

        for (reply, expected) in cases {
            let run = pass_with_probed_topic_replying(reply).await;

            assert_eq!(run.status_of(PROBED_TOPIC), expected);
            assert!(
                matches!(&run.result, Ok(pass) if pass.any_live()),
                "{expected:?}: only a 401 or 403 may stop the pass"
            );
            assert_eq!(
                run.status_of("channel.chat.message"),
                SubStatus::Active,
                "{expected:?}: the other topics must still subscribe"
            );
        }
    }

    #[tokio::test]
    async fn remote_throttling_on_one_topic_reaches_the_shared_limiter_and_request_fail() {
        let cases = [
            (
                ResponseTemplate::new(StatusCode::TOO_MANY_REQUESTS.as_u16())
                    .insert_header("Retry-After", "7"),
                vec![Duration::from_secs(7)],
                serde_json::json!(7),
            ),
            (
                ResponseTemplate::new(StatusCode::TOO_MANY_REQUESTS.as_u16()),
                vec![Duration::from_secs(5)],
                serde_json::json!(5),
            ),
            (
                ResponseTemplate::new(StatusCode::SERVICE_UNAVAILABLE.as_u16())
                    .insert_header("Retry-After", "4"),
                vec![Duration::from_secs(4)],
                serde_json::json!(4),
            ),
            (
                ResponseTemplate::new(StatusCode::SERVICE_UNAVAILABLE.as_u16()),
                Vec::new(),
                serde_json::Value::Null,
            ),
        ];

        for (reply, expected_pauses, expected_retry_after) in cases {
            let run = pass_with_probed_topic_replying(reply).await;

            let fails = run.request_fails();
            assert_eq!(run.limiter.observed_throttles(), expected_pauses);
            assert_eq!(fails.len(), 1, "{expected_pauses:?}");
            assert_eq!(fails[0].payload["endpoint"], EVENTSUB_PATH);
            assert_eq!(
                fails[0].payload["retry_after_secs"], expected_retry_after,
                "{expected_pauses:?}"
            );
        }
    }

    #[tokio::test]
    async fn only_a_conflicting_topic_create_is_kept_out_of_request_fail() {
        let cases = [
            (
                ResponseTemplate::new(StatusCode::CONFLICT.as_u16()).set_body_json(
                    serde_json::json!({
                        "error": "Conflict",
                        "status": StatusCode::CONFLICT.as_u16(),
                        "message": "subscription already exists",
                    }),
                ),
                failed("already exists on another session"),
                0,
            ),
            (
                ResponseTemplate::new(StatusCode::BAD_REQUEST.as_u16())
                    .set_body_string("subscription already exists"),
                failed("HTTP 400"),
                1,
            ),
        ];

        for (reply, expected, request_fails) in cases {
            let run = pass_with_probed_topic_replying(reply).await;

            assert_eq!(
                (run.status_of(PROBED_TOPIC), run.request_fails().len()),
                (expected, request_fails)
            );
        }
    }

    #[tokio::test]
    async fn an_exhausted_local_budget_fails_every_topic_as_rate_limited_without_request_fail() {
        let server = MockServer::start().await;
        mount_catch_all_success(&server).await;

        let run = pass_against(&server.uri(), RecordingRateLimiter::exhausted()).await;

        let records = run.tracker.read().unwrap().clone();
        assert!(
            records
                .iter()
                .all(|record| record.status == failed("rate limited")),
            "got {records:?}"
        );
        assert!(run.request_fails().is_empty());
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_auth_rejection_publishes_request_fail_with_the_masked_upstream_excerpt() {
        for (status, verdict) in [
            (StatusCode::UNAUTHORIZED, "unauthorized"),
            (StatusCode::FORBIDDEN, "missing scope"),
        ] {
            let body = format!(r#"{{"message":"Missing scope for {TOKEN_SENTINEL}"}}"#);
            let run = pass_with_probed_topic_replying(
                ResponseTemplate::new(status.as_u16()).set_body_string(body),
            )
            .await;

            let fails = run.request_fails();
            assert_eq!(fails.len(), 1, "{status}");
            let snippet = fails[0].payload["body_snippet"].as_str().unwrap();
            assert_eq!(fails[0].payload["status_code"], status.as_u16());
            assert!(snippet.contains("Missing scope for"), "{status}: {snippet}");
            assert!(!snippet.contains(TOKEN_SENTINEL), "{status}: {snippet}");
            assert_eq!(run.status_of(PROBED_TOPIC), failed(verdict));
        }
    }

    #[tokio::test]
    async fn an_auth_rejection_stops_the_pass_with_its_own_error() {
        for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
            let run = pass_with_probed_topic_replying(ResponseTemplate::new(status.as_u16())).await;

            let stopped = match run.result {
                Err(SubscribeError::Unauthorized) => Some(StatusCode::UNAUTHORIZED),
                Err(SubscribeError::ScopeMissing) => Some(StatusCode::FORBIDDEN),
                Ok(_) => None,
            };
            assert_eq!(stopped, Some(status));
        }
    }

    #[tokio::test]
    async fn a_transport_failure_records_a_reason_without_the_url_or_the_token() {
        let run = pass_against("http://127.0.0.1:1", RecordingRateLimiter::granting()).await;

        let records = run.tracker.read().unwrap().clone();
        for record in records {
            assert!(
                matches!(&record.status, SubStatus::Failed(reason)
                    if !reason.is_empty()
                        && !reason.contains("127.0.0.1")
                        && !reason.contains(EVENTSUB_PATH)
                        && !reason.contains(TOKEN_SENTINEL)),
                "{record:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_conflict_records_the_other_session_as_owner_rather_than_a_bare_status() {
        let server = MockServer::start().await;
        mount_catch_all_conflict(&server).await;

        let run = run_subscribe(&server).await;

        assert!(
            matches!(&run.result, Ok(pass) if !pass.any_live()),
            "a conflict is not a scope rejection, so the pass succeeds - but nothing it claimed \
             delivers on this socket, so the pass is not live either"
        );
        assert_eq!(
            run.status_of("channel.chat.message"),
            failed("already exists on another session")
        );
    }

    #[test]
    fn the_pass_summary_counts_every_outcome_class_apart() {
        let (result, lines) = crate::log_capture::capture_blocking(tracing::Level::DEBUG, async {
            let server = MockServer::start().await;
            mount_success(&server, "channel.chat.message", "sub-chat").await;
            mount_status(&server, "stream.online", StatusCode::INTERNAL_SERVER_ERROR).await;
            mount_catch_all_conflict(&server).await;

            run_subscribe(&server).await.result
        });

        assert!(matches!(&result, Ok(pass) if pass.any_live()));
        let summary = crate::log_capture::forge_lines(&lines)
            .into_iter()
            .find(|line| line.message() == "eventsub subscription pass complete")
            .expect("the pass must log its summary");
        let counted = |name: &str| summary.field(name).unwrap_or_default().to_owned();
        assert_eq!(
            (
                counted("active"),
                counted("failed"),
                counted("scope_rejected"),
                counted("already_exists"),
            ),
            (
                "1".to_owned(),
                "1".to_owned(),
                "0".to_owned(),
                (TOPICS.len() - 2).to_string(),
            ),
            "got {:?}",
            summary.fields
        );
    }
}
