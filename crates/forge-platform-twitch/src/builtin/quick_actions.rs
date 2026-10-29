use std::collections::BTreeMap;

use forge_platform_core::{
    CollectionId, CollectionRevisions, QuickAction, QuickActionAccent, QuickActionField,
    QuickActionLiveness, QuickActions, SectionIcon,
};
use forge_registry::{ChoiceField, ConfigChoice, DurationField};
use forge_types::{SubActionStep, Variant};

use super::TwitchIntegrationBundle;
use crate::account::identity::BroadcasterTier;
use crate::collection_keys::REWARDS_COLLECTION;
use crate::lifecycle::{LifecycleSnapshot, ShieldPhase};
use crate::quick_action_fields::{
    choice_field, choice_field_hint, int_field, multiline_field, text_field, toggle_field,
};
use crate::sub_actions::{
    AD_LENGTH, ANNOUNCEMENT_COLOR, AdLength, AnnouncementColor, EMOTE_ONLY, FOLLOWER_MODE_MIN_AGE,
    ModeToggle, POLL_DURATION, PREDICTION_WINDOW, PollEndStatus, SLOW_MODE_WAIT, TIMEOUT_DURATION,
};
use crate::vocabulary::{self, config_keys, sub_action_ids};

const TIER_LOCKED_REASON: &str = "Requires Twitch Affiliate or Partner";
const SHIELD_SCOPE_MISSING_REASON: &str = "Sign in to Twitch again to grant Shield mode access";
const QUICK_AD_LENGTH: AdLength = AdLength::Seconds90;
const QUICK_SLOW_MODE_WAIT_SECONDS: i64 = 10;
const QUICK_FOLLOWER_MIN_MINUTES: i64 = 10;
const SEND_MESSAGE_KIND_ID: &str = "twitch.chat.send_message";
const SEND_MESSAGE_TARGET_KEY: &str = "target";

fn blank() -> Variant {
    Variant::String(String::new())
}

fn choice(value: impl ConfigChoice) -> Variant {
    Variant::String(value.wire_name().to_owned())
}

