use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::*;
use crate::chat::topic_catalog::TOPICS;
use crate::vocabulary::events::automod::{
    AutomodMessageHeld, AutomodMessageUpdated, AutomodSettingsUpdated, AutomodTermsUpdated,
};
use crate::vocabulary::events::channel::{
    AdBreakBegan, ChannelUpdated, GuestStarGuestUpdated, GuestStarSessionBegan,
    GuestStarSessionEnded, GuestStarSettingsUpdated, ShoutoutReceived, ShoutoutSent,
    StreamWentOffline, StreamWentOnline, UserUpdated,
};
use crate::vocabulary::events::chat::{
    ChatCleared, ChatMessageDeleted, ChatMessageReceived, ChatSettingsUpdated, SharedChatBegan,
    SharedChatEnded, SharedChatUpdated, WhisperReceived,
};
use crate::vocabulary::events::engagement::{
    GoalBegan, GoalEnded, GoalProgressed, PollBegan, PollEnded, PollProgressed, PredictionBegan,
    PredictionEnded, PredictionLocked, PredictionProgressed,
};
use crate::vocabulary::events::moderation::{
    ModeratorAdded, ModeratorRemoved, ShieldModeBegan, ShieldModeEnded, SuspiciousUserMessaged,
    UnbanRequestCreated, UnbanRequestResolved, UserBanned, UserUnbanned, VipAdded, VipRemoved,
    WarningAcknowledged, WarningSent,
};
use crate::vocabulary::events::rewards::{
    AutomaticRewardRedeemed, CustomRewardAdded, CustomRewardRemoved, CustomRewardUpdated,
    RedemptionUpdated, RewardRedeemed,
};
use crate::vocabulary::events::support::{
    BitsCheered, ChannelFollowed, ChannelRaided, ChannelResubscribed, ChannelSubscribed,
    CharityCampaignProgressed, CharityCampaignStarted, CharityCampaignStopped, CharityDonated,
    HypeTrainBegan, HypeTrainEnded, HypeTrainProgressed, SubscriptionsGifted,
};

struct Person {
    id: &'static str,
    login: &'static str,
    name: &'static str,
}

const VIEWER: Person = Person {
    id: "4001",
    login: "viewer_one",
    name: "ViewerOne",
};

const MODERATOR: Person = Person {
    id: "5001",
    login: "mod_one",
    name: "ModOne",
};

const OTHER_CHANNEL: Person = Person {
    id: "6001",
    login: "other_chan",
    name: "OtherChan",
};

const OWN_CHANNEL: Person = Person {
    id: "bcast",
    login: "host_chan",
    name: "HostChan",
};

const AT: &str = "2026-10-29T18:00:00Z";
const LATER: &str = "2026-10-29T18:30:00Z";

impl Person {
    fn wire(&self, prefix: &str) -> Value {
        json!({
            format!("{prefix}_id"): self.id,
            format!("{prefix}_login"): self.login,
            format!("{prefix}_name"): self.name,
        })
    }

    fn nested(&self) -> Value {
        json!({"id": self.id, "login": self.login, "display_name": self.name})
    }
}

fn wire(parts: &[Value]) -> Value {
    let mut merged = serde_json::Map::new();
    for part in parts {
        if let Value::Object(fields) = part {
            merged.extend(fields.clone());
        }
    }
    Value::Object(merged)
}

fn decoded<T: TypedEvent>(event: &Event) -> Option<Value> {
    T::from_event(event).map(|typed| serde_json::to_value(typed).unwrap())
}

struct PayloadCase {
    topic: &'static str,
    scenario: &'static str,
    wire: Value,
    payload: Value,
    decode: fn(&Event) -> Option<Value>,
}

fn case(
    topic: &'static str,
    scenario: &'static str,
    wire: Value,
    payload: Value,
    decode: fn(&Event) -> Option<Value>,
) -> PayloadCase {
    PayloadCase {
        topic,
        scenario,
        wire,
        payload,
        decode,
    }
}

fn custom_reward_wire() -> Value {
    json!({"id": "rw-9", "title": "Hydrate", "cost": 500, "prompt": "drink", "is_enabled": true})
}

fn custom_reward_payload() -> Value {
    json!({"reward": {"id": "rw-9", "title": "Hydrate", "cost": 500, "prompt": "drink", "is_enabled": true}})
}

fn charity_campaign_wire() -> Value {
    json!({
        "id": "camp-2",
        "charity_name": "Helping Hands",
        "current_amount": {"value": 1200, "decimal_places": 2, "currency": "EUR"},
        "target_amount": {"value": 50000, "decimal_places": 2, "currency": "EUR"},
    })
}

fn charity_campaign_payload() -> Value {
    json!({"charity": {
        "id": "camp-2",
        "name": "Helping Hands",
        "current_amount_cents": 1200,
        "target_amount_cents": 50000,
        "currency_code": "EUR",
    }})
}

