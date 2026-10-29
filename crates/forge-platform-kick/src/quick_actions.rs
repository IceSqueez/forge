use std::collections::BTreeMap;

use forge_platform_core::{
    BuiltinStatus, QuickAction, QuickActionAccent, QuickActionField, QuickActionFieldKind,
    QuickActionFieldValue, QuickActionLiveness, QuickActions, SectionIcon,
};
use forge_types::SubActionStep;

use crate::builtin::KickIntegrationBundle;
use crate::vocabulary::variables::references;
use crate::vocabulary::{config_keys, sub_action_ids};

const STREAM_INFO: &str = "Stream info";
const CHAT: &str = "Chat";
const MODERATION: &str = "Moderation";
const REWARDS: &str = "Rewards";
const LOOKUPS: &str = "Lookups";

fn group_badge(group: &str) -> (SectionIcon, QuickActionAccent) {
    match group {
        STREAM_INFO => (SectionIcon::new("edit"), QuickActionAccent::Info),
        CHAT => (SectionIcon::new("message-2"), QuickActionAccent::Brand),
        MODERATION => (SectionIcon::new("shield"), QuickActionAccent::Danger),
        REWARDS => (SectionIcon::new("gift"), QuickActionAccent::Bits),
        LOOKUPS => (SectionIcon::new("search"), QuickActionAccent::Info),
        _ => (SectionIcon::new("dot"), QuickActionAccent::Brand),
    }
}

fn text_field(key: &str, label: &str, placeholder: &str) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.to_owned(),
        kind: QuickActionFieldKind::Text,
        default: None,
        placeholder: Some(placeholder.to_owned()),
        hint: None,
        required: false,
    }
}

fn text_field_hint(key: &str, label: &str, hint: &str) -> QuickActionField {
    QuickActionField {
        placeholder: None,
        hint: Some(hint.to_owned()),
        ..text_field(key, label, "")
    }
}

fn multiline_field(key: &str, label: &str, placeholder: &str) -> QuickActionField {
    QuickActionField {
        kind: QuickActionFieldKind::Multiline,
        ..text_field(key, label, placeholder)
    }
}

fn toggle_field(key: &str, label: &str, default: bool) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.to_owned(),
        kind: QuickActionFieldKind::Toggle,
        default: Some(QuickActionFieldValue::Toggle(default)),
        placeholder: None,
        hint: None,
        required: false,
    }
}

