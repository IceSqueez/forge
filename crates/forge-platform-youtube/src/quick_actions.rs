use forge_platform_core::{
    BuiltinStatus, QuickAction, QuickActionAccent, QuickActionField, QuickActionGroup, QuickActions,
};
use forge_registry::ConfigChoice;
use forge_types::{MessageCode, VerbatimText};

use crate::builtin::YoutubeIntegrationBundle;
use crate::sub_actions::PrivacyStatus;
use crate::vocabulary::{config_keys, labels, sub_action_ids};

const GAMING_CATEGORY_ID: &str = "20";

#[derive(Clone, Copy)]
enum Group {
    Broadcast,
    Polls,
    Chat,
    Content,
}

impl Group {
    fn badge(self) -> QuickActionGroup {
        let (label, icon, accent) = match self {
            Self::Broadcast => (labels::BROADCAST, "broadcast", QuickActionAccent::Danger),
            Self::Polls => (labels::POLLS, "chart-bar", QuickActionAccent::Brand),
            Self::Chat => (labels::CHAT, "message-2", QuickActionAccent::Brand),
            Self::Content => (labels::CONTENT, "video", QuickActionAccent::Bits),
        };
        QuickActionGroup::new(label, icon, accent)
    }
}

fn privacy_field() -> QuickActionField {
    let options: Vec<(&str, MessageCode)> = PrivacyStatus::ALL
        .iter()
        .map(|status| (status.wire_name(), status.label()))
        .collect();
    QuickActionField::static_choice(
        config_keys::PRIVACY_STATUS,
        labels::VISIBILITY,
        PrivacyStatus::Public.wire_name(),
        &options,
    )
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
            Group::Broadcast,
            sub_action_ids::STREAM_UPDATE_TITLE,
            vec![
                QuickActionField::text(config_keys::TITLE, labels::TITLE)
                    .with_placeholder(labels::NEW_STREAM_TITLE)
                    .required(),
            ],
        ),
        quick_action(
            labels::UPDATE_DESCRIPTION,
            "file-text",
            QuickActionAccent::Brand,
            Group::Broadcast,
            sub_action_ids::STREAM_UPDATE_DESCRIPTION,
            vec![
                QuickActionField::multiline(config_keys::DESCRIPTION, labels::DESCRIPTION)
                    .with_placeholder(labels::STREAM_DESCRIPTION)
                    .required(),
            ],
        ),
        quick_action(
            labels::SET_CATEGORY,
            "category",
            QuickActionAccent::Success,
            Group::Broadcast,
            sub_action_ids::STREAM_UPDATE_CATEGORY,
            vec![
                QuickActionField::text(config_keys::CATEGORY_ID, labels::CATEGORY_ID)
                    .with_default_text(GAMING_CATEGORY_ID)
                    .with_placeholder(labels::CATEGORY_ID_EXAMPLES)
                    .required(),
            ],
        ),
        quick_action(
            labels::CHANGE_PRIVACY,
            "eye",
            QuickActionAccent::Warning,
            Group::Broadcast,
            sub_action_ids::STREAM_UPDATE_PRIVACY,
            vec![privacy_field().required()],
        ),
        quick_action(
            labels::SET_THUMBNAIL,
            "photo",
            QuickActionAccent::Bits,
            Group::Broadcast,
            sub_action_ids::STREAM_SET_THUMBNAIL,
            vec![
                QuickActionField::text(config_keys::IMAGE_PATH, labels::IMAGE_PATH)
                    .with_placeholder(VerbatimText::new("/full/path/to/thumbnail.png"))
                    .required(),
            ],
        ),
        quick_action(
            labels::INSERT_AD_BREAK,
            "player-skip-forward",
            QuickActionAccent::Info,
            Group::Broadcast,
            sub_action_ids::STREAM_INSERT_AD_BREAK,
            Vec::new(),
        ),
        quick_action(
            labels::CREATE_LIVE_POLL,
            "chart-bar",
            QuickActionAccent::Brand,
            Group::Polls,
            sub_action_ids::CHAT_CREATE_POLL,
            vec![
                QuickActionField::text(config_keys::QUESTION, labels::QUESTION)
                    .with_placeholder(labels::POLL_QUESTION)
                    .required(),
                QuickActionField::multiline(config_keys::OPTIONS, labels::CHOICES_ONE_PER_LINE)
                    .with_placeholder(labels::ONE_CHOICE_PER_LINE)
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
                    .with_placeholder(labels::MESSAGE_ID_HINT)
                    .required(),
            ],
        )
        .destructive(),
        quick_action(
            labels::TIMEOUT_USER,
            "clock-pause",
            QuickActionAccent::Warning,
            Group::Chat,
            sub_action_ids::MODERATION_TIMEOUT_USER,
            vec![
                QuickActionField::text(config_keys::CHANNEL_ID, labels::USER)
                    .with_placeholder(labels::CHANNEL_ID_PLACEHOLDER)
                    .required(),
            ],
        ),
        quick_action(
            labels::BAN_USER,
            "ban",
            QuickActionAccent::Danger,
            Group::Chat,
            sub_action_ids::MODERATION_BAN_USER,
            vec![
                QuickActionField::text(config_keys::CHANNEL_ID, labels::USER)
                    .with_placeholder(labels::CHANNEL_ID_PLACEHOLDER)
                    .required(),
            ],
        )
        .destructive(),
        quick_action(
            labels::LOOKUP_VIEWER,
            "user",
            QuickActionAccent::Success,
            Group::Content,
            sub_action_ids::LOOKUP_VIEWER,
            vec![
                QuickActionField::text(config_keys::IDENTIFIER, labels::USERNAME)
                    .with_placeholder(labels::VIEWER_IDENTIFIER_EXAMPLE)
                    .required(),
            ],
        ),
        quick_action(
            labels::STREAM_STATS,
            "chart-line",
            QuickActionAccent::Info,
            Group::Content,
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