fn shared_chat_wire() -> Value {
    wire(&[
        json!({"session_id": "sc-1"}),
        OTHER_CHANNEL.wire("host_broadcaster_user"),
    ])
}

fn shared_chat_payload() -> Value {
    json!({"shared_chat": {"session_id": "sc-1"}, "host": OTHER_CHANNEL.nested()})
}

fn poll_choices_wire() -> Value {
    json!([{"id": "c1", "title": "Yes", "votes": 3}, {"id": "c2", "title": "No"}])
}

fn poll_choices_payload() -> Value {
    json!([
        {"id": "c1", "title": "Yes", "votes": 3},
        {"id": "c2", "title": "No", "votes": null},
    ])
}

fn prediction_outcomes_wire() -> Value {
    json!([
        {"id": "o1", "title": "Win", "color": "blue", "users": 2, "channel_points": 300},
        {"id": "o2", "title": "Lose", "color": "pink"},
    ])
}

fn prediction_outcomes_payload() -> Value {
    json!([
        {"id": "o1", "title": "Win", "color": "blue", "users": 2, "channel_points": 300},
        {"id": "o2", "title": "Lose", "color": "pink", "users": null, "channel_points": null},
    ])
}

fn payload_cases() -> Vec<PayloadCase> {
    vec![
        case(
            "channel.chat.message",
            "cheer relayed from a shared-chat guest channel",
            wire(&[
                OWN_CHANNEL.wire("broadcaster_user"),
                VIEWER.wire("chatter_user"),
                json!({
                    "message": {"text": "hello chat"},
                    "color": "#FF0000",
                    "badges": [{"set_id": "vip", "id": "1", "info": ""}],
                    "cheer": {"bits": 50},
                }),
                OTHER_CHANNEL.wire("source_broadcaster_user"),
            ]),
            json!({
                "channel": OWN_CHANNEL.login,
                "user": {"login": VIEWER.login, "id": VIEWER.id, "display_name": VIEWER.name, "roles": ["vip"]},
                "message": "hello chat",
                "badges": ["vip"],
                "color": "#FF0000",
                "cheer": {"bits": 50},
                "from_channel": {"login": OTHER_CHANNEL.login, "display_name": OTHER_CHANNEL.name},
            }),
            decoded::<ChatMessageReceived>,
        ),
        case(
            "channel.chat.message",
            "guest channel with an empty login carries no origin",
            wire(&[
                OWN_CHANNEL.wire("broadcaster_user"),
                VIEWER.wire("chatter_user"),
                json!({
                    "message": {"text": "hi"},
                    "color": "",
                    "source_broadcaster_user_id": OTHER_CHANNEL.id,
                    "source_broadcaster_user_login": "",
                    "source_broadcaster_user_name": OTHER_CHANNEL.name,
                }),
            ]),
            json!({
                "channel": OWN_CHANNEL.login,
                "user": {"login": VIEWER.login, "id": VIEWER.id, "display_name": VIEWER.name, "roles": []},
                "message": "hi",
                "badges": [],
                "color": "",
            }),
            decoded::<ChatMessageReceived>,
        ),
        case(
            "channel.subscribe",
            "gifted tier-1 sub",
            wire(&[
                VIEWER.wire("user"),
                json!({"tier": "1000", "is_gift": true}),
            ]),
            json!({"user": VIEWER.nested(), "tier": "1000", "is_gift": true}),
            decoded::<ChannelSubscribed>,
        ),
        case(
            "channel.subscription.message",
            "resub sharing its streak",
            wire(&[
                VIEWER.wire("user"),
                json!({
                    "tier": "2000",
                    "cumulative_months": 7,
                    "streak_months": 3,
                    "message": {"text": "seven months"},
                    "share_streak": true,
                }),
            ]),
            json!({
                "user": VIEWER.nested(),
                "tier": "2000",
                "cumulative_months": 7,
                "streak_months": 3,
                "message": "seven months",
                "share_streak": true,
            }),
            decoded::<ChannelResubscribed>,
        ),
        case(
            "channel.subscription.gift",
            "named gifter",
            wire(&[
                VIEWER.wire("user"),
                json!({"tier": "1000", "is_anonymous": false, "total": 5}),
            ]),
            json!({"tier": "1000", "is_anonymous": false, "gift_total": 5, "gifter": VIEWER.nested()}),
            decoded::<SubscriptionsGifted>,
        ),
        case(
            "channel.cheer",
            "named cheer",
            wire(&[
                VIEWER.wire("user"),
                json!({"bits": 100, "message": "cheer100 hype", "is_anonymous": false}),
            ]),
            json!({"bits": 100, "message": "cheer100 hype", "is_anonymous": false, "user": VIEWER.nested()}),
            decoded::<BitsCheered>,
        ),
        case(
            "channel.raid",
            "raid into the own channel",
            wire(&[
                OTHER_CHANNEL.wire("from_broadcaster_user"),
                OWN_CHANNEL.wire("to_broadcaster_user"),
                json!({"viewers": 42}),
            ]),
            json!({
                "direction": "received",
                "viewer_count": 42,
                "from_broadcaster": OTHER_CHANNEL.nested(),
                "to_broadcaster": OWN_CHANNEL.nested(),
            }),
            decoded::<ChannelRaided>,
        ),
        case(
            "channel.raid",
            "raid out of the own channel",
            wire(&[
                OWN_CHANNEL.wire("from_broadcaster_user"),
                OTHER_CHANNEL.wire("to_broadcaster_user"),
                json!({"viewers": 7}),
            ]),
            json!({
                "direction": "sent",
                "viewer_count": 7,
                "from_broadcaster": OWN_CHANNEL.nested(),
                "to_broadcaster": OTHER_CHANNEL.nested(),
            }),
            decoded::<ChannelRaided>,
        ),
        case(
            "channel.channel_points_custom_reward_redemption.add",
            "redemption with user input",
            wire(&[
                VIEWER.wire("user"),
                json!({
                    "id": "red-1",
                    "status": "unfulfilled",
                    "user_input": "play song",
                    "redeemed_at": AT,
                    "reward": {"id": "rw-1", "title": "Song", "cost": 300, "prompt": "name it"},
                }),
            ]),
            json!({
                "redemption": {"id": "red-1", "status": "unfulfilled", "user_input": "play song", "redeemed_at": AT},
                "user": VIEWER.nested(),
                "reward": {"id": "rw-1", "title": "Song", "cost": 300, "prompt": "name it"},
            }),
            decoded::<RewardRedeemed>,
        ),
        case(
            "channel.chat.message_delete",
            "deleted line",
            wire(&[VIEWER.wire("target_user"), json!({"message_id": "msg-1"})]),
            json!({"message_id": "msg-1", "target_user": VIEWER.nested()}),
            decoded::<ChatMessageDeleted>,
        ),
        case(
            "channel.chat.clear",
            "cleared chat",
            OWN_CHANNEL.wire("broadcaster_user"),
            json!({"broadcaster": {"id": OWN_CHANNEL.id, "login": OWN_CHANNEL.login}}),
            decoded::<ChatCleared>,
        ),
        case(
            "channel.follow",
            "new follower",
            wire(&[VIEWER.wire("user"), json!({"followed_at": AT})]),
            json!({"followed_at": AT, "user": VIEWER.nested()}),
            decoded::<ChannelFollowed>,
        ),
        case(
            "stream.online",
            "went live",
            wire(&[
                OWN_CHANNEL.wire("broadcaster_user"),
                json!({"id": "stream-1", "type": "live", "started_at": AT}),
            ]),
            json!({
                "stream": {"id": "stream-1", "type": "live", "started_at": AT},
                "broadcaster": {"id": OWN_CHANNEL.id, "login": OWN_CHANNEL.login},
            }),
            decoded::<StreamWentOnline>,
        ),
        case(
            "stream.offline",
            "went offline",
            OWN_CHANNEL.wire("broadcaster_user"),
            json!({"broadcaster": {"id": OWN_CHANNEL.id, "login": OWN_CHANNEL.login}}),
            decoded::<StreamWentOffline>,
        ),
        case(
            "channel.hype_train.begin",
            "train started",
            json!({"id": "ht-1", "level": 2, "total": 350, "goal": 1000, "progress": 340, "started_at": AT, "expires_at": LATER}),
            json!({"hype": {"id": "ht-1", "level": 2, "total": 350, "goal": 1000, "progress": 340, "started_at": AT, "expires_at": LATER}}),
            decoded::<HypeTrainBegan>,
        ),
        case(
            "channel.hype_train.progress",
            "train progressed",
            json!({"id": "ht-1", "level": 3, "total": 800, "goal": 1200, "progress": 790}),
            json!({"hype": {"id": "ht-1", "level": 3, "total": 800, "goal": 1200, "progress": 790}}),
            decoded::<HypeTrainProgressed>,
        ),
        case(
            "channel.hype_train.end",
            "train ended",
            json!({"id": "ht-1", "level": 5, "total": 9001, "ended_at": AT, "cooldown_ends_at": LATER}),
            json!({"hype": {"id": "ht-1", "level": 5, "total": 9001, "ended_at": AT, "cooldown_ends_at": LATER}}),
            decoded::<HypeTrainEnded>,
        ),
        case(
            "channel.charity_campaign.donate",
            "donation",
            wire(&[
                VIEWER.wire("user"),
                json!({
                    "campaign_id": "camp-1",
                    "charity_name": "Helping Hands",
                    "charity_description": "helps",
                    "charity_website": "https://example.org",
                    "amount": {"value": 2500, "decimal_places": 2, "currency": "USD"},
                }),
            ]),
            json!({
                "charity": {
                    "id": "camp-1",
                    "name": "Helping Hands",
                    "description": "helps",
                    "website": "https://example.org",
                    "amount_cents": 2500,
                    "currency_code": "USD",
                },
                "user": VIEWER.nested(),
            }),
            decoded::<CharityDonated>,
        ),
        case(
            "channel.charity_campaign.start",
            "campaign started",
            charity_campaign_wire(),
            charity_campaign_payload(),
            decoded::<CharityCampaignStarted>,
        ),
        case(
            "channel.charity_campaign.progress",
            "campaign progressed",
            charity_campaign_wire(),
            charity_campaign_payload(),
            decoded::<CharityCampaignProgressed>,
        ),
        case(
            "channel.charity_campaign.stop",
            "campaign stopped",
            charity_campaign_wire(),
            charity_campaign_payload(),
            decoded::<CharityCampaignStopped>,
        ),
        case(
            "channel.ban",
            "timeout with an end time",
            wire(&[
                VIEWER.wire("user"),
                MODERATOR.wire("moderator_user"),
                json!({"reason": "spam", "banned_at": AT, "ends_at": LATER, "is_permanent": false}),
            ]),
            json!({
                "user": VIEWER.nested(),
                "moderator": MODERATOR.nested(),
                "reason": "spam",
                "banned_at": AT,
                "ends_at": LATER,
                "is_permanent": false,
            }),
            decoded::<UserBanned>,
        ),
        case(
            "channel.ban",
            "permanent ban with a null end time and empty reason",
            wire(&[
                VIEWER.wire("user"),
                MODERATOR.wire("moderator_user"),
                json!({"reason": "", "banned_at": AT, "ends_at": null, "is_permanent": true}),
            ]),
            json!({
                "user": VIEWER.nested(),
                "moderator": MODERATOR.nested(),
                "reason": "",
                "banned_at": AT,
                "ends_at": null,
                "is_permanent": true,
            }),
            decoded::<UserBanned>,
        ),
        case(
            "channel.unban",
            "unban",
            wire(&[VIEWER.wire("user"), MODERATOR.wire("moderator_user")]),
            json!({"user": VIEWER.nested(), "moderator": MODERATOR.nested()}),
            decoded::<UserUnbanned>,
        ),
        case(
            "channel.moderator.add",
            "modded",
            VIEWER.wire("user"),
            json!({"user": VIEWER.nested()}),
            decoded::<ModeratorAdded>,
        ),
        case(
            "channel.moderator.remove",
            "unmodded",
            VIEWER.wire("user"),
            json!({"user": VIEWER.nested()}),
            decoded::<ModeratorRemoved>,
        ),
        case(
            "channel.vip.add",
            "vip granted",
            VIEWER.wire("user"),
            json!({"user": VIEWER.nested()}),
            decoded::<VipAdded>,
        ),
        case(
            "channel.vip.remove",
            "vip revoked",
            VIEWER.wire("user"),
            json!({"user": VIEWER.nested()}),
            decoded::<VipRemoved>,
        ),
        case(
            "channel.unban_request.create",
            "unban request",
            wire(&[VIEWER.wire("user"), json!({"id": "ur-1", "text": "sorry"})]),
            json!({"id": "ur-1", "user": VIEWER.nested(), "reason_text": "sorry"}),
            decoded::<UnbanRequestCreated>,
        ),
        case(
            "channel.unban_request.resolve",
            "unban request approved",
            wire(&[
                VIEWER.wire("user"),
                MODERATOR.wire("moderator_user"),
                json!({"id": "ur-1", "status": "approved", "resolution_text": "ok"}),
            ]),
            json!({
                "id": "ur-1",
                "user": VIEWER.nested(),
                "status": "approved",
                "moderator": MODERATOR.nested(),
                "resolution_text": "ok",
            }),
            decoded::<UnbanRequestResolved>,
        ),
        case(
            "channel.shield_mode.begin",
            "shield on",
            wire(&[MODERATOR.wire("moderator_user"), json!({"started_at": AT})]),
            json!({"moderator": MODERATOR.nested(), "started_at": AT}),
            decoded::<ShieldModeBegan>,
        ),
        case(
            "channel.shield_mode.end",
            "shield off",
            wire(&[MODERATOR.wire("moderator_user"), json!({"ended_at": AT})]),
            json!({"moderator": MODERATOR.nested(), "ended_at": AT}),
            decoded::<ShieldModeEnded>,
        ),
        case(
            "channel.shoutout.create",
            "shoutout given",
            wire(&[
                OTHER_CHANNEL.wire("to_broadcaster_user"),
                json!({"viewer_count": 12, "started_at": AT}),
            ]),
            json!({"to_broadcaster": OTHER_CHANNEL.nested(), "viewer_count": 12, "started_at": AT}),
            decoded::<ShoutoutSent>,
        ),
        case(
            "channel.shoutout.receive",
            "shoutout received",
            wire(&[
                OTHER_CHANNEL.wire("from_broadcaster_user"),
                json!({"viewer_count": 34, "started_at": AT}),
            ]),
            json!({"from_broadcaster": OTHER_CHANNEL.nested(), "viewer_count": 34, "started_at": AT}),
            decoded::<ShoutoutReceived>,
        ),
        case(
            "channel.suspicious_user.message",
            "restricted user line",
            wire(&[
                VIEWER.wire("user"),
                json!({"low_trust_status": "restricted", "message": {"text": "buy followers"}}),
            ]),
            json!({"user": VIEWER.nested(), "low_trust_status": "restricted", "message_text": "buy followers"}),
            decoded::<SuspiciousUserMessaged>,
        ),
        case(
            "channel.warning.acknowledge",
            "warning acknowledged",
            VIEWER.wire("user"),
            json!({"user": VIEWER.nested()}),
            decoded::<WarningAcknowledged>,
        ),
        case(
            "channel.warning.send",
            "warning citing a rule",
            wire(&[
                VIEWER.wire("user"),
                MODERATOR.wire("moderator_user"),
                json!({"reason": "be kind", "chat_rules_cited": ["No spam"]}),
            ]),
            json!({
                "user": VIEWER.nested(),
                "moderator": MODERATOR.nested(),
                "reason": "be kind",
                "chat_rules_cited": ["No spam"],
            }),
            decoded::<WarningSent>,
        ),
        case(
            "channel.warning.send",
            "warning without cited rules",
            wire(&[
                VIEWER.wire("user"),
                MODERATOR.wire("moderator_user"),
                json!({"reason": "", "chat_rules_cited": null}),
            ]),
            json!({
                "user": VIEWER.nested(),
                "moderator": MODERATOR.nested(),
                "reason": "",
                "chat_rules_cited": [],
            }),
            decoded::<WarningSent>,
        ),
        case(
            "channel.poll.begin",
            "poll opened with an unvoted choice",
            json!({"id": "poll-1", "title": "Next game?", "started_at": AT, "ends_at": LATER, "choices": poll_choices_wire()}),
            json!({
                "poll": {"id": "poll-1", "title": "Next game?", "started_at": AT, "ends_at": LATER},
                "choices": poll_choices_payload(),
            }),
            decoded::<PollBegan>,
        ),
        case(
            "channel.poll.progress",
            "poll progressed",
            json!({"id": "poll-1", "title": "Next game?", "choices": poll_choices_wire()}),
            json!({"poll": {"id": "poll-1", "title": "Next game?"}, "choices": poll_choices_payload()}),
            decoded::<PollProgressed>,
        ),
        case(
            "channel.poll.end",
            "poll completed",
            json!({"id": "poll-1", "title": "Next game?", "status": "completed", "ended_at": AT, "choices": poll_choices_wire()}),
            json!({
                "poll": {"id": "poll-1", "title": "Next game?", "status": "completed", "ended_at": AT},
                "choices": poll_choices_payload(),
            }),
            decoded::<PollEnded>,
        ),
        case(
            "channel.poll.end",
            "poll without choices",
            json!({"id": "poll-2", "title": "", "status": "archived", "ended_at": AT}),
            json!({
                "poll": {"id": "poll-2", "title": "", "status": "archived", "ended_at": AT},
                "choices": [],
            }),
            decoded::<PollEnded>,
        ),
        case(
            "channel.prediction.begin",
            "prediction opened with an unbacked outcome",
            json!({"id": "pred-1", "title": "Win?", "started_at": AT, "locks_at": LATER, "outcomes": prediction_outcomes_wire()}),
            json!({
                "prediction": {"id": "pred-1", "title": "Win?", "started_at": AT, "locks_at": LATER},
                "outcomes": prediction_outcomes_payload(),
            }),
            decoded::<PredictionBegan>,
        ),
        case(
            "channel.prediction.progress",
            "prediction progressed",
            json!({"id": "pred-1", "title": "Win?", "outcomes": prediction_outcomes_wire()}),
            json!({"prediction": {"id": "pred-1", "title": "Win?"}, "outcomes": prediction_outcomes_payload()}),
            decoded::<PredictionProgressed>,
        ),
        case(
            "channel.prediction.lock",
            "prediction locked",
            json!({"id": "pred-1", "title": "Win?", "locked_at": AT, "outcomes": prediction_outcomes_wire()}),
            json!({"prediction": {"id": "pred-1", "title": "Win?", "locked_at": AT}}),
            decoded::<PredictionLocked>,
        ),
        case(
            "channel.prediction.end",
            "prediction resolved",
            json!({"id": "pred-1", "title": "Win?", "winning_outcome_id": "o1", "status": "resolved", "ended_at": AT}),
            json!({"prediction": {
                "id": "pred-1",
                "title": "Win?",
                "winning_outcome_id": "o1",
                "status": "resolved",
                "ended_at": AT,
            }}),
            decoded::<PredictionEnded>,
        ),
        case(
            "channel.goal.begin",
            "goal started",
            json!({"id": "goal-1", "type": "follower", "description": "Road to 1k", "current_amount": 10, "target_amount": 1000, "started_at": AT}),
            json!({"goal": {
                "id": "goal-1",
                "type": "follower",
                "description": "Road to 1k",
                "current_amount": 10,
                "target_amount": 1000,
                "started_at": AT,
            }}),
            decoded::<GoalBegan>,
        ),
        case(
            "channel.goal.progress",
            "live goal progress",
            json!({"id": "goal-1", "type": "follower", "description": "Road to 1k", "current_amount": 250, "target_amount": 1000}),
            json!({"goal": {"id": "goal-1", "type": "follower", "current_amount": 250, "target_amount": 1000, "is_synced": false}}),
            decoded::<GoalProgressed>,
        ),
        case(
            "channel.goal.end",
            "goal achieved",
            json!({"id": "goal-1", "type": "follower", "current_amount": 1000, "target_amount": 1000, "is_achieved": true, "ended_at": AT}),
            json!({"goal": {
                "id": "goal-1",
                "type": "follower",
                "current_amount": 1000,
                "target_amount": 1000,
                "is_achieved": true,
                "ended_at": AT,
            }}),
            decoded::<GoalEnded>,
        ),
        case(
            "channel.channel_points_custom_reward.add",
            "reward created",
            custom_reward_wire(),
            custom_reward_payload(),
            decoded::<CustomRewardAdded>,
        ),
        case(
            "channel.channel_points_custom_reward.update",
            "reward edited",
            custom_reward_wire(),
            custom_reward_payload(),
            decoded::<CustomRewardUpdated>,
        ),
        case(
            "channel.channel_points_custom_reward.remove",
            "reward deleted",
            custom_reward_wire(),
            custom_reward_payload(),
            decoded::<CustomRewardRemoved>,
        ),
        case(
            "channel.channel_points_custom_reward_redemption.update",
            "redemption fulfilled",
            wire(&[
                VIEWER.wire("user"),
                json!({
                    "id": "red-1",
                    "status": "fulfilled",
                    "user_input": "",
                    "redeemed_at": AT,
                    "reward": {"id": "rw-1", "title": "Song", "cost": 300, "prompt": "name it"},
                }),
            ]),
            json!({
                "redemption": {"id": "red-1", "status": "fulfilled", "user_input": "", "redeemed_at": AT},
                "user": VIEWER.nested(),
                "reward": {"id": "rw-1", "title": "Song", "cost": 300},
            }),
            decoded::<RedemptionUpdated>,
        ),
        case(
            "automod.message.hold",
            "held by classifier",
            wire(&[
                VIEWER.wire("user"),
                json!({
                    "message_id": "held-1",
                    "reason": "automod",
                    "automod": {"category": "swearing", "level": 3},
                    "held_at": AT,
                    "message": {"text": "rude words"},
                }),
            ]),
            json!({
                "automod": {"message_id": "held-1", "category": "swearing", "level": 3, "held_at": AT},
                "reason": "automod",
                "blocked_term": null,
                "user": VIEWER.nested(),
                "message_text": "rude words",
            }),
            decoded::<AutomodMessageHeld>,
        ),
        case(
            "automod.message.hold",
            "held by a blocked term",
            wire(&[
                VIEWER.wire("user"),
                json!({
                    "message_id": "held-2",
                    "reason": "blocked_term",
                    "blocked_term": {"terms_found": [{"term_id": "t-1"}]},
                    "held_at": AT,
                    "message": {"text": "blocked"},
                }),
            ]),
            json!({
                "automod": {"message_id": "held-2", "category": null, "level": null, "held_at": AT},
                "reason": "blocked_term",
                "blocked_term": {"terms_found": ["t-1"]},
                "user": VIEWER.nested(),
                "message_text": "blocked",
            }),
            decoded::<AutomodMessageHeld>,
        ),
        case(
            "channel.chat_settings.update",
            "every mode on with durations",
            json!({
                "emote_mode": true,
                "follower_mode": true,
                "follower_mode_duration_minutes": 10,
                "slow_mode": true,
                "slow_mode_wait_time_seconds": 30,
                "subscriber_mode": true,
                "unique_chat_mode": true,
            }),
            json!({"settings": {
                "emote_mode": true,
                "follower_mode": true,
                "follower_mode_duration_minutes": 10,
                "slow_mode": true,
                "slow_mode_wait_time_seconds": 30,
                "subscriber_mode": true,
                "unique_chat_mode": true,
            }}),
            decoded::<ChatSettingsUpdated>,
        ),
        case(
            "channel.chat_settings.update",
            "modes off with null durations",
            json!({
                "emote_mode": false,
                "follower_mode": false,
                "follower_mode_duration_minutes": null,
                "slow_mode": false,
                "slow_mode_wait_time_seconds": null,
                "subscriber_mode": false,
                "unique_chat_mode": false,
            }),
            json!({"settings": {
                "emote_mode": false,
                "follower_mode": false,
                "follower_mode_duration_minutes": null,
                "slow_mode": false,
                "slow_mode_wait_time_seconds": null,
                "subscriber_mode": false,
                "unique_chat_mode": false,
            }}),
            decoded::<ChatSettingsUpdated>,
        ),
        case(
            "channel.guest_star_session.begin",
            "session opened",
            json!({"session_id": "gs-1", "started_at": AT}),
            json!({"session": {"id": "gs-1", "started_at": AT}}),
            decoded::<GuestStarSessionBegan>,
        ),
        case(
            "channel.guest_star_session.end",
            "session closed",
            json!({"session_id": "gs-1", "started_at": AT, "ended_at": LATER}),
            json!({"session": {"id": "gs-1", "started_at": AT, "ended_at": LATER}}),
            decoded::<GuestStarSessionEnded>,
        ),
        case(
            "channel.guest_star_settings.update",
            "settings changed",
            json!({
                "slot_count": 4,
                "group_layout": "tiled",
                "is_moderator_send_live_enabled": true,
                "is_browser_source_audio_enabled": false,
            }),
            json!({"settings": {
                "slot_count": 4,
                "group_layout": "tiled",
                "is_moderator_send_live_enabled": true,
                "is_browser_source_audio_enabled": false,
            }}),
            decoded::<GuestStarSettingsUpdated>,
        ),
        case(
            "channel.guest_star_guest.update",
            "guest went live",
            wire(&[
                VIEWER.wire("guest_user"),
                json!({
                    "session_id": "gs-1",
                    "slot_id": "2",
                    "state": "live",
                    "host_video_enabled": true,
                    "host_audio_enabled": false,
                    "host_volume": 80,
                }),
            ]),
            json!({
                "session_id": "gs-1",
                "slot_id": "2",
                "state": "live",
                "guest": VIEWER.nested(),
                "host": {"video_enabled": true, "audio_enabled": false, "volume": 80},
            }),
            decoded::<GuestStarGuestUpdated>,
        ),
        case(
            "channel.guest_star_guest.update",
            "empty-string guest identity reads as absent",
            json!({
                "session_id": "gs-1",
                "slot_id": "",
                "state": "removed",
                "guest_user_id": "",
                "guest_user_login": "",
                "guest_user_name": "",
            }),
            json!({
                "session_id": "gs-1",
                "slot_id": "",
                "state": "removed",
                "guest": {"id": null, "login": null, "display_name": null},
                "host": {"video_enabled": false, "audio_enabled": false, "volume": 0},
            }),
            decoded::<GuestStarGuestUpdated>,
        ),
        case(
            "automod.settings.update",
            "overall level set",
            wire(&[
                MODERATOR.wire("moderator_user"),
                json!({"overall_level": 2}),
            ]),
            json!({"moderator": MODERATOR.nested(), "overall_level": 2}),
            decoded::<AutomodSettingsUpdated>,
        ),
        case(
            "automod.settings.update",
            "per-category levels leave the overall level null",
            wire(&[
                MODERATOR.wire("moderator_user"),
                json!({"overall_level": null, "swearing": 1}),
            ]),
            json!({"moderator": MODERATOR.nested(), "overall_level": null}),
            decoded::<AutomodSettingsUpdated>,
        ),
        case(
            "automod.terms.update",
            "blocked term added",
            wire(&[
                MODERATOR.wire("moderator_user"),
                json!({"action": "add_blocked", "terms": ["badword"]}),
            ]),
            json!({"moderator": MODERATOR.nested(), "action": "add_blocked", "terms": ["badword"]}),
            decoded::<AutomodTermsUpdated>,
        ),
        case(
            "automod.message.update",
            "held message approved",
            wire(&[
                VIEWER.wire("user"),
                MODERATOR.wire("moderator_user"),
                json!({
                    "message_id": "held-1",
                    "status": "approved",
                    "reason": "automod",
                    "automod": {"category": "bullying", "level": 2},
                    "message": {"text": "borderline"},
                }),
            ]),
            json!({
                "automod": {"message_id": "held-1", "status": "approved", "category": "bullying", "level": 2},
                "reason": "automod",
                "blocked_term": null,
                "user": VIEWER.nested(),
                "moderator": MODERATOR.nested(),
                "message_text": "borderline",
            }),
            decoded::<AutomodMessageUpdated>,
        ),
        case(
            "channel.shared_chat.begin",
            "shared chat started",
            shared_chat_wire(),
            shared_chat_payload(),
            decoded::<SharedChatBegan>,
        ),
        case(
            "channel.shared_chat.update",
            "shared chat changed",
            shared_chat_wire(),
            shared_chat_payload(),
            decoded::<SharedChatUpdated>,
        ),
        case(
            "channel.shared_chat.end",
            "shared chat ended",
            shared_chat_wire(),
            shared_chat_payload(),
            decoded::<SharedChatEnded>,
        ),
        case(
            "channel.update",
            "title and category changed",
            json!({"title": "Speedruns", "language": "uk", "category_id": "509658", "category_name": "Just Chatting"}),
            json!({"channel": {"title": "Speedruns", "language": "uk", "category_id": "509658", "category_name": "Just Chatting"}}),
            decoded::<ChannelUpdated>,
        ),
        case(
            "channel.ad_break.begin",
            "manual ad break",
            wire(&[
                MODERATOR.wire("requester_user"),
                json!({"duration_seconds": 90, "is_automatic": false, "started_at": AT}),
            ]),
            json!({
                "ad_break": {"duration_seconds": 90, "is_automatic": false, "started_at": AT},
                "requester": MODERATOR.nested(),
            }),
            decoded::<AdBreakBegan>,
        ),
        case(
            "channel.channel_points_automatic_reward_redemption.add",
            "highlighted message",
            wire(&[
                VIEWER.wire("user"),
                json!({
                    "id": "ar-1",
                    "redeemed_at": AT,
                    "reward": {"type": "send_highlighted_message", "cost": 300},
                }),
            ]),
            json!({
                "redemption": {"id": "ar-1", "redeemed_at": AT},
                "user": VIEWER.nested(),
                "reward": {"type": "send_highlighted_message", "cost": 300},
            }),
            decoded::<AutomaticRewardRedeemed>,
        ),
        case(
            "user.whisper.message",
            "whisper received",
            wire(&[
                VIEWER.wire("from_user"),
                json!({"whisper_id": "w-1", "whisper": {"text": "psst"}}),
            ]),
            json!({
                "user": {"id": VIEWER.id, "login": VIEWER.login, "display_name": VIEWER.name, "color": null},
                "whisper": {"text": "psst"},
                "whisper_thread_id": "w-1",
            }),
            decoded::<WhisperReceived>,
        ),
        case(
            "user.update",
            "profile edited",
            wire(&[VIEWER.wire("user"), json!({"description": "bio"})]),
            json!({"user": {"id": VIEWER.id, "login": VIEWER.login, "display_name": VIEWER.name, "description": "bio"}}),
            decoded::<UserUpdated>,
        ),
    ]
}

