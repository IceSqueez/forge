use forge_platform_core::{
    ConnectionState, QuickAction, QuickActionAccent, QuickActionLiveness, QuickActions, SectionIcon,
};
use forge_registry::SubActionConfig;
use forge_types::{SubActionStep, Variant};

use crate::client::DiscordClient;
use crate::vocabulary::{config_keys, sub_action_ids};

const TEST_POST_CONTENT: &str = "forge test post";

fn webhook_action(
    label: &str,
    icon: &str,
    enabled: bool,
    kind_id: &str,
    config: SubActionConfig,
) -> QuickAction {
    QuickAction {
        label: label.to_owned(),
        icon: SectionIcon::new(icon),
        enabled,
        locked_reason: None,
        liveness: QuickActionLiveness::Unknown,
        group: None,
        group_icon: None,
        group_accent: None,
        destructive: false,
        accent: QuickActionAccent::Brand,
        subaction_template: SubActionStep {
            kind_id: kind_id.to_owned(),
            config,
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        },
        fields: Vec::new(),
        collection: None,
    }
}

impl QuickActions for DiscordClient {
    fn actions(&self) -> Vec<QuickAction> {
        let snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
        let first_webhook = snap.webhook_names.first().cloned();
        drop(snap);

        let enabled =
            first_webhook.is_some() && self.connection_state() != ConnectionState::Disconnected;
        let preset = |overrides: Vec<(&str, Variant)>| -> SubActionConfig {
            first_webhook
                .clone()
                .map(|name| (config_keys::WEBHOOK_NAME, Variant::String(name)))
                .into_iter()
                .chain(overrides)
                .map(|(key, value)| (key.to_owned(), value))
                .collect()
        };

        vec![
            webhook_action(
                "Post Text",
                "message",
                enabled,
                sub_action_ids::WEBHOOK_SEND_MESSAGE,
                preset(Vec::new()),
            ),
            webhook_action(
                "Post Embed",
                "layout-cards",
                enabled,
                sub_action_ids::WEBHOOK_SEND_EMBED,
                preset(Vec::new()),
            ),
            webhook_action(
                "Edit Message",
                "pencil",
                enabled,
                sub_action_ids::WEBHOOK_UPDATE_MESSAGE,
                preset(Vec::new()),
            ),
            webhook_action(
                "Test Webhook",
                "send",
                enabled,
                sub_action_ids::WEBHOOK_SEND_MESSAGE,
                preset(vec![(
                    config_keys::CONTENT,
                    Variant::String(TEST_POST_CONTENT.to_owned()),
                )]),
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use forge_platform_core::QuickActions;

    use super::*;
    use crate::client::DiscordClient;
    use crate::content::record_send;

    #[test]
    fn all_actions_disabled_when_no_webhooks() {
        let c = DiscordClient::new_for_test();
        let actions = c.actions();
        for action in &actions {
            assert!(
                !action.enabled,
                "{} must be disabled with no webhooks",
                action.label
            );
        }
    }

    #[test]
    fn actions_enabled_after_webhook_registered() {
        let c = DiscordClient::new_for_test();
        {
            let mut snap = c.content_state.lock().unwrap();
            record_send(&mut snap, "alerts", Some("msg1".to_owned()), false, true);
        }
        let actions = c.actions();
        for action in &actions {
            assert!(
                action.enabled,
                "{} must be enabled with webhooks",
                action.label
            );
        }
    }

    #[test]
    fn test_webhook_prefills_forge_test_post_content() {
        let c = DiscordClient::new_for_test();
        {
            let mut snap = c.content_state.lock().unwrap();
            record_send(&mut snap, "alerts", None, false, true);
        }
        let actions = c.actions();
        let test_action = actions.iter().find(|a| a.label == "Test Webhook").unwrap();
        assert_eq!(
            test_action.subaction_template.kind_id,
            "discord.webhook.send_message"
        );
        assert_eq!(
            test_action.subaction_template.config.get("content"),
            Some(&Variant::String("forge test post".to_owned()))
        );
    }
}
