use std::collections::BTreeMap;

use forge_platform_core::{
    CollectionId, CollectionRevisions, QuickAction, QuickActionAccent, QuickActionField,
    QuickActionGroup, QuickActionLiveness, QuickActions,
};
use forge_registry::{ChoiceField, ConfigChoice, DurationField};
use forge_types::{MessageCode, UserText, Variant};

use super::TwitchIntegrationBundle;
use crate::account::identity::BroadcasterTier;
use crate::collection_keys::REWARDS_COLLECTION;
use crate::lifecycle::{LifecycleSnapshot, ShieldPhase};
use crate::sub_actions::{
    AD_LENGTH, ANNOUNCEMENT_COLOR, AdLength, AnnouncementColor, EMOTE_ONLY, FOLLOWER_MODE_MIN_AGE,
    ModeToggle, POLL_DURATION, PREDICTION_WINDOW, PollEndStatus, SLOW_MODE_WAIT, TIMEOUT_DURATION,
};
use crate::vocabulary::option_lists::{
    ACTIVE_POLL_LIST, LOCKABLE_PREDICTION_LIST, OPEN_PREDICTION_LIST, PREDICTION_OUTCOME_LIST,
    QUEUED_REWARD_LIST, UNFULFILLED_REDEMPTION_LIST,
};
use crate::vocabulary::{self, config_keys, labels, sub_action_ids};

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
    label: fn(T) -> MessageCode,
) -> Vec<(&'static str, MessageCode)> {
    T::ALL
        .iter()
        .map(|choice| (choice.wire_name(), label(*choice)))
        .collect()
}