fn route_through_session(topic: &str, wire: &Value) -> Event {
    let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
    let session = make_session(&bus);
    let mut sub = bus.subscribe();
    let route = dispatch::route_for(topic).unwrap_or_else(|| panic!("no route for {topic}"));

    route(&session, wire, "frame-msg-1");

    let event = sub
        .try_recv()
        .unwrap()
        .unwrap_or_else(|| panic!("{topic} published nothing"));
    assert!(
        sub.try_recv().unwrap().is_none(),
        "{topic} published more than one event"
    );
    event
}

fn without_chat_envelopes(payload: &Value) -> Value {
    let mut payload = payload.clone();
    if let Some(fields) = payload.as_object_mut() {
        for key in [
            ChatPayload::KEY,
            ChatModerationPayload::KEY,
            ChatViewer::KEY,
            ChatReply::KEY,
        ] {
            fields.remove(key);
        }
    }
    payload
}

#[test]
fn every_routed_topic_publishes_the_pinned_payload_under_its_twitch_kind() {
    for case in payload_cases() {
        let event = route_through_session(case.topic, &case.wire);

        assert_eq!(
            (event.kind.as_str(), event.source),
            (
                format!("twitch.{}", case.topic).as_str(),
                EventSource::Twitch
            ),
            "{} ({})",
            case.topic,
            case.scenario
        );
        assert_eq!(
            without_chat_envelopes(&event.payload),
            case.payload,
            "{} ({})",
            case.topic,
            case.scenario
        );
    }
}

