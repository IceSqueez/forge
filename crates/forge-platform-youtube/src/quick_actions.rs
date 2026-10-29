use std::collections::BTreeMap;

use forge_platform_core::{
    BuiltinStatus, QuickAction, QuickActionAccent, QuickActionChoiceOption,
    QuickActionChoiceSource, QuickActionField, QuickActionFieldKind, QuickActionFieldValue,
    QuickActionLiveness, QuickActions, SectionIcon,
};
use forge_registry::ConfigChoice;
use forge_types::SubActionStep;

use crate::builtin::YoutubeIntegrationBundle;
use crate::sub_actions::PrivacyStatus;
use crate::vocabulary::{config_keys, sub_action_ids};

const GAMING_CATEGORY_ID: &str = "20";
const BROADCAST: &str = "Broadcast";
const POLLS: &str = "Polls";
const CHAT: &str = "Chat";
const CONTENT: &str = "Content";
const CHANNEL_ID_PLACEHOLDER: &str = "Channel ID (UC...)";

fn group_badge(group: &str) -> (SectionIcon, QuickActionAccent) {
    match group {
        BROADCAST => (SectionIcon::new("broadcast"), QuickActionAccent::Danger),
        POLLS => (SectionIcon::new("chart-bar"), QuickActionAccent::Brand),
        CHAT => (SectionIcon::new("message-2"), QuickActionAccent::Brand),
        CONTENT => (SectionIcon::new("video"), QuickActionAccent::Bits),
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

fn multiline_field(key: &str, label: &str, placeholder: &str) -> QuickActionField {
    QuickActionField {
        kind: QuickActionFieldKind::Multiline,
        ..text_field(key, label, placeholder)
    }
}

fn privacy_field() -> QuickActionField {
    let options = PrivacyStatus::ALL
        .iter()
        .map(|status| QuickActionChoiceOption {
            value: status.wire_name().to_owned(),
            label: status.label().to_owned(),
        })
        .collect();
    QuickActionField {
        kind: QuickActionFieldKind::Choice(QuickActionChoiceSource::Static(options)),
        default: Some(QuickActionFieldValue::Text(
            PrivacyStatus::Public.wire_name().to_owned(),
        )),
        placeholder: None,
        ..text_field(config_keys::PRIVACY_STATUS, "Visibility", "")
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
            BROADCAST,
            sub_action_ids::STREAM_UPDATE_TITLE,
            vec![text_field(config_keys::TITLE, "Title", "New stream title").required()],
        ),
        quick_action(
            "Update description",
            "file-text",
            QuickActionAccent::Brand,
            BROADCAST,
            sub_action_ids::STREAM_UPDATE_DESCRIPTION,
            vec![
                multiline_field(
                    config_keys::DESCRIPTION,
                    "Description",
                    "Stream description",
                )
                .required(),
            ],
        ),
        quick_action(
            "Set category",
            "category",
            QuickActionAccent::Success,
            BROADCAST,
            sub_action_ids::STREAM_UPDATE_CATEGORY,
            vec![
                QuickActionField {
                    default: Some(QuickActionFieldValue::Text(GAMING_CATEGORY_ID.to_owned())),
                    ..text_field(
                        config_keys::CATEGORY_ID,
                        "Category ID",
                        "20 (Gaming), 24 (Entertainment)...",
                    )
                }
                .required(),
            ],
        ),
        quick_action(
            "Change privacy",
            "eye",
            QuickActionAccent::Warning,
            BROADCAST,
            sub_action_ids::STREAM_UPDATE_PRIVACY,
            vec![privacy_field().required()],
        ),
        quick_action(
            "Set thumbnail",
            "photo",
            QuickActionAccent::Bits,
            BROADCAST,
            sub_action_ids::STREAM_SET_THUMBNAIL,
            vec![
                text_field(
                    config_keys::IMAGE_PATH,
                    "Image path",
                    "/full/path/to/thumbnail.png",
                )
                .required(),
            ],
        ),
        quick_action(
            "Insert ad break (cuepoint)",
            "player-skip-forward",
            QuickActionAccent::Info,
            BROADCAST,
            sub_action_ids::STREAM_INSERT_AD_BREAK,
            Vec::new(),
        ),
        quick_action(
            "Create live poll",
            "chart-bar",
            QuickActionAccent::Brand,
            POLLS,
            sub_action_ids::CHAT_CREATE_POLL,
            vec![
                text_field(config_keys::QUESTION, "Question", "Poll question").required(),
                multiline_field(
                    config_keys::OPTIONS,
                    "Choices (one per line)",
                    "One choice per line",
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
            vec![multiline_field(config_keys::MESSAGE, "Message", "Message").required()],
        ),
        destructive(quick_action(
            "Delete message",
            "trash",
            QuickActionAccent::Danger,
            CHAT,
            sub_action_ids::CHAT_DELETE_MESSAGE,
            vec![text_field(config_keys::MESSAGE_ID, "Message ID", "message id").required()],
        )),
        quick_action(
            "Timeout user",
            "clock-pause",
            QuickActionAccent::Warning,
            CHAT,
            sub_action_ids::MODERATION_TIMEOUT_USER,
            vec![text_field(config_keys::CHANNEL_ID, "User", CHANNEL_ID_PLACEHOLDER).required()],
        ),
        destructive(quick_action(
            "Ban user",
            "ban",
            QuickActionAccent::Danger,
            CHAT,
            sub_action_ids::MODERATION_BAN_USER,
            vec![text_field(config_keys::CHANNEL_ID, "User", CHANNEL_ID_PLACEHOLDER).required()],
        )),
        quick_action(
            "Lookup viewer",
            "user",
            QuickActionAccent::Success,
            CONTENT,
            sub_action_ids::LOOKUP_VIEWER,
            vec![text_field(config_keys::IDENTIFIER, "Username", "@handle or UC...").required()],
        ),
        quick_action(
            "Stream stats",
            "chart-line",
            QuickActionAccent::Info,
            CONTENT,
            sub_action_ids::LOOKUP_STREAM_STATS,
            Vec::new(),
        ),
    ]
}

impl QuickActions for YoutubeIntegrationBundle {
    fn actions(&self) -> Vec<QuickAction> {
        let enabled = self.connection().is_connected();
        catalog()
            .into_iter()
            .map(|action| QuickAction { enabled, ..action })
            .collect()
    }
}