fn duration_field(field: &DurationField, label: MessageCode, default: i64) -> QuickActionField {
    QuickActionField::int(
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

#[derive(Clone, Copy)]
enum Group {
    StreamInfo,
    Polls,
    Predictions,
    Chat,
    Moderation,
    RaidsAds,
    ChannelPoints,
}

impl Group {
    fn badge(self) -> QuickActionGroup {
        let (label, icon, accent) = match self {
            Self::StreamInfo => (labels::STREAM_INFO, "edit", QuickActionAccent::Info),
            Self::Polls => (labels::POLLS, "chart-bar", QuickActionAccent::Brand),
            Self::Predictions => (
                labels::PREDICTIONS,
                "crystal-ball",
                QuickActionAccent::AccentPinkLight,
            ),
            Self::Chat => (labels::CHAT, "message-2", QuickActionAccent::Brand),
            Self::Moderation => (labels::MODERATION, "shield", QuickActionAccent::Danger),
            Self::RaidsAds => (labels::RAIDS_ADS, "businessplan", QuickActionAccent::Bits),
            Self::ChannelPoints => (
                labels::CHANNEL_POINTS,
                "diamond",
                QuickActionAccent::AccentPinkLight,
            ),
        };
        QuickActionGroup::new(label, icon, accent)
    }
}

fn manage_collection_action(
    label: MessageCode,
    icon: &str,
    accent: QuickActionAccent,
    enabled: bool,
    group: Group,
    collection: CollectionId,
) -> QuickAction {
    QuickAction {
        collection: Some(collection),
        ..QuickAction::new(label, icon, accent, "")
            .in_group(group.badge())
            .enabled_if(enabled)
    }
}

fn active_poll_field() -> QuickActionField {
    QuickActionField::option_list(config_keys::POLL_ID, labels::POLL, ACTIVE_POLL_LIST).required()
}

fn prediction_field(list: &str) -> QuickActionField {
    QuickActionField::option_list(config_keys::PREDICTION_ID, labels::PREDICTION, list).required()
}

fn winning_outcome_field() -> QuickActionField {
    QuickActionField::option_list(
        config_keys::WINNING_OUTCOME_ID,
        labels::WINNING_OUTCOME,
        PREDICTION_OUTCOME_LIST,
    )
    .required()
}

fn redemption_fields() -> Vec<QuickActionField> {
    vec![
        QuickActionField::option_list(config_keys::REWARD_ID, labels::REWARD, QUEUED_REWARD_LIST)
            .required()
            .with_hint(labels::QUEUED_REWARD_HINT),
        QuickActionField::option_list(
            config_keys::REDEMPTION_ID,
            labels::REDEMPTION,
            UNFULFILLED_REDEMPTION_LIST,
        )
        .required(),
    ]
}

fn gated_on(action: QuickAction, liveness: QuickActionLiveness) -> QuickAction {
    QuickAction { liveness, ..action }
}

fn shield_mode_toggle(connected: bool, phase: ShieldPhase) -> QuickAction {
    let (label, icon, kind_id, liveness) = match phase {
        ShieldPhase::Active => (
            labels::END_SHIELD_MODE,
            "shield-off",
            sub_action_ids::MODERATION_SHIELD_MODE_OFF,
            QuickActionLiveness::Live,
        ),
        ShieldPhase::Inactive | ShieldPhase::Unknown | ShieldPhase::Unauthorized => (
            labels::START_SHIELD_MODE,
            "shield",
            sub_action_ids::MODERATION_SHIELD_MODE_ON,
            QuickActionLiveness::Unknown,
        ),
    };
    let unauthorized = phase == ShieldPhase::Unauthorized;
    gated_on(
        QuickAction::new(label, icon, QuickActionAccent::Brand, kind_id)
            .in_group(Group::Moderation.badge())
            .enabled_if(connected && !unauthorized)
            .locked_because(unauthorized.then(|| labels::SHIELD_SCOPE_MISSING.into())),
        liveness,
    )
}

struct Availability {
    connected: bool,
    tier_locked: bool,
    locked_reason: Option<UserText>,
    gated: bool,
    live: LifecycleSnapshot,
    shield_phase: ShieldPhase,
}

fn stream_info_actions(ctx: &Availability) -> Vec<QuickAction> {
    vec![
        QuickAction::new(
            labels::UPDATE_TITLE,
            "edit",
            QuickActionAccent::Info,
            sub_action_ids::CHANNEL_UPDATE_TITLE,
        )
        .in_group(Group::StreamInfo.badge())
        .enabled_if(ctx.connected)
        .with_fields(vec![
            QuickActionField::text(config_keys::TITLE, labels::STREAM_TITLE)
                .with_placeholder(labels::STREAM_TITLE)
                .required(),
        ]),
        QuickAction::new(
            labels::SET_CATEGORY_GAME,
            "device-gamepad-2",
            QuickActionAccent::Brand,
            sub_action_ids::CHANNEL_UPDATE_CATEGORY,
        )
        .in_group(Group::StreamInfo.badge())
        .enabled_if(ctx.connected)
        .with_fields(vec![
            QuickActionField::text(config_keys::CATEGORY_NAME, labels::CATEGORY)
                .with_placeholder(labels::CATEGORY_EXAMPLE)
                .required(),
        ]),
        QuickAction::new(
            labels::UPDATE_TAGS,
            "tags",
            QuickActionAccent::Success,
            sub_action_ids::CHANNEL_UPDATE_TAGS,
        )
        .in_group(Group::StreamInfo.badge())
        .enabled_if(ctx.connected)
        .with_fields(vec![
            QuickActionField::text(config_keys::TAGS, labels::TAGS_COMMA_SEP)
                .with_placeholder(labels::TAGS_EXAMPLE),
        ]),
        QuickAction::new(
            labels::CREATE_STREAM_MARKER,
            "bookmark",
            QuickActionAccent::Bits,
            sub_action_ids::CHANNEL_CREATE_MARKER,
        )
        .in_group(Group::StreamInfo.badge())
        .enabled_if(ctx.connected)
        .with_fields(vec![
            QuickActionField::text(config_keys::DESCRIPTION, labels::MARKER_NOTE)
                .with_placeholder(labels::OPTIONAL),
        ]),
    ]
}

fn poll_actions(ctx: &Availability) -> Vec<QuickAction> {
    vec![
        gated_on(
            QuickAction::new(
                labels::START_POLL,
                "chart-bar",
                QuickActionAccent::Brand,
                sub_action_ids::POLL_START,
            )
            .in_group(Group::Polls.badge())
            .enabled_if(ctx.gated)
            .locked_because(ctx.locked_reason.clone())
            .with_fields(vec![
                QuickActionField::text(config_keys::TITLE, labels::QUESTION)
                    .with_placeholder(labels::POLL_QUESTION)
                    .required(),
                QuickActionField::multiline(config_keys::CHOICES, labels::CHOICES_ONE_PER_LINE)
                    .with_placeholder(labels::ONE_CHOICE_PER_LINE)
                    .required(),
                QuickActionField::toggle(
                    config_keys::CHANNEL_POINTS_VOTING_ENABLED,
                    labels::CHANNEL_POINTS_VOTING,
                    false,
                ),
                duration_field(
                    &POLL_DURATION,
                    labels::DURATION_SECONDS,
                    POLL_DURATION.default,
                ),
            ]),
            ctx.live.poll_slot_free(),
        ),
        gated_on(
            QuickAction::new(
                labels::END_POLL_NOW,
                "player-stop",
                QuickActionAccent::Warning,
                sub_action_ids::POLL_END,
            )
            .in_group(Group::Polls.badge())
            .enabled_if(ctx.gated)
            .locked_because(ctx.locked_reason.clone())
            .with_fields(vec![active_poll_field()]),
            ctx.live.poll_in_flight(),
        ),
        gated_on(
            QuickAction::new(
                labels::CANCEL_POLL,
                "x",
                QuickActionAccent::Danger,
                sub_action_ids::POLL_END,
            )
            .in_group(Group::Polls.badge())
            .enabled_if(ctx.gated)
            .locked_because(ctx.locked_reason.clone())
            .destructive()
            .with_config(config([(
                config_keys::STATUS,
                choice(PollEndStatus::Archived),
            )]))
            .with_fields(vec![active_poll_field()]),
            ctx.live.poll_in_flight(),
        ),
    ]
}

fn prediction_actions(ctx: &Availability) -> Vec<QuickAction> {
    vec![
        gated_on(
            QuickAction::new(
                labels::START_PREDICTION,
                "crystal-ball",
                QuickActionAccent::AccentPinkLight,
                sub_action_ids::PREDICTION_START,
            )
            .in_group(Group::Predictions.badge())
            .enabled_if(ctx.gated)
            .locked_because(ctx.locked_reason.clone())
            .with_fields(vec![
                QuickActionField::text(config_keys::TITLE, labels::TITLE)
                    .with_placeholder(labels::PREDICTION_QUESTION)
                    .required(),
                QuickActionField::multiline(config_keys::OUTCOMES, labels::OUTCOMES_ONE_PER_LINE)
                    .with_placeholder(labels::ONE_OUTCOME_PER_LINE)
                    .required(),
                duration_field(
                    &PREDICTION_WINDOW,
                    labels::WINDOW_SECONDS,
                    PREDICTION_WINDOW.default,
                ),
            ]),
            ctx.live.prediction_slot_free(),
        ),
        gated_on(
            QuickAction::new(
                labels::LOCK_PREDICTION,
                "lock",
                QuickActionAccent::Warning,
                sub_action_ids::PREDICTION_LOCK,
            )
            .in_group(Group::Predictions.badge())
            .enabled_if(ctx.gated)
            .locked_because(ctx.locked_reason.clone())
            .with_fields(vec![prediction_field(LOCKABLE_PREDICTION_LIST)]),
            ctx.live.prediction_lockable(),
        ),
        gated_on(
            QuickAction::new(
                labels::RESOLVE_PAY_OUT,
                "trophy",
                QuickActionAccent::Success,
                sub_action_ids::PREDICTION_RESOLVE,
            )
            .in_group(Group::Predictions.badge())
            .enabled_if(ctx.gated)
            .locked_because(ctx.locked_reason.clone())
            .with_fields(vec![
                prediction_field(OPEN_PREDICTION_LIST),
                winning_outcome_field(),
            ]),
            ctx.live.prediction_settleable(),
        ),
        gated_on(
            QuickAction::new(
                labels::CANCEL_REFUND,
                "x",
                QuickActionAccent::Danger,
                sub_action_ids::PREDICTION_CANCEL,
            )
            .in_group(Group::Predictions.badge())
            .enabled_if(ctx.gated)
            .locked_because(ctx.locked_reason.clone())
            .destructive()
            .with_fields(vec![prediction_field(OPEN_PREDICTION_LIST)]),
            ctx.live.prediction_settleable(),
        ),
    ]
}

fn chat_actions(ctx: &Availability) -> Vec<QuickAction> {
    vec![
        QuickAction::new(
            labels::SEND_MESSAGE,
            "send",
            QuickActionAccent::Brand,
            SEND_MESSAGE_KIND_ID,
        )
        .in_group(Group::Chat.badge())
        .enabled_if(ctx.connected)
        .with_config(config([
            (config_keys::MESSAGE, blank()),
            (
                SEND_MESSAGE_TARGET_KEY,
                Variant::String(vocabulary::PLATFORM_ID.to_owned()),
            ),
        ]))
        .with_fields(vec![
            QuickActionField::multiline(config_keys::MESSAGE, labels::MESSAGE)
                .with_placeholder(labels::MESSAGE)
                .required(),
        ]),
        QuickAction::new(
            labels::ANNOUNCEMENT,
            "speakerphone",
            QuickActionAccent::Warning,
            sub_action_ids::CHAT_SEND_ANNOUNCEMENT,
        )
        .in_group(Group::Chat.badge())
        .enabled_if(ctx.connected)
        .with_fields(vec![
            QuickActionField::multiline(config_keys::MESSAGE, labels::ANNOUNCEMENT)
                .with_placeholder(labels::ANNOUNCEMENT_TEXT)
                .required(),
            QuickActionField::static_choice(
                ANNOUNCEMENT_COLOR.key,
                labels::COLOR,
                default_choice(&ANNOUNCEMENT_COLOR),
                &choice_options(AnnouncementColor::label),
            ),
        ]),
        QuickAction::new(
            labels::CLEAR_CHAT,
            "message-off",
            QuickActionAccent::Danger,
            sub_action_ids::CHAT_CLEAR,
        )
        .in_group(Group::Chat.badge())
        .enabled_if(ctx.connected)
        .destructive(),
        QuickAction::new(
            labels::SLOW_MODE,
            "clock-hour-4",
            QuickActionAccent::Info,
            sub_action_ids::CHAT_SET_MODE,
        )
        .in_group(Group::Chat.badge())
        .enabled_if(ctx.connected)
        .with_config(config([(config_keys::SLOW_MODE, choice(ModeToggle::On))]))
        .with_fields(vec![duration_field(
            &SLOW_MODE_WAIT,
            labels::WAIT_TIME_SECONDS,
            QUICK_SLOW_MODE_WAIT_SECONDS,
        )]),
        QuickAction::new(
            labels::FOLLOWERS_ONLY_MODE,
            "user-check",
            QuickActionAccent::Success,
            sub_action_ids::CHAT_SET_MODE,
        )
        .in_group(Group::Chat.badge())
        .enabled_if(ctx.connected)
        .with_config(config([(
            config_keys::FOLLOWER_MODE,
            choice(ModeToggle::On),
        )]))
        .with_fields(vec![duration_field(
            &FOLLOWER_MODE_MIN_AGE,
            labels::MIN_ACCOUNT_AGE,
            QUICK_FOLLOWER_MIN_MINUTES,
        )]),
        QuickAction::new(
            labels::EMOTE_ONLY_MODE,
            "mood-smile",
            QuickActionAccent::Bits,
            sub_action_ids::CHAT_SET_MODE,
        )
        .in_group(Group::Chat.badge())
        .enabled_if(ctx.connected)
        .with_fields(vec![QuickActionField::static_choice(
            EMOTE_ONLY.key,
            labels::EMOTE_ONLY_MODE,
            ModeToggle::On.wire_name(),
            &choice_options(ModeToggle::label),
        )]),
    ]
}

fn moderation_actions(ctx: &Availability) -> Vec<QuickAction> {
    vec![
        QuickAction::new(
            labels::TIMEOUT_USER,
            "clock-pause",
            QuickActionAccent::Warning,
            sub_action_ids::MODERATION_TIMEOUT_USER,
        )
        .in_group(Group::Moderation.badge())
        .enabled_if(ctx.connected)
        .with_fields(vec![
            QuickActionField::text(config_keys::TARGET_USER_LOGIN, labels::USERNAME)
                .with_placeholder(labels::USERNAME_PLACEHOLDER)
                .required(),
            duration_field(
                &TIMEOUT_DURATION,
                labels::DURATION_SECONDS,
                TIMEOUT_DURATION.default,
            ),
            QuickActionField::text(config_keys::REASON, labels::REASON)
                .with_placeholder(labels::OPTIONAL),
        ]),
        QuickAction::new(
            labels::BAN_USER,
            "ban",
            QuickActionAccent::Danger,
            sub_action_ids::MODERATION_BAN_USER,
        )
        .in_group(Group::Moderation.badge())
        .enabled_if(ctx.connected)
        .destructive()
        .with_fields(vec![
            QuickActionField::text(config_keys::TARGET_USER_LOGIN, labels::USERNAME)
                .with_placeholder(labels::USERNAME_PLACEHOLDER)
                .required(),
            QuickActionField::text(config_keys::REASON, labels::REASON)
                .with_placeholder(labels::OPTIONAL),
        ]),
        QuickAction::new(
            labels::UNBAN_USER,
            "lock-open",
            QuickActionAccent::Success,
            sub_action_ids::MODERATION_UNBAN_USER,
        )
        .in_group(Group::Moderation.badge())
        .enabled_if(ctx.connected)
        .with_fields(vec![
            QuickActionField::text(config_keys::TARGET_USER_LOGIN, labels::USERNAME)
                .with_placeholder(labels::USERNAME_PLACEHOLDER)
                .required(),
        ]),
        shield_mode_toggle(ctx.connected, ctx.shield_phase),
        QuickAction::new(
            labels::ADD_VIP,
            "star",
            QuickActionAccent::Warning,
            sub_action_ids::MODERATION_ADD_VIP,
        )
        .in_group(Group::Moderation.badge())
        .enabled_if(ctx.connected)
        .with_fields(vec![
            QuickActionField::text(config_keys::TARGET_USER_LOGIN, labels::USERNAME)
                .with_placeholder(labels::USERNAME_PLACEHOLDER)
                .required(),
        ]),
        QuickAction::new(
            labels::REMOVE_VIP,
            "star",
            QuickActionAccent::Danger,
            sub_action_ids::MODERATION_REMOVE_VIP,
        )
        .in_group(Group::Moderation.badge())
        .enabled_if(ctx.connected)
        .destructive()
        .with_fields(vec![
            QuickActionField::text(config_keys::TARGET_USER_LOGIN, labels::USERNAME)
                .with_placeholder(labels::USERNAME_PLACEHOLDER)
                .required(),
        ]),
        QuickAction::new(
            labels::ADD_MOD,
            "sword",
            QuickActionAccent::Info,
            sub_action_ids::MODERATION_ADD_MODERATOR,
        )
        .in_group(Group::Moderation.badge())
        .enabled_if(ctx.connected)
        .with_fields(vec![
            QuickActionField::text(config_keys::TARGET_USER_LOGIN, labels::USERNAME)
                .with_placeholder(labels::USERNAME_PLACEHOLDER)
                .required(),
        ]),
        QuickAction::new(
            labels::REMOVE_MOD,
            "sword",
            QuickActionAccent::Danger,
            sub_action_ids::MODERATION_REMOVE_MODERATOR,
        )
        .in_group(Group::Moderation.badge())
        .enabled_if(ctx.connected)
        .destructive()
        .with_fields(vec![
            QuickActionField::text(config_keys::TARGET_USER_LOGIN, labels::USERNAME)
                .with_placeholder(labels::USERNAME_PLACEHOLDER)
                .required(),
        ]),
    ]
}

fn raid_and_ad_actions(ctx: &Availability) -> Vec<QuickAction> {
    vec![
        gated_on(
            QuickAction::new(
                labels::START_RAID,
                "flag",
                QuickActionAccent::Bits,
                sub_action_ids::CHANNEL_START_RAID,
            )
            .in_group(Group::RaidsAds.badge())
            .enabled_if(ctx.connected)
            .with_fields(vec![
                QuickActionField::text(config_keys::TO_BROADCASTER_LOGIN, labels::RAID_TARGET)
                    .with_placeholder(labels::CHANNEL_NAME)
                    .required(),
            ]),
            ctx.live.raid_slot_free(),
        ),
        gated_on(
            QuickAction::new(
                labels::CANCEL_RAID,
                "flag-off",
                QuickActionAccent::Danger,
                sub_action_ids::CHANNEL_CANCEL_RAID,
            )
            .in_group(Group::RaidsAds.badge())
            .enabled_if(ctx.connected)
            .destructive(),
            ctx.live.raid_in_flight(),
        ),
        QuickAction::new(
            labels::SEND_SHOUTOUT,
            "speakerphone",
            QuickActionAccent::Warning,
            sub_action_ids::CHANNEL_SEND_SHOUTOUT,
        )
        .in_group(Group::RaidsAds.badge())
        .enabled_if(ctx.connected)
        .with_fields(vec![
            QuickActionField::text(config_keys::TO_BROADCASTER_LOGIN, labels::CHANNEL)
                .with_placeholder(labels::CHANNEL_NAME)
                .required(),
        ]),
        QuickAction::new(
            labels::RUN_COMMERCIAL,
            "clock",
            QuickActionAccent::Info,
            sub_action_ids::CHANNEL_RUN_AD,
        )
        .in_group(Group::RaidsAds.badge())
        .enabled_if(ctx.connected && !ctx.tier_locked)
        .locked_because(ctx.locked_reason.clone())
        .with_fields(vec![
            QuickActionField::static_choice(
                AD_LENGTH.key,
                labels::DURATION,
                QUICK_AD_LENGTH.wire_name(),
                &choice_options(AdLength::label),
            )
            .with_hint(labels::AD_COOLDOWN_HINT)
            .required(),
        ]),
        QuickAction::new(
            labels::SNOOZE_NEXT_AD,
            "player-skip-forward",
            QuickActionAccent::Success,
            sub_action_ids::CHANNEL_SNOOZE_AD,
        )
        .in_group(Group::RaidsAds.badge())
        .enabled_if(ctx.connected),
    ]
}

fn channel_point_actions(ctx: &Availability) -> Vec<QuickAction> {
    vec![
        manage_collection_action(
            labels::MANAGE_REWARDS,
            "adjustments",
            QuickActionAccent::AccentPinkLight,
            ctx.connected,
            Group::ChannelPoints,
            CollectionId::new(REWARDS_COLLECTION),
        ),
        QuickAction::new(
            labels::FULFILL_REDEMPTION,
            "check",
            QuickActionAccent::Success,
            sub_action_ids::CHANNEL_POINTS_FULFILL_REDEMPTION,
        )
        .in_group(Group::ChannelPoints.badge())
        .enabled_if(ctx.connected)
        .with_fields(redemption_fields()),
        QuickAction::new(
            labels::REJECT_REFUND,
            "x",
            QuickActionAccent::Danger,
            sub_action_ids::CHANNEL_POINTS_CANCEL_REDEMPTION,
        )
        .in_group(Group::ChannelPoints.badge())
        .enabled_if(ctx.connected)
        .destructive()
        .with_fields(redemption_fields()),
    ]
}

impl QuickActions for TwitchIntegrationBundle {
    fn revisions(&self) -> Option<CollectionRevisions> {
        Some(self.lifecycle.quick_action_revisions())
    }

    fn actions(&self) -> Vec<QuickAction> {
        let tier_locked = self.tier() == BroadcasterTier::Standard;
        let connected = self.is_chat_connected();
        let ctx = Availability {
            connected,
            tier_locked,
            locked_reason: tier_locked.then(|| labels::TIER_LOCKED.into()),
            gated: connected && !tier_locked,
            live: self.lifecycle.snapshot(),
            shield_phase: self.lifecycle.shield_phase(),
        };
        [
            stream_info_actions(&ctx),
            poll_actions(&ctx),
            prediction_actions(&ctx),
            chat_actions(&ctx),
            moderation_actions(&ctx),
            raid_and_ad_actions(&ctx),
            channel_point_actions(&ctx),
        ]
        .concat()
    }
}

#[cfg(test)]
mod tests {
    use forge_platform_core::{
        BuiltinOptionSource, OptionListId, QuickActionChoiceSource, QuickActionFieldKind,
    };

    use forge_types::UserText;

    use super::*;
    use crate::builtin::tests::make_bundle_with_tier;
    use crate::chat::ChatSessionState;

    #[test]
    fn commercial_action_locks_for_standard_tier_and_unlocks_for_affiliate() {
        let standard =
            make_bundle_with_tier(ChatSessionState::Connected, BroadcasterTier::Standard);
        let affiliate =
            make_bundle_with_tier(ChatSessionState::Connected, BroadcasterTier::Affiliate);

        let find_commercial = |bundle: &TwitchIntegrationBundle| {
            bundle
                .actions()
                .into_iter()
                .find(|a| a.subaction_template.kind_id == "twitch.channel.run_ad")
                .unwrap()
        };

        let locked = find_commercial(&standard);
        assert_eq!(locked.locked_reason, Some(labels::TIER_LOCKED.into()));
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
            make_bundle_with_tier(ChatSessionState::Connected, BroadcasterTier::Standard);
        let affiliate =
            make_bundle_with_tier(ChatSessionState::Connected, BroadcasterTier::Affiliate);
        let is_gated = |a: &QuickAction| {
            a.group == Some(Group::Polls.badge()) || a.group == Some(Group::Predictions.badge())
        };

        let standard_actions = standard.actions();
        let gated: Vec<&QuickAction> = standard_actions.iter().filter(|a| is_gated(a)).collect();
        for kind in [
            sub_action_ids::POLL_START,
            sub_action_ids::POLL_END,
            sub_action_ids::PREDICTION_START,
            sub_action_ids::PREDICTION_LOCK,
            sub_action_ids::PREDICTION_RESOLVE,
            sub_action_ids::PREDICTION_CANCEL,
        ] {
            assert!(
                gated.iter().any(|a| a.subaction_template.kind_id == kind),
                "{kind} must sit in a gated group"
            );
        }
        for a in gated {
            assert_eq!(
                a.locked_reason,
                Some(labels::TIER_LOCKED.into()),
                "{} must lock under standard tier",
                a.label.code_or_text()
            );
            assert!(
                !a.enabled,
                "{} must be disabled under standard tier",
                a.label.code_or_text()
            );
        }

        for a in affiliate.actions().iter().filter(|a| is_gated(a)) {
            assert!(
                a.locked_reason.is_none(),
                "{} must unlock at affiliate tier",
                a.label.code_or_text()
            );
            assert!(
                a.enabled,
                "{} must be enabled at affiliate tier",
                a.label.code_or_text()
            );
        }
    }

    fn option_list_of(field: &QuickActionField) -> Option<&OptionListId> {
        match &field.kind {
            QuickActionFieldKind::Choice(QuickActionChoiceSource::OptionList(list)) => Some(list),
            _ => None,
        }
    }

    #[test]
    fn every_action_on_a_live_item_picks_it_from_a_list_the_source_declares() {
        let bundle = make_bundle_with_tier(ChatSessionState::Connected, BroadcasterTier::Affiliate);
        let declared: Vec<OptionListId> = bundle
            .option_lists()
            .into_iter()
            .map(|list| list.id)
            .collect();
        let redemption_pick = [
            (config_keys::REWARD_ID, QUEUED_REWARD_LIST),
            (config_keys::REDEMPTION_ID, UNFULFILLED_REDEMPTION_LIST),
        ];
        let settle_pick = [
            (config_keys::PREDICTION_ID, OPEN_PREDICTION_LIST),
            (config_keys::WINNING_OUTCOME_ID, PREDICTION_OUTCOME_LIST),
        ];
        let actions = bundle.actions();
        for (kind, picks) in [
            (
                sub_action_ids::POLL_END,
                &[(config_keys::POLL_ID, ACTIVE_POLL_LIST)][..],
            ),
            (
                sub_action_ids::PREDICTION_LOCK,
                &[(config_keys::PREDICTION_ID, LOCKABLE_PREDICTION_LIST)][..],
            ),
            (sub_action_ids::PREDICTION_RESOLVE, &settle_pick[..]),
            (
                sub_action_ids::PREDICTION_CANCEL,
                &[(config_keys::PREDICTION_ID, OPEN_PREDICTION_LIST)][..],
            ),
            (
                sub_action_ids::CHANNEL_POINTS_FULFILL_REDEMPTION,
                &redemption_pick[..],
            ),
            (
                sub_action_ids::CHANNEL_POINTS_CANCEL_REDEMPTION,
                &redemption_pick[..],
            ),
        ] {
            let matching: Vec<&QuickAction> = actions
                .iter()
                .filter(|action| action.subaction_template.kind_id == kind)
                .collect();
            assert!(!matching.is_empty(), "no quick action runs {kind}");
            for action in matching {
                let offered: Vec<(&str, &str)> = action
                    .fields
                    .iter()
                    .filter_map(|field| {
                        option_list_of(field).map(|list| (field.key.as_str(), list.as_str()))
                    })
                    .collect();
                assert_eq!(offered, picks, "{kind}");
                assert!(
                    offered
                        .iter()
                        .all(|(_, list)| declared.contains(&OptionListId::new(*list))),
                    "{kind}: {offered:?}"
                );
            }
        }
    }

    #[test]
    fn a_dependent_live_item_field_follows_the_field_it_depends_on() {
        let bundle = make_bundle_with_tier(ChatSessionState::Connected, BroadcasterTier::Affiliate);
        let lists = bundle.option_lists();

        let mut followers = std::collections::BTreeSet::new();
        for action in bundle.actions() {
            for (position, field) in action.fields.iter().enumerate() {
                let Some(list) = option_list_of(field) else {
                    continue;
                };
                let metadata = lists.iter().find(|candidate| &candidate.id == list);
                for parent in metadata.into_iter().flat_map(|found| &found.depends_on) {
                    let parent_position = action
                        .fields
                        .iter()
                        .position(|candidate| &candidate.key == parent);
                    assert!(
                        parent_position.is_some_and(|found| found < position),
                        "{}: {} needs {parent} before it",
                        action.label.code_or_text(),
                        field.key
                    );
                    followers.insert(field.key.clone());
                }
            }
        }
        assert!(
            [config_keys::WINNING_OUTCOME_ID, config_keys::REDEMPTION_ID]
                .iter()
                .all(|key| followers.contains(*key)),
            "expected the settle and refund pairs, got {followers:?}"
        );
    }

    fn shield_face(action: &QuickAction) -> (UserText, &str, QuickActionLiveness) {
        (
            action.label.clone(),
            action.subaction_template.kind_id.as_str(),
            action.liveness,
        )
    }

    #[test]
    fn the_shield_toggle_offers_to_end_only_an_active_shield() {
        let end = (
            UserText::from(labels::END_SHIELD_MODE),
            "twitch.moderation.shield_mode_off",
            QuickActionLiveness::Live,
        );
        let start = (
            UserText::from(labels::START_SHIELD_MODE),
            "twitch.moderation.shield_mode_on",
            QuickActionLiveness::Unknown,
        );
        for (phase, expected) in [
            (ShieldPhase::Active, end),
            (ShieldPhase::Inactive, start.clone()),
            (ShieldPhase::Unknown, start.clone()),
            (ShieldPhase::Unauthorized, start),
        ] {
            let toggle = shield_mode_toggle(true, phase);
            assert_eq!(shield_face(&toggle), expected.clone(), "{phase:?}");
        }
    }

    #[test]
    fn the_shield_toggle_is_usable_only_while_connected_and_authorized() {
        let reason = Some(UserText::from(labels::SHIELD_SCOPE_MISSING));
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
