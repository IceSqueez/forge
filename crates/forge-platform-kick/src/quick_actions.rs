use std::collections::BTreeMap;

use forge_platform_core::{
    BuiltinStatus, QuickAction, QuickActionAccent, QuickActionField, QuickActionFieldKind,
    QuickActionFieldValue, QuickActionGroup, QuickActionLiveness, QuickActions, SectionIcon,
};
use forge_types::{MessageCode, SubActionStep, UserText};

use crate::builtin::KickIntegrationBundle;
use crate::vocabulary::variables::references;
use crate::vocabulary::{config_keys, labels, sub_action_ids};

#[derive(Clone, Copy)]
enum Group {
    StreamInfo,
    Chat,
    Moderation,
    Rewards,
    Lookups,
}

impl Group {
    fn badge(self) -> QuickActionGroup {
        let (label, icon, accent) = match self {
            Self::StreamInfo => (labels::STREAM_INFO, "edit", QuickActionAccent::Info),
            Self::Chat => (labels::CHAT, "message-2", QuickActionAccent::Brand),
            Self::Moderation => (labels::MODERATION, "shield", QuickActionAccent::Danger),
            Self::Rewards => (labels::REWARDS, "gift", QuickActionAccent::Bits),
            Self::Lookups => (labels::LOOKUPS, "search", QuickActionAccent::Info),
        };
        QuickActionGroup {
            label: label.into(),
            icon: SectionIcon::new(icon),
            accent,
        }
    }
}

fn bare_text_field(key: &str, label: MessageCode) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.into(),
        kind: QuickActionFieldKind::Text,
        default: None,
        placeholder: None,
        hint: None,
        required: false,
    }
}

fn text_field(key: &str, label: MessageCode, placeholder: impl Into<UserText>) -> QuickActionField {
    QuickActionField {
        placeholder: Some(placeholder.into()),
        ..bare_text_field(key, label)
    }
}

fn text_field_hint(key: &str, label: MessageCode, hint: impl Into<UserText>) -> QuickActionField {
    QuickActionField {
        hint: Some(hint.into()),
        ..bare_text_field(key, label)
    }
}

fn multiline_field(
    key: &str,
    label: MessageCode,
    placeholder: impl Into<UserText>,
) -> QuickActionField {
    QuickActionField {
        kind: QuickActionFieldKind::Multiline,
        ..text_field(key, label, placeholder)
    }
}

fn toggle_field(key: &str, label: MessageCode, default: bool) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.into(),
        kind: QuickActionFieldKind::Toggle,
        default: Some(QuickActionFieldValue::Toggle(default)),
        placeholder: None,
        hint: None,
        required: false,
    }
}

fn quick_action(
    label: MessageCode,
    icon: &str,
    accent: QuickActionAccent,
    group: Group,
    kind_id: &str,
    fields: Vec<QuickActionField>,
) -> QuickAction {
    QuickAction {
        label: label.into(),
        icon: SectionIcon::new(icon),
        enabled: false,
        locked_reason: None,
        liveness: QuickActionLiveness::Unknown,
        group: Some(group.badge()),
        destructive: false,
        accent,
        subaction_template: SubActionStep {
            kind_id: kind_id.to_owned(),
            config: BTreeMap::new(),
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        },
        fields,
        collection: None,
    }
}

fn destructive(action: QuickAction) -> QuickAction {
    QuickAction {
        destructive: true,
        ..action
    }
}