fn config(pairs: impl IntoIterator<Item = (&'static str, Variant)>) -> BTreeMap<String, Variant> {
    pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

fn choice_options<T: ConfigChoice>(
    label: fn(T) -> &'static str,
) -> Vec<(&'static str, &'static str)> {
    T::ALL
        .iter()
        .map(|choice| (choice.wire_name(), label(*choice)))
        .collect()
}

fn duration_field(field: &DurationField, label: &str, default: i64) -> QuickActionField {
    int_field(
        field.key,
        label,
        default,
        field.bounds.min,
        field.bounds.max,
    )
}

fn default_choice<T: ConfigChoice>(field: &ChoiceField<T>) -> &'static str {
    field.default.wire_name()
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
            sub_action_ids::MODERATION_SHIELD_MODE_OFF,
            QuickActionLiveness::Live,
        ),
        ShieldPhase::Inactive | ShieldPhase::Unknown | ShieldPhase::Unauthorized => (
            "Start Shield mode",
            "shield",
            sub_action_ids::MODERATION_SHIELD_MODE_ON,
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
                sub_action_ids::CHANNEL_UPDATE_TITLE,
                BTreeMap::new(),
                vec![text_field(config_keys::TITLE, "Stream title", "Stream title").required()],
            ),
            quick_action(
                "Set category / game",
                "device-gamepad-2",
                QuickActionAccent::Brand,
                connected,
                None,
                "Stream info",
                false,
                sub_action_ids::CHANNEL_UPDATE_CATEGORY,
                BTreeMap::new(),
                vec![
                    text_field(config_keys::CATEGORY_NAME, "Category", "e.g. Just Chatting")
                        .required(),
                ],
            ),
            quick_action(
                "Update tags",
                "tags",
                QuickActionAccent::Success,
                connected,
                None,
                "Stream info",
                false,
                sub_action_ids::CHANNEL_UPDATE_TAGS,
                BTreeMap::new(),
                vec![text_field(
                    config_keys::TAGS,
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
                sub_action_ids::CHANNEL_CREATE_MARKER,
                BTreeMap::new(),
                vec![text_field(
                    config_keys::DESCRIPTION,
                    "Marker note",
                    "optional",
                )],
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
                    sub_action_ids::POLL_START,
                    BTreeMap::new(),
                    vec![
                        text_field(config_keys::TITLE, "Question", "Poll question").required(),
                        multiline_field(
                            config_keys::CHOICES,
                            "Choices (one per line)",
                            "One choice per line",
                        )
                        .required(),
                        toggle_field(
                            config_keys::CHANNEL_POINTS_VOTING_ENABLED,
                            "Channel Points voting",
                            false,
                        ),
                        duration_field(&POLL_DURATION, "Duration (seconds)", POLL_DURATION.default),
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
                    sub_action_ids::POLL_END,
                    config([(config_keys::POLL_ID, blank())]),
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
                    sub_action_ids::POLL_END,
                    config([
                        (config_keys::POLL_ID, blank()),
                        (config_keys::STATUS, choice(PollEndStatus::Archived)),
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
                    sub_action_ids::PREDICTION_START,
                    BTreeMap::new(),
                    vec![
                        text_field(config_keys::TITLE, "Title", "Prediction question").required(),
                        multiline_field(
                            config_keys::OUTCOMES,
                            "Outcomes (one per line)",
                            "One outcome per line",
                        )
                        .required(),
                        duration_field(
                            &PREDICTION_WINDOW,
                            "Window (seconds)",
                            PREDICTION_WINDOW.default,
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
                    sub_action_ids::PREDICTION_LOCK,
                    config([(config_keys::PREDICTION_ID, blank())]),
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
                    sub_action_ids::PREDICTION_RESOLVE,
                    config([(config_keys::PREDICTION_ID, blank())]),
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
                    sub_action_ids::PREDICTION_CANCEL,
                    config([(config_keys::PREDICTION_ID, blank())]),
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
                SEND_MESSAGE_KIND_ID,
                config([
                    (config_keys::MESSAGE, blank()),
                    (
                        SEND_MESSAGE_TARGET_KEY,
                        Variant::String(vocabulary::PLATFORM_ID.to_owned()),
                    ),
                ]),
                vec![multiline_field(config_keys::MESSAGE, "Message", "Message").required()],
            ),
            quick_action(
                "Announcement",
                "speakerphone",
                QuickActionAccent::Warning,
                connected,
                None,
                "Chat",
                false,
                sub_action_ids::CHAT_SEND_ANNOUNCEMENT,
                BTreeMap::new(),
                vec![
                    multiline_field(config_keys::MESSAGE, "Announcement", "Announcement text")
                        .required(),
                    choice_field(
                        ANNOUNCEMENT_COLOR.key,
                        "Color",
                        default_choice(&ANNOUNCEMENT_COLOR),
                        &choice_options(AnnouncementColor::label),
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
                sub_action_ids::CHAT_CLEAR,
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
                sub_action_ids::CHAT_SET_MODE,
                config([(config_keys::SLOW_MODE, choice(ModeToggle::On))]),
                vec![duration_field(
                    &SLOW_MODE_WAIT,
                    "Wait time (seconds)",
                    QUICK_SLOW_MODE_WAIT_SECONDS,
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
                sub_action_ids::CHAT_SET_MODE,
                config([(config_keys::FOLLOWER_MODE, choice(ModeToggle::On))]),
                vec![duration_field(
                    &FOLLOWER_MODE_MIN_AGE,
                    "Min. account age (minutes)",
                    QUICK_FOLLOWER_MIN_MINUTES,
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
                sub_action_ids::CHAT_SET_MODE,
                BTreeMap::new(),
                vec![choice_field(
                    EMOTE_ONLY.key,
                    "Emote-only mode",
                    ModeToggle::On.wire_name(),
                    &choice_options(ModeToggle::label),
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
                sub_action_ids::MODERATION_TIMEOUT_USER,
                BTreeMap::new(),
                vec![
                    text_field(config_keys::TARGET_USER_LOGIN, "Username", "username").required(),
                    duration_field(
                        &TIMEOUT_DURATION,
                        "Duration (seconds)",
                        TIMEOUT_DURATION.default,
                    ),
                    text_field(config_keys::REASON, "Reason", "optional"),
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
                sub_action_ids::MODERATION_BAN_USER,
                BTreeMap::new(),
                vec![
                    text_field(config_keys::TARGET_USER_LOGIN, "Username", "username").required(),
                    text_field(config_keys::REASON, "Reason", "optional"),
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
                sub_action_ids::MODERATION_UNBAN_USER,
                BTreeMap::new(),
                vec![text_field(config_keys::TARGET_USER_LOGIN, "Username", "username").required()],
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
                sub_action_ids::MODERATION_ADD_VIP,
                BTreeMap::new(),
                vec![text_field(config_keys::TARGET_USER_LOGIN, "Username", "username").required()],
            ),
            quick_action(
                "Add / remove Mod",
                "sword",
                QuickActionAccent::Info,
                connected,
                None,
                "Moderation",
                false,
                sub_action_ids::MODERATION_ADD_MODERATOR,
                BTreeMap::new(),
                vec![text_field(config_keys::TARGET_USER_LOGIN, "Username", "username").required()],
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
                    sub_action_ids::CHANNEL_START_RAID,
                    BTreeMap::new(),
                    vec![
                        text_field(
                            config_keys::TO_BROADCASTER_LOGIN,
                            "Raid target",
                            "channel name",
                        )
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
                    sub_action_ids::CHANNEL_CANCEL_RAID,
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
                sub_action_ids::CHANNEL_SEND_SHOUTOUT,
                BTreeMap::new(),
                vec![
                    text_field(config_keys::TO_BROADCASTER_LOGIN, "Channel", "channel name")
                        .required(),
                ],
            ),
            quick_action(
                "Run commercial",
                "clock",
                QuickActionAccent::Info,
                connected && !tier_locked,
                locked_reason.clone(),
                "Raids & ads",
                false,
                sub_action_ids::CHANNEL_RUN_AD,
                BTreeMap::new(),
                vec![
                    choice_field_hint(
                        AD_LENGTH.key,
                        "Duration",
                        QUICK_AD_LENGTH.wire_name(),
                        &choice_options(AdLength::label),
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
                sub_action_ids::CHANNEL_SNOOZE_AD,
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
                sub_action_ids::CHANNEL_POINTS_FULFILL_REDEMPTION,
                config([
                    (config_keys::REDEMPTION_ID, blank()),
                    (config_keys::REWARD_ID, blank()),
                ]),
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
                sub_action_ids::CHANNEL_POINTS_CANCEL_REDEMPTION,
                config([
                    (config_keys::REDEMPTION_ID, blank()),
                    (config_keys::REWARD_ID, blank()),
                ]),
                Vec::new(),
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::tests::make_bundle_with_tier;
    use crate::chat::ChatConnectionState;

    #[test]
    fn commercial_action_locks_for_standard_tier_and_unlocks_for_affiliate() {
        let standard =
            make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Standard);
        let affiliate =
            make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Affiliate);

        let find_commercial = |bundle: &TwitchIntegrationBundle| {
            bundle
                .actions()
                .into_iter()
                .find(|a| a.subaction_template.kind_id == "twitch.channel.run_ad")
                .unwrap()
        };

        let locked = find_commercial(&standard);
        assert_eq!(locked.locked_reason.as_deref(), Some(TIER_LOCKED_REASON));
        assert!(
            !locked.enabled,
            "commercial must be disabled under standard"
        );

        let unlocked = find_commercial(&affiliate);
        assert!(unlocked.locked_reason.is_none());
        assert!(unlocked.enabled, "commercial must be enabled at affiliate");

        for bundle in [&standard, &affiliate] {
            let open = bundle
                .actions()
                .into_iter()
                .find(|a| a.subaction_template.kind_id == "twitch.chat.send_message")
                .unwrap();
            assert!(open.locked_reason.is_none());
        }
    }

    #[test]
    fn every_poll_and_prediction_action_locks_for_standard_and_unlocks_for_affiliate() {
        let standard =
            make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Standard);
        let affiliate =
            make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Affiliate);
        let is_gated =
            |a: &QuickAction| matches!(a.group.as_deref(), Some("Polls" | "Predictions"));

        let standard_actions = standard.actions();
        let gated: Vec<&QuickAction> = standard_actions.iter().filter(|a| is_gated(a)).collect();
        assert!(
            gated.len() >= 6,
            "expected every Polls and Predictions action, got {}",
            gated.len()
        );
        for a in gated {
            assert_eq!(
                a.locked_reason.as_deref(),
                Some(TIER_LOCKED_REASON),
                "{} must lock under standard tier",
                a.label
            );
            assert!(
                !a.enabled,
                "{} must be disabled under standard tier",
                a.label
            );
        }

        for a in affiliate.actions().iter().filter(|a| is_gated(a)) {
            assert!(
                a.locked_reason.is_none(),
                "{} must unlock at affiliate tier",
                a.label
            );
            assert!(a.enabled, "{} must be enabled at affiliate tier", a.label);
        }
    }

    fn shield_face(action: &QuickAction) -> (&str, &str, QuickActionLiveness) {
        (
            action.label.as_str(),
            action.subaction_template.kind_id.as_str(),
            action.liveness,
        )
    }

    #[test]
    fn the_shield_toggle_offers_to_end_only_an_active_shield() {
        let end = (
            "End Shield mode",
            "twitch.moderation.shield_mode_off",
            QuickActionLiveness::Live,
        );
        let start = (
            "Start Shield mode",
            "twitch.moderation.shield_mode_on",
            QuickActionLiveness::Unknown,
        );
        for (phase, expected) in [
            (ShieldPhase::Active, end),
            (ShieldPhase::Inactive, start),
            (ShieldPhase::Unknown, start),
            (ShieldPhase::Unauthorized, start),
        ] {
            let toggle = shield_mode_toggle(true, phase);
            assert_eq!(shield_face(&toggle), expected, "{phase:?}");
        }
    }

    #[test]
    fn the_shield_toggle_is_usable_only_while_connected_and_authorized() {
        let reason = Some(SHIELD_SCOPE_MISSING_REASON.to_owned());
        for (connected, phase, enabled, locked_reason) in [
            (true, ShieldPhase::Active, true, None),
            (true, ShieldPhase::Inactive, true, None),
            (true, ShieldPhase::Unknown, true, None),
            (true, ShieldPhase::Unauthorized, false, reason.clone()),
            (false, ShieldPhase::Active, false, None),
            (false, ShieldPhase::Inactive, false, None),
            (false, ShieldPhase::Unauthorized, false, reason.clone()),
        ] {
            let toggle = shield_mode_toggle(connected, phase);
            assert_eq!(
                (toggle.enabled, toggle.locked_reason),
                (enabled, locked_reason),
                "connected={connected} {phase:?}"
            );
        }
    }
}
