use forge_platform_core::{
    BuiltinStatus, QuickAction, QuickActionAccent, QuickActionField, QuickActionGroup, QuickActions,
};
use forge_types::{MessageCode, VerbatimText};

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
        QuickActionGroup::new(label, icon, accent)
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
    QuickAction::new(label, icon, accent, kind_id)
        .in_group(group.badge())
        .with_fields(fields)
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
                QuickActionField::text(config_keys::TITLE, labels::STREAM_TITLE_FIELD)
                    .with_placeholder(labels::STREAM_TITLE)
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
                QuickActionField::text(config_keys::CATEGORY_ID, labels::CATEGORY_ID)
                    .with_hint(labels::CATEGORY_ID_HINT)
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
                QuickActionField::text(config_keys::TAGS, labels::TAGS_COMMA_SEPARATED)
                    .with_placeholder(labels::TAGS_PLACEHOLDER)
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
                QuickActionField::multiline(config_keys::MESSAGE, labels::MESSAGE)
                    .with_placeholder(labels::MESSAGE)
                    .required(),
                QuickActionField::toggle(config_keys::AS_BOT, labels::SEND_AS_BOT, false),
            ],
        ),
        quick_action(
            labels::DELETE_MESSAGE,
            "trash",
            QuickActionAccent::Danger,
            Group::Chat,
            sub_action_ids::CHAT_DELETE_MESSAGE,
            vec![
                QuickActionField::text(config_keys::MESSAGE_ID, labels::MESSAGE_ID_FIELD)
                    .with_placeholder(VerbatimText::new(references::MESSAGE_ID))
                    .required(),
            ],
        )
        .destructive(),
        quick_action(
            labels::TIMEOUT_USER,
            "clock-pause",
            QuickActionAccent::Warning,
            Group::Moderation,
            sub_action_ids::MODERATION_TIMEOUT,
            vec![
                QuickActionField::text(config_keys::USER_ID, labels::TARGET_USER_ID)
                    .with_placeholder(VerbatimText::new(references::USER_ID))
                    .required(),
            ],
        ),
        quick_action(
            labels::BAN_USER,
            "ban",
            QuickActionAccent::Danger,
            Group::Moderation,
            sub_action_ids::MODERATION_BAN,
            vec![
                QuickActionField::text(config_keys::USER_ID, labels::TARGET_USER_ID)
                    .with_placeholder(VerbatimText::new(references::USER_ID))
                    .required(),
            ],
        )
        .destructive(),
        quick_action(
            labels::UNBAN_USER,
            "unban",
            QuickActionAccent::Success,
            Group::Moderation,
            sub_action_ids::MODERATION_UNBAN,
            vec![
                QuickActionField::text(config_keys::USER_ID, labels::TARGET_USER_ID)
                    .with_placeholder(VerbatimText::new(references::USER_ID))
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
                QuickActionField::text(config_keys::TITLE, labels::REWARD_TITLE)
                    .with_placeholder(labels::REWARD_TITLE_EXAMPLE)
                    .required(),
                QuickActionField::text(config_keys::COST, labels::COST_CHANNEL_POINTS)
                    .with_placeholder(labels::COST_EXAMPLE)
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
                QuickActionField::text(config_keys::REWARD_ID, labels::REWARD_ID)
                    .with_placeholder(VerbatimText::new(references::REWARD_ID))
                    .required(),
                QuickActionField::text(config_keys::TITLE, labels::NEW_TITLE_OPTIONAL)
                    .with_placeholder(labels::LEAVE_EMPTY_TO_KEEP_CURRENT),
                QuickActionField::text(config_keys::COST, labels::NEW_COST_OPTIONAL)
                    .with_placeholder(labels::LEAVE_EMPTY_TO_KEEP_CURRENT),
            ],
        ),
        quick_action(
            labels::DELETE_REWARD,
            "trash",
            QuickActionAccent::Danger,
            Group::Rewards,
            sub_action_ids::REWARD_DELETE,
            vec![
                QuickActionField::text(config_keys::REWARD_ID, labels::REWARD_ID)
                    .with_placeholder(VerbatimText::new(references::REWARD_ID))
                    .required(),
            ],
        )
        .destructive(),
        quick_action(
            labels::ACCEPT_REDEMPTION,
            "check",
            QuickActionAccent::Success,
            Group::Rewards,
            sub_action_ids::REWARD_REDEMPTION_ACCEPT,
            vec![
                QuickActionField::text(config_keys::REDEMPTION_IDS, labels::REDEMPTION_ID_S)
                    .with_placeholder(labels::REDEMPTION_IDS_HINT)
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
                QuickActionField::text(config_keys::REDEMPTION_IDS, labels::REDEMPTION_ID_S)
                    .with_placeholder(labels::REDEMPTION_IDS_HINT)
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
                QuickActionField::text(config_keys::SLUG, labels::CHANNEL_SLUG)
                    .with_placeholder(labels::CHANNEL_SLUG_PLACEHOLDER)
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
                QuickActionField::text(config_keys::QUERY, labels::SEARCH_QUERY)
                    .with_placeholder(labels::CATEGORY_SEARCH_PLACEHOLDER)
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