fn quick_action(
    label: &str,
    icon: &str,
    accent: QuickActionAccent,
    group: &str,
    kind_id: &str,
    fields: Vec<QuickActionField>,
) -> QuickAction {
    let (group_icon, group_accent) = group_badge(group);
    QuickAction {
        label: label.to_owned(),
        icon: SectionIcon::new(icon),
        enabled: false,
        locked_reason: None,
        liveness: QuickActionLiveness::Unknown,
        group: Some(group.to_owned()),
        group_icon: Some(group_icon),
        group_accent: Some(group_accent),
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
            "Update title",
            "edit",
            QuickActionAccent::Info,
            STREAM_INFO,
            sub_action_ids::CHANNEL_UPDATE_INFO,
            vec![text_field(config_keys::TITLE, "Stream Title", "Stream title").required()],
        ),
        quick_action(
            "Set category",
            "category",
            QuickActionAccent::Success,
            STREAM_INFO,
            sub_action_ids::CHANNEL_UPDATE_INFO,
            vec![
                text_field_hint(
                    config_keys::CATEGORY_ID,
                    "Category ID",
                    "Use Find category (Lookups) to get the numeric id",
                )
                .required(),
            ],
        ),
        quick_action(
            "Set custom tags",
            "hash",
            QuickActionAccent::Brand,
            STREAM_INFO,
            sub_action_ids::CHANNEL_UPDATE_INFO,
            vec![
                text_field(
                    config_keys::TAGS,
                    "Tags (comma-separated, max 10)",
                    "gaming, chill, speedrun",
                )
                .required(),
            ],
        ),
        quick_action(
            "Send message",
            "send",
            QuickActionAccent::Brand,
            CHAT,
            sub_action_ids::CHAT_SEND_MESSAGE,
            vec![
                multiline_field(config_keys::MESSAGE, "Message", "Message").required(),
                toggle_field(config_keys::AS_BOT, "Send as bot", false),
            ],
        ),
        destructive(quick_action(
            "Delete message",
            "trash",
            QuickActionAccent::Danger,
            CHAT,
            sub_action_ids::CHAT_DELETE_MESSAGE,
            vec![
                text_field(
                    config_keys::MESSAGE_ID,
                    "Message ID",
                    references::MESSAGE_ID,
                )
                .required(),
            ],
        )),
        quick_action(
            "Timeout user",
            "clock-pause",
            QuickActionAccent::Warning,
            MODERATION,
            sub_action_ids::MODERATION_TIMEOUT,
            vec![
                text_field(config_keys::USER_ID, "Target User ID", references::USER_ID).required(),
            ],
        ),
        destructive(quick_action(
            "Ban user",
            "ban",
            QuickActionAccent::Danger,
            MODERATION,
            sub_action_ids::MODERATION_BAN,
            vec![
                text_field(config_keys::USER_ID, "Target User ID", references::USER_ID).required(),
            ],
        )),
        quick_action(
            "Unban user",
            "unban",
            QuickActionAccent::Success,
            MODERATION,
            sub_action_ids::MODERATION_UNBAN,
            vec![
                text_field(config_keys::USER_ID, "Target User ID", references::USER_ID).required(),
            ],
        ),
        quick_action(
            "Create reward",
            "gift",
            QuickActionAccent::Success,
            REWARDS,
            sub_action_ids::REWARD_CREATE,
            vec![
                text_field(config_keys::TITLE, "Reward Title", "e.g. Hydrate").required(),
                text_field(config_keys::COST, "Cost (channel points)", "e.g. 500").required(),
            ],
        ),
        quick_action(
            "Update reward",
            "edit",
            QuickActionAccent::Info,
            REWARDS,
            sub_action_ids::REWARD_UPDATE,
            vec![
                text_field(config_keys::REWARD_ID, "Reward ID", references::REWARD_ID).required(),
                text_field(
                    config_keys::TITLE,
                    "New Title (optional)",
                    "Leave empty to keep current",
                ),
                text_field(
                    config_keys::COST,
                    "New Cost (optional)",
                    "Leave empty to keep current",
                ),
            ],
        ),
        destructive(quick_action(
            "Delete reward",
            "trash",
            QuickActionAccent::Danger,
            REWARDS,
            sub_action_ids::REWARD_DELETE,
            vec![text_field(config_keys::REWARD_ID, "Reward ID", references::REWARD_ID).required()],
        )),
        quick_action(
            "Accept redemption",
            "check",
            QuickActionAccent::Success,
            REWARDS,
            sub_action_ids::REWARD_REDEMPTION_ACCEPT,
            vec![
                text_field(
                    config_keys::REDEMPTION_IDS,
                    "Redemption ID(s)",
                    references::REDEMPTION_IDS_HINT,
                )
                .required(),
            ],
        ),
        quick_action(
            "Reject redemption",
            "x-circle",
            QuickActionAccent::Warning,
            REWARDS,
            sub_action_ids::REWARD_REDEMPTION_REJECT,
            vec![
                text_field(
                    config_keys::REDEMPTION_IDS,
                    "Redemption ID(s)",
                    references::REDEMPTION_IDS_HINT,
                )
                .required(),
            ],
        ),
        quick_action(
            "Lookup user",
            "user",
            QuickActionAccent::Success,
            LOOKUPS,
            sub_action_ids::LOOKUP_USER,
            vec![text_field(config_keys::SLUG, "Channel Slug", "channel-slug").required()],
        ),
        quick_action(
            "Stream stats",
            "chart-line",
            QuickActionAccent::Info,
            LOOKUPS,
            sub_action_ids::LOOKUP_STREAM_STATS,
            Vec::new(),
        ),
        quick_action(
            "Find category",
            "search",
            QuickActionAccent::Brand,
            LOOKUPS,
            sub_action_ids::LOOKUP_CATEGORY,
            vec![text_field(config_keys::QUERY, "Search Query", "just chatting").required()],
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
