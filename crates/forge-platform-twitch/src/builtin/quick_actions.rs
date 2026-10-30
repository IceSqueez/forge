use std::collections::BTreeMap;

use forge_platform_core::{
    CollectionId, CollectionRevisions, QuickAction, QuickActionAccent, QuickActionField,
    QuickActionGroup, QuickActionLiveness, QuickActions, SectionIcon,
};
use forge_registry::{ChoiceField, ConfigChoice, DurationField};
use forge_types::{MessageCode, SubActionStep, UserText, Variant};

use super::TwitchIntegrationBundle;
use crate::account::identity::BroadcasterTier;
use crate::collection_keys::REWARDS_COLLECTION;
use crate::lifecycle::{LifecycleSnapshot, ShieldPhase};
use crate::quick_action_fields::{
    choice_field, choice_field_hint, int_field, live_item_field, multiline_field, text_field,
    toggle_field,
};
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
        QuickActionGroup {
            label: label.into(),
            icon: SectionIcon::new(icon),
            accent,
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "each argument is one field of the quick-action template"
)]
fn quick_action(
    label: MessageCode,
    icon: &str,
    accent: QuickActionAccent,
    enabled: bool,
    locked_reason: Option<UserText>,
    group: Group,
    destructive: bool,
    kind_id: &str,
    config: BTreeMap<String, Variant>,
    fields: Vec<QuickActionField>,
) -> QuickAction {
    QuickAction {
        label: label.into(),
        icon: SectionIcon::new(icon),
        enabled,
        locked_reason,
        liveness: QuickActionLiveness::Unknown,
        group: Some(group.badge()),
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
    label: MessageCode,
    icon: &str,
    accent: QuickActionAccent,
    enabled: bool,
    group: Group,
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

fn active_poll_field() -> QuickActionField {
    live_item_field(config_keys::POLL_ID, labels::POLL, ACTIVE_POLL_LIST)
}

fn prediction_field(list: &str) -> QuickActionField {
    live_item_field(config_keys::PREDICTION_ID, labels::PREDICTION, list)
}

fn winning_outcome_field() -> QuickActionField {
    live_item_field(
        config_keys::WINNING_OUTCOME_ID,
        labels::WINNING_OUTCOME,
        PREDICTION_OUTCOME_LIST,
    )
}

fn redemption_fields() -> Vec<QuickActionField> {
    vec![
        QuickActionField {
            hint: Some(labels::QUEUED_REWARD_HINT.into()),
            ..live_item_field(config_keys::REWARD_ID, labels::REWARD, QUEUED_REWARD_LIST)
        },
        live_item_field(
            config_keys::REDEMPTION_ID,
            labels::REDEMPTION,
            UNFULFILLED_REDEMPTION_LIST,
        ),
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
        quick_action(
            label,
            icon,
            QuickActionAccent::Brand,
            connected && !unauthorized,
            unauthorized.then(|| labels::SHIELD_SCOPE_MISSING.into()),
            Group::Moderation,
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
        let locked_reason = tier_locked.then(|| labels::TIER_LOCKED.into());
        let gated = connected && !tier_locked;
        let live: LifecycleSnapshot = self.lifecycle.snapshot();

        vec![
            quick_action(
                labels::UPDATE_TITLE,
                "edit",
                QuickActionAccent::Info,
                connected,
                None,
                Group::StreamInfo,
                false,
                sub_action_ids::CHANNEL_UPDATE_TITLE,
                BTreeMap::new(),
                vec![
                    text_field(
                        config_keys::TITLE,
                        labels::STREAM_TITLE,
                        labels::STREAM_TITLE,
                    )
                    .required(),
                ],
            ),
            quick_action(
                labels::SET_CATEGORY_GAME,
                "device-gamepad-2",
                QuickActionAccent::Brand,
                connected,
                None,
                Group::StreamInfo,
                false,
                sub_action_ids::CHANNEL_UPDATE_CATEGORY,
                BTreeMap::new(),
                vec![
                    text_field(
                        config_keys::CATEGORY_NAME,
                        labels::CATEGORY,
                        labels::CATEGORY_EXAMPLE,
                    )
                    .required(),
                ],
            ),
            quick_action(
                labels::UPDATE_TAGS,
                "tags",
                QuickActionAccent::Success,
                connected,
                None,
                Group::StreamInfo,
                false,
                sub_action_ids::CHANNEL_UPDATE_TAGS,
                BTreeMap::new(),
                vec![text_field(
                    config_keys::TAGS,
                    labels::TAGS_COMMA_SEP,
                    labels::TAGS_EXAMPLE,
                )],
            ),
            quick_action(
                labels::CREATE_STREAM_MARKER,
                "bookmark",
                QuickActionAccent::Bits,
                connected,
                None,
                Group::StreamInfo,
                false,
                sub_action_ids::CHANNEL_CREATE_MARKER,
                BTreeMap::new(),
                vec![text_field(
                    config_keys::DESCRIPTION,
                    labels::MARKER_NOTE,
                    labels::OPTIONAL,
                )],
            ),
            gated_on(
                quick_action(
                    labels::START_POLL,
                    "chart-bar",
                    QuickActionAccent::Brand,
                    gated,
                    locked_reason.clone(),
                    Group::Polls,
                    false,
                    sub_action_ids::POLL_START,
                    BTreeMap::new(),
                    vec![
                        text_field(config_keys::TITLE, labels::QUESTION, labels::POLL_QUESTION)
                            .required(),
                        multiline_field(
                            config_keys::CHOICES,
                            labels::CHOICES_ONE_PER_LINE,
                            labels::ONE_CHOICE_PER_LINE,
                        )
                        .required(),
                        toggle_field(
                            config_keys::CHANNEL_POINTS_VOTING_ENABLED,
                            labels::CHANNEL_POINTS_VOTING,
                            false,
                        ),
                        duration_field(
                            &POLL_DURATION,
                            labels::DURATION_SECONDS,
                            POLL_DURATION.default,
                        ),
                    ],
                ),
                live.poll_slot_free(),
            ),
            gated_on(
                quick_action(
                    labels::END_POLL_NOW,
                    "player-stop",
                    QuickActionAccent::Warning,
                    gated,
                    locked_reason.clone(),
                    Group::Polls,
                    false,
                    sub_action_ids::POLL_END,
                    BTreeMap::new(),
                    vec![active_poll_field()],
                ),
                live.poll_in_flight(),
            ),
            gated_on(
                quick_action(
                    labels::CANCEL_POLL,
                    "x",
                    QuickActionAccent::Danger,
                    gated,
                    locked_reason.clone(),
                    Group::Polls,
                    true,
                    sub_action_ids::POLL_END,
                    config([(config_keys::STATUS, choice(PollEndStatus::Archived))]),
                    vec![active_poll_field()],
                ),
                live.poll_in_flight(),
            ),
            gated_on(
                quick_action(
                    labels::START_PREDICTION,
                    "crystal-ball",
                    QuickActionAccent::AccentPinkLight,
                    gated,
                    locked_reason.clone(),
                    Group::Predictions,
                    false,
                    sub_action_ids::PREDICTION_START,
                    BTreeMap::new(),
                    vec![
                        text_field(
                            config_keys::TITLE,
                            labels::TITLE,
                            labels::PREDICTION_QUESTION,
                        )
                        .required(),
                        multiline_field(
                            config_keys::OUTCOMES,
                            labels::OUTCOMES_ONE_PER_LINE,
                            labels::ONE_OUTCOME_PER_LINE,
                        )
                        .required(),
                        duration_field(
                            &PREDICTION_WINDOW,
                            labels::WINDOW_SECONDS,
                            PREDICTION_WINDOW.default,
                        ),
                    ],
                ),
                live.prediction_slot_free(),
            ),
            gated_on(
                quick_action(
                    labels::LOCK_PREDICTION,
                    "lock",
                    QuickActionAccent::Warning,
                    gated,
                    locked_reason.clone(),
                    Group::Predictions,
                    false,
                    sub_action_ids::PREDICTION_LOCK,
                    BTreeMap::new(),
                    vec![prediction_field(LOCKABLE_PREDICTION_LIST)],
                ),
                live.prediction_lockable(),
            ),
            gated_on(
                quick_action(
                    labels::RESOLVE_PAY_OUT,
                    "trophy",
                    QuickActionAccent::Success,
                    gated,
                    locked_reason.clone(),
                    Group::Predictions,
                    false,
                    sub_action_ids::PREDICTION_RESOLVE,
                    BTreeMap::new(),
                    vec![
                        prediction_field(OPEN_PREDICTION_LIST),
                        winning_outcome_field(),
                    ],
                ),
                live.prediction_settleable(),
            ),
            gated_on(
                quick_action(
                    labels::CANCEL_REFUND,
                    "x",
                    QuickActionAccent::Danger,
                    gated,
                    locked_reason.clone(),
                    Group::Predictions,
                    true,
                    sub_action_ids::PREDICTION_CANCEL,
                    BTreeMap::new(),
                    vec![prediction_field(OPEN_PREDICTION_LIST)],
                ),
                live.prediction_settleable(),
            ),
            quick_action(
                labels::SEND_MESSAGE,
                "send",
                QuickActionAccent::Brand,
                connected,
                None,
                Group::Chat,
                false,
                SEND_MESSAGE_KIND_ID,
                config([
                    (config_keys::MESSAGE, blank()),
                    (
                        SEND_MESSAGE_TARGET_KEY,
                        Variant::String(vocabulary::PLATFORM_ID.to_owned()),
                    ),
                ]),
                vec![
                    multiline_field(config_keys::MESSAGE, labels::MESSAGE, labels::MESSAGE)
                        .required(),
                ],
            ),
            quick_action(
                labels::ANNOUNCEMENT,
                "speakerphone",
                QuickActionAccent::Warning,
                connected,
                None,
                Group::Chat,
                false,
                sub_action_ids::CHAT_SEND_ANNOUNCEMENT,
                BTreeMap::new(),
                vec![
                    multiline_field(
                        config_keys::MESSAGE,
                        labels::ANNOUNCEMENT,
                        labels::ANNOUNCEMENT_TEXT,
                    )
                    .required(),
                    choice_field(
                        ANNOUNCEMENT_COLOR.key,
                        labels::COLOR,
                        default_choice(&ANNOUNCEMENT_COLOR),
                        &choice_options(AnnouncementColor::label),
                    ),
                ],
            ),
            quick_action(
                labels::CLEAR_CHAT,
                "message-off",
                QuickActionAccent::Danger,
                connected,
                None,
                Group::Chat,
                true,
                sub_action_ids::CHAT_CLEAR,
                BTreeMap::new(),
                Vec::new(),
            ),
            quick_action(
                labels::SLOW_MODE,
                "clock-hour-4",
                QuickActionAccent::Info,
                connected,
                None,
                Group::Chat,
                false,
                sub_action_ids::CHAT_SET_MODE,
                config([(config_keys::SLOW_MODE, choice(ModeToggle::On))]),
                vec![duration_field(
                    &SLOW_MODE_WAIT,
                    labels::WAIT_TIME_SECONDS,
                    QUICK_SLOW_MODE_WAIT_SECONDS,
                )],
            ),
            quick_action(
                labels::FOLLOWERS_ONLY_MODE,
                "user-check",
                QuickActionAccent::Success,
                connected,
                None,
                Group::Chat,
                false,
                sub_action_ids::CHAT_SET_MODE,
                config([(config_keys::FOLLOWER_MODE, choice(ModeToggle::On))]),
                vec![duration_field(
                    &FOLLOWER_MODE_MIN_AGE,
                    labels::MIN_ACCOUNT_AGE,
                    QUICK_FOLLOWER_MIN_MINUTES,
                )],
            ),
            quick_action(
                labels::EMOTE_ONLY_MODE,
                "mood-smile",
                QuickActionAccent::Bits,
                connected,
                None,
                Group::Chat,
                false,
                sub_action_ids::CHAT_SET_MODE,
                BTreeMap::new(),
                vec![choice_field(
                    EMOTE_ONLY.key,
                    labels::EMOTE_ONLY_MODE,
                    ModeToggle::On.wire_name(),
                    &choice_options(ModeToggle::label),
                )],
            ),
            quick_action(
                labels::TIMEOUT_USER,
                "clock-pause",
                QuickActionAccent::Warning,
                connected,
                None,
                Group::Moderation,
                false,
                sub_action_ids::MODERATION_TIMEOUT_USER,
                BTreeMap::new(),
                vec![
                    text_field(
                        config_keys::TARGET_USER_LOGIN,
                        labels::USERNAME,
                        labels::USERNAME_PLACEHOLDER,
                    )
                    .required(),
                    duration_field(
                        &TIMEOUT_DURATION,
                        labels::DURATION_SECONDS,
                        TIMEOUT_DURATION.default,
                    ),
                    text_field(config_keys::REASON, labels::REASON, labels::OPTIONAL),
                ],
            ),
            quick_action(
                labels::BAN_USER,
                "ban",
                QuickActionAccent::Danger,
                connected,
                None,
                Group::Moderation,
                true,
                sub_action_ids::MODERATION_BAN_USER,
                BTreeMap::new(),
                vec![
                    text_field(
                        config_keys::TARGET_USER_LOGIN,
                        labels::USERNAME,
                        labels::USERNAME_PLACEHOLDER,
                    )
                    .required(),
                    text_field(config_keys::REASON, labels::REASON, labels::OPTIONAL),
                ],
            ),
            quick_action(
                labels::UNBAN_USER,
                "lock-open",
                QuickActionAccent::Success,
                connected,
                None,
                Group::Moderation,
                false,
                sub_action_ids::MODERATION_UNBAN_USER,
                BTreeMap::new(),
                vec![
                    text_field(
                        config_keys::TARGET_USER_LOGIN,
                        labels::USERNAME,
                        labels::USERNAME_PLACEHOLDER,
                    )
                    .required(),
                ],
            ),
            shield_mode_toggle(connected, self.lifecycle.shield_phase()),
            quick_action(
                labels::ADD_VIP,
                "star",
                QuickActionAccent::Warning,
                connected,
                None,
                Group::Moderation,
                false,
                sub_action_ids::MODERATION_ADD_VIP,
                BTreeMap::new(),
                vec![
                    text_field(
                        config_keys::TARGET_USER_LOGIN,
                        labels::USERNAME,
                        labels::USERNAME_PLACEHOLDER,
                    )
                    .required(),
                ],
            ),
            quick_action(
                labels::REMOVE_VIP,
                "star",
                QuickActionAccent::Danger,
                connected,
                None,
                Group::Moderation,
                true,
                sub_action_ids::MODERATION_REMOVE_VIP,
                BTreeMap::new(),
                vec![
                    text_field(
                        config_keys::TARGET_USER_LOGIN,
                        labels::USERNAME,
                        labels::USERNAME_PLACEHOLDER,
                    )
                    .required(),
                ],
            ),
            quick_action(
                labels::ADD_MOD,
                "sword",
                QuickActionAccent::Info,
                connected,
                None,
                Group::Moderation,
                false,
                sub_action_ids::MODERATION_ADD_MODERATOR,
                BTreeMap::new(),
                vec![
                    text_field(
                        config_keys::TARGET_USER_LOGIN,
                        labels::USERNAME,
                        labels::USERNAME_PLACEHOLDER,
                    )
                    .required(),
                ],
            ),
            quick_action(
                labels::REMOVE_MOD,
                "sword",
                QuickActionAccent::Danger,
                connected,
                None,
                Group::Moderation,
                true,
                sub_action_ids::MODERATION_REMOVE_MODERATOR,
                BTreeMap::new(),
                vec![
                    text_field(
                        config_keys::TARGET_USER_LOGIN,
                        labels::USERNAME,
                        labels::USERNAME_PLACEHOLDER,
                    )
                    .required(),
                ],
            ),
            gated_on(
                quick_action(
                    labels::START_RAID,
                    "flag",
                    QuickActionAccent::Bits,
                    connected,
                    None,
                    Group::RaidsAds,
                    false,
                    sub_action_ids::CHANNEL_START_RAID,
                    BTreeMap::new(),
                    vec![
                        text_field(
                            config_keys::TO_BROADCASTER_LOGIN,
                            labels::RAID_TARGET,
                            labels::CHANNEL_NAME,
                        )
                        .required(),
                    ],
                ),
                live.raid_slot_free(),
            ),
            gated_on(
                quick_action(
                    labels::CANCEL_RAID,
                    "flag-off",
                    QuickActionAccent::Danger,
                    connected,
                    None,
                    Group::RaidsAds,
                    true,
                    sub_action_ids::CHANNEL_CANCEL_RAID,
                    BTreeMap::new(),
                    Vec::new(),
                ),
                live.raid_in_flight(),
            ),
            quick_action(
                labels::SEND_SHOUTOUT,
                "speakerphone",
                QuickActionAccent::Warning,
                connected,
                None,
                Group::RaidsAds,
                false,
                sub_action_ids::CHANNEL_SEND_SHOUTOUT,
                BTreeMap::new(),
                vec![
                    text_field(
                        config_keys::TO_BROADCASTER_LOGIN,
                        labels::CHANNEL,
                        labels::CHANNEL_NAME,
                    )
                    .required(),
                ],
            ),
            quick_action(
                labels::RUN_COMMERCIAL,
                "clock",
                QuickActionAccent::Info,
                connected && !tier_locked,
                locked_reason.clone(),
                Group::RaidsAds,
                false,
                sub_action_ids::CHANNEL_RUN_AD,
                BTreeMap::new(),
                vec![
                    choice_field_hint(
                        AD_LENGTH.key,
                        labels::DURATION,
                        QUICK_AD_LENGTH.wire_name(),
                        &choice_options(AdLength::label),
                        labels::AD_COOLDOWN_HINT,
                    )
                    .required(),
                ],
            ),
            quick_action(
                labels::SNOOZE_NEXT_AD,
                "player-skip-forward",
                QuickActionAccent::Success,
                connected,
                None,
                Group::RaidsAds,
                false,
                sub_action_ids::CHANNEL_SNOOZE_AD,
                BTreeMap::new(),
                Vec::new(),
            ),
            manage_collection_action(
                labels::MANAGE_REWARDS,
                "adjustments",
                QuickActionAccent::AccentPinkLight,
                connected,
                Group::ChannelPoints,
                CollectionId::new(REWARDS_COLLECTION),
            ),
            quick_action(
                labels::FULFILL_REDEMPTION,
                "check",
                QuickActionAccent::Success,
                connected,
                None,
                Group::ChannelPoints,
                false,
                sub_action_ids::CHANNEL_POINTS_FULFILL_REDEMPTION,
                BTreeMap::new(),
                redemption_fields(),
            ),
            quick_action(
                labels::REJECT_REFUND,
                "x",
                QuickActionAccent::Danger,
                connected,
                None,
                Group::ChannelPoints,
                true,
                sub_action_ids::CHANNEL_POINTS_CANCEL_REDEMPTION,
                BTreeMap::new(),
                redemption_fields(),
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use forge_platform_core::{
        BuiltinOptionSource, OptionListId, QuickActionChoiceSource, QuickActionFieldKind,
    };

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
            make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Standard);
        let affiliate =
            make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Affiliate);
        let is_gated = |a: &QuickAction| {
            a.group == Some(Group::Polls.badge()) || a.group == Some(Group::Predictions.badge())
        };

        let standard_actions = standard.actions();
        let gated: Vec<&QuickAction> = standard_actions.iter().filter(|a| is_gated(a)).collect();
        assert!(
            gated.len() >= 6,
            "expected every Polls and Predictions action, got {}",
            gated.len()
        );
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
        let bundle =
            make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Affiliate);
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
        let bundle =
            make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Affiliate);
        let lists = bundle.option_lists();

        let mut checked = 0;
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
                    checked += 1;
                }
            }
        }
        assert!(
            checked >= 3,
            "expected the settle and refund pairs, got {checked}"
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