#[test]
fn every_published_payload_decodes_through_its_typed_event_back_to_the_same_wire_value() {
    for case in payload_cases() {
        let event = route_through_session(case.topic, &case.wire);

        assert_eq!(
            (case.decode)(&event),
            Some(without_chat_envelopes(&event.payload)),
            "{} ({})",
            case.topic,
            case.scenario
        );
    }
}

#[test]
fn the_payload_table_pins_every_subscribed_topic() {
    let subscribed: BTreeSet<&str> = TOPICS.iter().map(|topic| topic.kind).collect();
    let pinned: BTreeSet<&str> = payload_cases().iter().map(|case| case.topic).collect();

    assert_eq!(
        subscribed.difference(&pinned).collect::<Vec<_>>(),
        Vec::<&&str>::new()
    );
}

#[test]
fn live_and_connect_sync_goal_progress_differ_only_in_the_synced_flag() {
    let goal = json!({
        "id": "goal-7",
        "broadcaster_user_id": OWN_CHANNEL.id,
        "type": "subscription",
        "description": "Road to 100 subs",
        "current_amount": 42,
        "target_amount": 100,
        "started_at": AT,
    });

    let live = route_through_session("channel.goal.progress", &goal);
    let synced = goal_progress_event(&goal, GoalProgressOrigin::ConnectSync);

    let mut synced_payload = synced.payload.clone();
    synced_payload["goal"]["is_synced"] = json!(false);
    assert_eq!(
        (live.kind.as_str(), &live.payload),
        (synced.kind.as_str(), &synced_payload)
    );
}
