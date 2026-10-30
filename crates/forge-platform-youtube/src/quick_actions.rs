use std::collections::BTreeMap;

use forge_platform_core::{
    BuiltinStatus, QuickAction, QuickActionAccent, QuickActionChoiceOption,
    QuickActionChoiceSource, QuickActionField, QuickActionFieldKind, QuickActionFieldValue,
    QuickActionGroup, QuickActionLiveness, QuickActions, SectionIcon,
};
use forge_registry::ConfigChoice;
use forge_types::{MessageCode, SubActionStep, UserText, VerbatimText};

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

fn privacy_field() -> QuickActionField {
    let options = PrivacyStatus::ALL
        .iter()
        .map(|status| QuickActionChoiceOption {
            value: status.wire_name().to_owned(),
            label: status.label().into(),
        })
        .collect();
    QuickActionField {
        kind: QuickActionFieldKind::Choice(QuickActionChoiceSource::Static(options)),
        default: Some(QuickActionFieldValue::Text(
            PrivacyStatus::Public.wire_name().to_owned(),
        )),
        ..bare_text_field(config_keys::PRIVACY_STATUS, labels::VISIBILITY)
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
            Group::Broadcast,
            sub_action_ids::STREAM_UPDATE_TITLE,
            vec![
                text_field(config_keys::TITLE, labels::TITLE, labels::NEW_STREAM_TITLE).required(),
            ],
        ),
        quick_action(
            labels::UPDATE_DESCRIPTION,
            "file-text",
            QuickActionAccent::Brand,
            Group::Broadcast,
            sub_action_ids::STREAM_UPDATE_DESCRIPTION,
            vec![
                multiline_field(
                    config_keys::DESCRIPTION,
                    labels::DESCRIPTION,
                    labels::STREAM_DESCRIPTION,
                )
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
                QuickActionField {
                    default: Some(QuickActionFieldValue::Text(GAMING_CATEGORY_ID.to_owned())),
                    ..text_field(
                        config_keys::CATEGORY_ID,
                        labels::CATEGORY_ID,
                        labels::CATEGORY_ID_EXAMPLES,
                    )
                }
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
                text_field(
                    config_keys::IMAGE_PATH,
                    labels::IMAGE_PATH,
                    VerbatimText::new("/full/path/to/thumbnail.png"),
                )
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
                text_field(
                    config_keys::QUESTION,
                    labels::QUESTION,
                    labels::POLL_QUESTION,
                )
                .required(),
                multiline_field(
                    config_keys::OPTIONS,
                    labels::CHOICES_ONE_PER_LINE,
                    labels::ONE_CHOICE_PER_LINE,
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
                    labels::MESSAGE_ID_HINT,
                )
                .required(),
            ],
        )),
        quick_action(
            labels::TIMEOUT_USER,
            "clock-pause",
            QuickActionAccent::Warning,
            Group::Chat,
            sub_action_ids::MODERATION_TIMEOUT_USER,
            vec![
                text_field(
                    config_keys::CHANNEL_ID,
                    labels::USER,
                    labels::CHANNEL_ID_PLACEHOLDER,
                )
                .required(),
            ],
        ),
        destructive(quick_action(
            labels::BAN_USER,
            "ban",
            QuickActionAccent::Danger,
            Group::Chat,
            sub_action_ids::MODERATION_BAN_USER,
            vec![
                text_field(
                    config_keys::CHANNEL_ID,
                    labels::USER,
                    labels::CHANNEL_ID_PLACEHOLDER,
                )
                .required(),
            ],
        )),
        quick_action(
            labels::LOOKUP_VIEWER,
            "user",
            QuickActionAccent::Success,
            Group::Content,
            sub_action_ids::LOOKUP_VIEWER,
            vec![
                text_field(
                    config_keys::IDENTIFIER,
                    labels::USERNAME,
                    labels::VIEWER_IDENTIFIER_EXAMPLE,
                )
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