fn catalog() -> Vec<QuickAction> {
    vec![
        quick_action(
            labels::UPDATE_TITLE,
            "edit",
            QuickActionAccent::Info,
            Group::StreamInfo,
            sub_action_ids::CHANNEL_UPDATE_INFO,
            vec![
                text_field(
                    config_keys::TITLE,
                    labels::STREAM_TITLE_FIELD,
                    labels::STREAM_TITLE,
                )
                .required(),
            ],
        ),
        quick_action(
            labels::SET_CATEGORY,
            "category",
            QuickActionAccent::Success,
            Group::StreamInfo,
            sub_action_ids::CHANNEL_UPDATE_INFO,
            vec![
                text_field_hint(
                    config_keys::CATEGORY_ID,
                    labels::CATEGORY_ID,
                    labels::CATEGORY_ID_HINT,
                )
                .required(),
            ],
        ),
        quick_action(
            labels::SET_CUSTOM_TAGS,
            "hash",
            QuickActionAccent::Brand,
            Group::StreamInfo,
            sub_action_ids::CHANNEL_UPDATE_INFO,
            vec![
                text_field(
                    config_keys::TAGS,
                    labels::TAGS_COMMA_SEPARATED,
                    labels::TAGS_PLACEHOLDER,
                )
                .required(),
            ],
        ),
        quick_action(
            labels::SEND_MESSAGE,
            "send",
            QuickActionAccent::Brand,
            Group::Chat,
            sub_action_ids::CHAT_SEND_MESSAGE,
            vec![
                multiline_field(config_keys::MESSAGE, labels::MESSAGE, labels::MESSAGE).required(),
                toggle_field(config_keys::AS_BOT, labels::SEND_AS_BOT, false),
            ],
        ),
        destructive(quick_action(
            labels::DELETE_MESSAGE,
            "trash",
            QuickActionAccent::Danger,
            Group::Chat,
            sub_action_ids::CHAT_DELETE_MESSAGE,
            vec![
                text_field(
                    config_keys::MESSAGE_ID,
                    labels::MESSAGE_ID_FIELD,
                    UserText::unlocalized(references::MESSAGE_ID),
                )
                .required(),
            ],
        )),
        quick_action(
            labels::TIMEOUT_USER,
            "clock-pause",
            QuickActionAccent::Warning,
            Group::Moderation,
            sub_action_ids::MODERATION_TIMEOUT,
            vec![
                text_field(
                    config_keys::USER_ID,
                    labels::TARGET_USER_ID,
                    UserText::unlocalized(references::USER_ID),
                )
                .required(),
            ],
        ),
        destructive(quick_action(
            labels::BAN_USER,
            "ban",
            QuickActionAccent::Danger,
            Group::Moderation,
            sub_action_ids::MODERATION_BAN,
            vec![
                text_field(
                    config_keys::USER_ID,
                    labels::TARGET_USER_ID,
                    UserText::unlocalized(references::USER_ID),
                )
                .required(),
            ],
        )),
        quick_action(
            labels::UNBAN_USER,
            "unban",
            QuickActionAccent::Success,
            Group::Moderation,
            sub_action_ids::MODERATION_UNBAN,
            vec![
                text_field(
                    config_keys::USER_ID,
                    labels::TARGET_USER_ID,
                    UserText::unlocalized(references::USER_ID),
                )
                .required(),
            ],
        ),
        quick_action(
            labels::CREATE_REWARD,
            "gift",
            QuickActionAccent::Success,
            Group::Rewards,
            sub_action_ids::REWARD_CREATE,
            vec![
                text_field(
                    config_keys::TITLE,
                    labels::REWARD_TITLE,
                    labels::REWARD_TITLE_EXAMPLE,
                )
                .required(),
                text_field(
                    config_keys::COST,
                    labels::COST_CHANNEL_POINTS,
                    labels::COST_EXAMPLE,
                )
                .required(),
            ],
        ),
        quick_action(
            labels::UPDATE_REWARD,
            "edit",
            QuickActionAccent::Info,
            Group::Rewards,
            sub_action_ids::REWARD_UPDATE,
            vec![
                text_field(
                    config_keys::REWARD_ID,
                    labels::REWARD_ID,
                    UserText::unlocalized(references::REWARD_ID),
                )
                .required(),
                text_field(
                    config_keys::TITLE,
                    labels::NEW_TITLE_OPTIONAL,
                    labels::LEAVE_EMPTY_TO_KEEP_CURRENT,
                ),
                text_field(
                    config_keys::COST,
                    labels::NEW_COST_OPTIONAL,
                    labels::LEAVE_EMPTY_TO_KEEP_CURRENT,
                ),
            ],
        ),
        destructive(quick_action(
            labels::DELETE_REWARD,
            "trash",
            QuickActionAccent::Danger,
            Group::Rewards,
            sub_action_ids::REWARD_DELETE,
            vec![
                text_field(
                    config_keys::REWARD_ID,
                    labels::REWARD_ID,
                    UserText::unlocalized(references::REWARD_ID),
                )
                .required(),
            ],
        )),
        quick_action(
            labels::ACCEPT_REDEMPTION,
            "check",
            QuickActionAccent::Success,
            Group::Rewards,
            sub_action_ids::REWARD_REDEMPTION_ACCEPT,
            vec![
                text_field(
                    config_keys::REDEMPTION_IDS,
                    labels::REDEMPTION_ID_S,
                    UserText::unlocalized(references::REDEMPTION_IDS_HINT),
                )
                .required(),
            ],
        ),
        quick_action(
            labels::REJECT_REDEMPTION,
            "x-circle",
            QuickActionAccent::Warning,
            Group::Rewards,
            sub_action_ids::REWARD_REDEMPTION_REJECT,
            vec![
                text_field(
                    config_keys::REDEMPTION_IDS,
                    labels::REDEMPTION_ID_S,
                    UserText::unlocalized(references::REDEMPTION_IDS_HINT),
                )
                .required(),
            ],
        ),
        quick_action(
            labels::LOOKUP_USER,
            "user",
            QuickActionAccent::Success,
            Group::Lookups,
            sub_action_ids::LOOKUP_USER,
            vec![
                text_field(
                    config_keys::SLUG,
                    labels::CHANNEL_SLUG,
                    labels::CHANNEL_SLUG_PLACEHOLDER,
                )
                .required(),
            ],
        ),
        quick_action(
            labels::STREAM_STATS,
            "chart-line",
            QuickActionAccent::Info,
            Group::Lookups,
            sub_action_ids::LOOKUP_STREAM_STATS,
            Vec::new(),
        ),
        quick_action(
            labels::FIND_CATEGORY,
            "search",
            QuickActionAccent::Brand,
            Group::Lookups,
            sub_action_ids::LOOKUP_CATEGORY,
            vec![
                text_field(
                    config_keys::QUERY,
                    labels::SEARCH_QUERY,
                    labels::CATEGORY_SEARCH_PLACEHOLDER,
                )
                .required(),
            ],
        ),
    ]
}

impl QuickActions for KickIntegrationBundle {
    fn actions(&self) -> Vec<QuickAction> {
        let enabled = self.connection().is_connected();
        catalog()
            .into_iter()
            .map(|action| QuickAction { enabled, ..action })
            .collect()
    }
}
