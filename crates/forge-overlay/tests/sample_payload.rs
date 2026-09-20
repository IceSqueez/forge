#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use forge_overlay::sample_payload;

const CHAT_MESSAGE_FIELDS: &[&str] = &[
    "channel",
    "message_text",
    "user_color",
    "user_id",
    "user_login",
];
const RESUBSCRIBE_FIELDS: &[&str] = &[
    "sub_cumulative_months",
    "sub_message",
    "sub_streak_months",
    "sub_tier",
    "user_id",
    "user_login",
];
const CHEER_FIELDS: &[&str] = &[
    "bits_amount",
    "cheer_is_anonymous",
    "cheer_message",
    "user_id",
    "user_login",
];
const FOLLOW_FIELDS: &[&str] = &["followed_at", "user"];
const SUBSCRIBE_FIELDS: &[&str] = &["is_gift", "tier", "user"];
const GIFT_SUBSCRIPTION_FIELDS: &[&str] = &["gifter", "is_anonymous", "recipient", "tier"];
const RAID_FIELDS: &[&str] = &[
    "direction",
    "from_broadcaster",
    "to_broadcaster",
    "viewer_count",
];
const PLAIN_FIELDS: &[&str] = &["message", "user"];

fn keys_of(event_kind: &str) -> BTreeSet<String> {
    sample_payload(event_kind)
        .as_object()
        .unwrap_or_else(|| panic!("{event_kind} sampled something other than an object"))
        .keys()
        .cloned()
        .collect()
}

fn expected(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|n| (*n).to_owned()).collect()
}

#[test]
fn a_sample_carries_exactly_the_fields_of_the_family_its_kind_names() {
    for (event_kind, fields) in [
        ("twitch.chat.message", CHAT_MESSAGE_FIELDS),
        ("kick.chat.message", CHAT_MESSAGE_FIELDS),
        ("twitch.channel.follow", FOLLOW_FIELDS),
        ("twitch.channel.subscribe", SUBSCRIBE_FIELDS),
        ("twitch.channel.subscription.message", RESUBSCRIBE_FIELDS),
        ("twitch.channel.subscription.gift", GIFT_SUBSCRIPTION_FIELDS),
        ("kick.channel.subscription.gifts", GIFT_SUBSCRIPTION_FIELDS),
        ("twitch.channel.cheer", CHEER_FIELDS),
        ("twitch.channel.raid", RAID_FIELDS),
        ("obs.scene.changed", PLAIN_FIELDS),
    ] {
        assert_eq!(
            keys_of(event_kind),
            expected(fields),
            "{event_kind} was sampled as the wrong family"
        );
    }
}

#[test]
fn an_overlay_kind_id_selects_the_family_its_own_default_wording_was_written_against() {
    for (kind_id, fields) in [
        ("overlay.alert", RESUBSCRIBE_FIELDS),
        ("overlay.chat", CHAT_MESSAGE_FIELDS),
        ("overlay.ticker", CHEER_FIELDS),
        ("overlay.frame", PLAIN_FIELDS),
        ("overlay.goal", PLAIN_FIELDS),
    ] {
        assert_eq!(
            keys_of(kind_id),
            expected(fields),
            "{kind_id} sampled a family its stored defaults never name"
        );
    }
}
