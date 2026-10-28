use std::collections::BTreeMap;

use forge_platform_core::{
    CollectionId, CollectionRevisions, QuickAction, QuickActionAccent, QuickActionField,
    QuickActionLiveness, QuickActions, SectionIcon,
};
use forge_types::{SubActionStep, Variant};

use super::TwitchIntegrationBundle;
use crate::account::identity::BroadcasterTier;
use crate::collection_keys::REWARDS_COLLECTION;
use crate::lifecycle::{LifecycleSnapshot, ShieldPhase};
use crate::quick_action_fields::{
    choice_field, choice_field_hint, int_field, multiline_field, text_field, toggle_field,
};

const TIER_LOCKED_REASON: &str = "Requires Twitch Affiliate or Partner";
const SHIELD_SCOPE_MISSING_REASON: &str = "Sign in to Twitch again to grant Shield mode access";

fn blank() -> Variant {
    Variant::String(String::new())
}

fn config(pairs: impl IntoIterator<Item = (&'static str, Variant)>) -> BTreeMap<String, Variant> {
    pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

fn group_badge(group: &str) -> (SectionIcon, QuickActionAccent) {
    match group {
        "Stream info" => (SectionIcon::new("edit"), QuickActionAccent::Info),
        "Polls" => (SectionIcon::new("chart-bar"), QuickActionAccent::Brand),
        "Predictions" => (
            SectionIcon::new("crystal-ball"),
            QuickActionAccent::AccentPinkLight,
        ),
        "Chat" => (SectionIcon::new("message-2"), QuickActionAccent::Brand),
        "Moderation" => (SectionIcon::new("shield"), QuickActionAccent::Danger),
        "Raids & ads" => (SectionIcon::new("businessplan"), QuickActionAccent::Bits),
        "Channel Points" => (
            SectionIcon::new("diamond"),
            QuickActionAccent::AccentPinkLight,
        ),
        _ => (SectionIcon::new("dot"), QuickActionAccent::Brand),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "each argument is one field of the quick-action template"
)]
fn quick_action(
    label: &str,
    icon: &str,
    accent: QuickActionAccent,
    enabled: bool,
    locked_reason: Option<String>,
    group: &str,
    destructive: bool,
    kind_id: &str,
    config: BTreeMap<String, Variant>,
    fields: Vec<QuickActionField>,
) -> QuickAction {
    let (group_icon, group_accent) = group_badge(group);
    QuickAction {
        label: label.to_owned(),
        icon: SectionIcon::new(icon),
        enabled,
        locked_reason,
        liveness: QuickActionLiveness::Unknown,
        group: Some(group.to_owned()),
        group_icon: Some(group_icon),
        group_accent: Some(group_accent),
        destructive,
        accent,
        subaction_template: SubActionStep {
            kind_id: kind_id.to_owned(),
            config,
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        },
        fields,
        collection: None,
    }
}

fn manage_collection_action(
    label: &str,
    icon: &str,
    accent: QuickActionAccent,
    enabled: bool,
    group: &str,
    collection: CollectionId,
) -> QuickAction {
    QuickAction {
        collection: Some(collection),
        ..quick_action(
            label,
            icon,
            accent,
            enabled,
            None,
            group,
            false,
            "",
            BTreeMap::new(),
            Vec::new(),
        )
    }
}

fn gated_on(action: QuickAction, liveness: QuickActionLiveness) -> QuickAction {
    QuickAction { liveness, ..action }
}

fn shield_mode_toggle(connected: bool, phase: ShieldPhase) -> QuickAction {
    let (label, icon, kind_id, liveness) = match phase {
        ShieldPhase::Active => (
            "End Shield mode",
            "shield-off",
            "twitch.moderation.shield_mode_off",
            QuickActionLiveness::Live,
        ),
        ShieldPhase::Inactive | ShieldPhase::Unknown | ShieldPhase::Unauthorized => (
            "Start Shield mode",
            "shield",
            "twitch.moderation.shield_mode_on",
            QuickActionLiveness::Unknown,
        ),
    };
    let unauthorized = phase == ShieldPhase::Unauthorized;
    gated_on(
        quick_action(
            label,
            icon,
            QuickActionAccent::Brand,
            connected && !unauthorized,
            unauthorized.then(|| SHIELD_SCOPE_MISSING_REASON.to_owned()),
            "Moderation",
            false,
            kind_id,
            BTreeMap::new(),
            Vec::new(),
        ),
        liveness,
    )
}

impl QuickActions for TwitchIntegrationBundle {
    fn revisions(&self) -> Option<CollectionRevisions> {
        Some(self.lifecycle.quick_action_revisions())
    }

    fn actions(&self) -> Vec<QuickAction> {
        let connected = self.is_chat_connected();
        let tier_locked = self.tier() == BroadcasterTier::Standard;
        let locked_reason = tier_locked.then(|| TIER_LOCKED_REASON.to_owned());
        let gated = connected && !tier_locked;
        let live: LifecycleSnapshot = self.lifecycle.snapshot();

        vec![
            quick_action(
                "Update title",
                "edit",
                QuickActionAccent::Info,
                connected,
                None,
                "Stream info",
                false,
                "twitch.channel.update_title",
                config([("title", blank())]),
                vec![text_field("title", "Stream title", "Stream title").required()],
            ),
            quick_action(
                "Set category / game",
                "device-gamepad-2",
                QuickActionAccent::Brand,
                connected,
                None,
                "Stream info",
                false,
                "twitch.channel.update_category",
                config([("category_name", blank())]),
                vec![text_field("category_name", "Category", "e.g. Just Chatting").required()],
            ),
            quick_action(
                "Update tags",
                "tags",
                QuickActionAccent::Success,
                connected,
                None,
                "Stream info",
                false,
                "twitch.channel.update_tags",
                config([("tags", blank())]),
                vec![text_field(
                    "tags",
                    "Tags (comma-sep)",
                    "e.g. English, Chill",
                )],
            ),
            quick_action(
                "Create stream marker",
                "bookmark",
                QuickActionAccent::Bits,
                connected,
                None,
                "Stream info",
                false,
                "twitch.channel.create_marker",
                config([("description", blank())]),
                vec![text_field("description", "Marker note", "optional")],
            ),
            gated_on(
                quick_action(
                    "Start poll",
                    "chart-bar",
                    QuickActionAccent::Brand,
                    gated,
                    locked_reason.clone(),
                    "Polls",
                    false,
                    "twitch.poll.start",
                    config([
                        ("title", blank()),
                        ("choices", blank()),
                        ("duration_seconds", Variant::Int(60)),
                    ]),
                    vec![
                        text_field("title", "Question", "Poll question").required(),
                        multiline_field("choices", "Choices (one per line)", "One choice per line")
                            .required(),
                        toggle_field(
                            "channel_points_voting_enabled",
                            "Channel Points voting",
                            false,
                        ),
                        int_field("duration_seconds", "Duration (seconds)", 60, 15, 1800),
                    ],
                ),
                live.poll_slot_free(),
            ),
            gated_on(
                quick_action(
                    "End poll (finish now)",
                    "player-stop",
                    QuickActionAccent::Warning,
                    gated,
                    locked_reason.clone(),
                    "Polls",
                    false,
                    "twitch.poll.end",
                    config([
                        ("poll_id", blank()),
                        ("status", Variant::String("terminated".to_owned())),
                    ]),
                    Vec::new(),
                ),
                live.poll_in_flight(),
            ),
            gated_on(
                quick_action(
                    "Cancel poll",
                    "x",
                    QuickActionAccent::Danger,
                    gated,
                    locked_reason.clone(),
                    "Polls",
                    true,
                    "twitch.poll.end",
                    config([
                        ("poll_id", blank()),
                        ("status", Variant::String("archived".to_owned())),
                    ]),
                    Vec::new(),
                ),
                live.poll_in_flight(),
            ),
            gated_on(
                quick_action(
                    "Start prediction",
                    "crystal-ball",
                    QuickActionAccent::AccentPinkLight,
                    gated,
                    locked_reason.clone(),
                    "Predictions",
                    false,
                    "twitch.prediction.start",
                    config([
                        ("title", blank()),
                        ("outcomes", blank()),
                        ("prediction_window_seconds", Variant::Int(120)),
                    ]),
                    vec![
                        text_field("title", "Title", "Prediction question").required(),
                        multiline_field(
                            "outcomes",
                            "Outcomes (one per line)",
                            "One outcome per line",
                        )
                        .required(),
                        int_field(
                            "prediction_window_seconds",
                            "Window (seconds)",
                            120,
                            30,
                            1800,
                        ),
                    ],
                ),
                live.prediction_slot_free(),
            ),
            gated_on(
                quick_action(
                    "Lock prediction",
                    "lock",
                    QuickActionAccent::Warning,
                    gated,
                    locked_reason.clone(),
                    "Predictions",
                    false,
                    "twitch.prediction.lock",
                    config([("prediction_id", blank())]),
                    Vec::new(),
                ),
                live.prediction_lockable(),
            ),
            gated_on(
                quick_action(
                    "Resolve / pay out",
                    "trophy",
                    QuickActionAccent::Success,
                    gated,
                    locked_reason.clone(),
                    "Predictions",
                    false,
                    "twitch.prediction.resolve",
                    config([("prediction_id", blank()), ("winning_outcome_id", blank())]),
                    Vec::new(),
                ),
                live.prediction_settleable(),
            ),
            gated_on(
                quick_action(
                    "Cancel & refund",
                    "x",
                    QuickActionAccent::Danger,
                    gated,
                    locked_reason.clone(),
                    "Predictions",
                    true,
                    "twitch.prediction.cancel",
                    config([("prediction_id", blank())]),
                    Vec::new(),
                ),
                live.prediction_settleable(),
            ),
            quick_action(
                "Send message",
                "send",
                QuickActionAccent::Brand,
                connected,
                None,
                "Chat",
                false,
                "twitch.chat.send_message",
                config([
                    ("message", blank()),
                    ("target", Variant::String("twitch".to_owned())),
                ]),
                vec![multiline_field("message", "Message", "Message").required()],
            ),
            quick_action(
                "Announcement",
                "speakerphone",
                QuickActionAccent::Warning,
                connected,
                None,
                "Chat",
                false,
                "twitch.chat.send_announcement",
                config([
                    ("message", blank()),
                    ("color", Variant::String("primary".to_owned())),
                ]),
                vec![
                    multiline_field("message", "Announcement", "Announcement text").required(),
                    choice_field(
                        "color",
                        "Color",
                        "primary",
                        &[
                            ("primary", "Primary"),
                            ("blue", "Blue"),
                            ("green", "Green"),
                            ("orange", "Orange"),
                            ("purple", "Purple"),
                        ],
                    ),
                ],
            ),
            quick_action(
                "Clear chat",
                "message-off",
                QuickActionAccent::Danger,
                connected,
                None,
                "Chat",
                true,
                "twitch.chat.clear",
                BTreeMap::new(),
                Vec::new(),
            ),
            quick_action(
                "Slow mode",
                "clock-hour-4",
                QuickActionAccent::Info,
                connected,
                None,
                "Chat",
                false,
                "twitch.chat.set_mode",
                config([
                    ("slow_mode", Variant::String("on".to_owned())),
                    ("slow_mode_wait_seconds", Variant::Int(10)),
                ]),
                vec![int_field(
                    "slow_mode_wait_seconds",
                    "Wait time (seconds)",
                    10,
                    3,
                    120,
                )],
            ),
            quick_action(
                "Followers-only mode",
                "user-check",
                QuickActionAccent::Success,
                connected,
                None,
                "Chat",
                false,
                "twitch.chat.set_mode",
                config([
                    ("follower_mode", Variant::String("on".to_owned())),
                    ("follower_mode_min_minutes", Variant::Int(10)),
                ]),
                vec![int_field(
                    "follower_mode_min_minutes",
                    "Min. account age (minutes)",
                    10,
                    0,
                    129_600,
                )],
            ),
            quick_action(
                "Emote-only mode",
                "mood-smile",
                QuickActionAccent::Bits,
                connected,
                None,
                "Chat",
                false,
                "twitch.chat.set_mode",
                config([("emote_only", Variant::String("on".to_owned()))]),
                vec![choice_field(
                    "emote_only",
                    "Emote-only mode",
                    "on",
                    &[("unchanged", "Unchanged"), ("on", "On"), ("off", "Off")],
                )],
            ),
            quick_action(
                "Timeout user",
                "clock-pause",
                QuickActionAccent::Warning,
                connected,
                None,
                "Moderation",
                false,
                "twitch.moderation.timeout_user",
                config([
                    ("target_user_login", blank()),
                    ("duration_seconds", Variant::Int(600)),
                    ("reason", blank()),
                ]),
                vec![
                    text_field("target_user_login", "Username", "username").required(),
                    int_field("duration_seconds", "Duration (seconds)", 600, 1, 1_209_600),
                    text_field("reason", "Reason", "optional"),
                ],
            ),
            quick_action(
                "Ban user",
                "ban",
                QuickActionAccent::Danger,
                connected,
                None,
                "Moderation",
                true,
                "twitch.moderation.ban_user",
                config([("target_user_login", blank()), ("reason", blank())]),
                vec![
                    text_field("target_user_login", "Username", "username").required(),
                    text_field("reason", "Reason", "optional"),
                ],
            ),
            quick_action(
                "Unban user",
                "lock-open",
                QuickActionAccent::Success,
                connected,
                None,
                "Moderation",
                false,
                "twitch.moderation.unban_user",
                config([("target_user_login", blank())]),
                vec![text_field("target_user_login", "Username", "username").required()],
            ),
            shield_mode_toggle(connected, self.lifecycle.shield_phase()),
            quick_action(
                "Add / remove VIP",
                "star",
                QuickActionAccent::Warning,
                connected,
                None,
                "Moderation",
                false,
                "twitch.moderation.add_vip",
                config([("target_user_login", blank())]),
                vec![text_field("target_user_login", "Username", "username").required()],
            ),
            quick_action(
                "Add / remove Mod",
                "sword",
                QuickActionAccent::Info,
                connected,
                None,
                "Moderation",
                false,
                "twitch.moderation.add_moderator",
                config([("target_user_login", blank())]),
                vec![text_field("target_user_login", "Username", "username").required()],
            ),
            gated_on(
                quick_action(
                    "Start raid",
                    "flag",
                    QuickActionAccent::Bits,
                    connected,
                    None,
                    "Raids & ads",
                    false,
                    "twitch.channel.start_raid",
                    config([("to_broadcaster_login", blank())]),
                    vec![
                        text_field("to_broadcaster_login", "Raid target", "channel name")
                            .required(),
                    ],
                ),
                live.raid_slot_free(),
            ),
            gated_on(
                quick_action(
                    "Cancel raid",
                    "flag-off",
                    QuickActionAccent::Danger,
                    connected,
                    None,
                    "Raids & ads",
                    true,
                    "twitch.channel.cancel_raid",
                    BTreeMap::new(),
                    Vec::new(),
                ),
                live.raid_in_flight(),
            ),
            quick_action(
                "Send shoutout",
                "speakerphone",
                QuickActionAccent::Warning,
                connected,
                None,
                "Raids & ads",
                false,
                "twitch.channel.send_shoutout",
                config([("to_broadcaster_login", blank())]),
                vec![text_field("to_broadcaster_login", "Channel", "channel name").required()],
            ),
            quick_action(
                "Run commercial",
                "clock",
                QuickActionAccent::Info,
                connected && !tier_locked,
                locked_reason.clone(),
                "Raids & ads",
                false,
                "twitch.channel.run_ad",
                config([("duration_seconds", Variant::String("90".to_owned()))]),
                vec![
                    choice_field_hint(
                        "duration_seconds",
                        "Duration",
                        "90",
                        &[
                            ("30", "30 seconds"),
                            ("60", "60 seconds"),
                            ("90", "90 seconds"),
                            ("120", "120 seconds"),
                            ("150", "150 seconds"),
                            ("180", "180 seconds"),
                        ],
                        "8+ min cooldown between ads",
                    )
                    .required(),
                ],
            ),
            quick_action(
                "Snooze next ad",
                "player-skip-forward",
                QuickActionAccent::Success,
                connected,
                None,
                "Raids & ads",
                false,
                "twitch.channel.snooze_ad",
                BTreeMap::new(),
                Vec::new(),
            ),
            manage_collection_action(
                "Manage rewards",
                "adjustments",
                QuickActionAccent::AccentPinkLight,
                connected,
                "Channel Points",
                CollectionId::new(REWARDS_COLLECTION),
            ),
            quick_action(
                "Fulfill redemption",
                "check",
                QuickActionAccent::Success,
                connected,
                None,
                "Channel Points",
                false,
                "twitch.channel_points.fulfill_redemption",
                config([("redemption_id", blank()), ("reward_id", blank())]),
                Vec::new(),
            ),
            quick_action(
                "Reject & refund",
                "x",
                QuickActionAccent::Danger,
                connected,
                None,
                "Channel Points",
                true,
                "twitch.channel_points.cancel_redemption",
                config([("redemption_id", blank()), ("reward_id", blank())]),
                Vec::new(),
            ),
        ]
    }
}
