use forge_platform_core::{
    ConnectionState, QuickAction, QuickActionAccent, QuickActionLiveness, QuickActions, SectionIcon,
};
use forge_registry::SubActionConfig;
use forge_types::{MessageCode, SubActionStep, Variant};

use crate::client::DiscordClient;
use crate::vocabulary::{config_keys, labels, sub_action_ids};

const TEST_POST_CONTENT: &str = "forge test post";

fn webhook_action(
    label: MessageCode,
    icon: &str,
    enabled: bool,
    kind_id: &str,
    config: SubActionConfig,
) -> QuickAction {
    QuickAction {
        label: label.into(),
        icon: SectionIcon::new(icon),
        enabled,
        locked_reason: None,
        liveness: QuickActionLiveness::Unknown,
        group: None,
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
                labels::POST_TEXT,
                "message",
                enabled,
                sub_action_ids::WEBHOOK_SEND_MESSAGE,
                preset(Vec::new()),
            ),
            webhook_action(
                labels::POST_EMBED,
                "layout-cards",
                enabled,
                sub_action_ids::WEBHOOK_SEND_EMBED,
                preset(Vec::new()),
            ),
            webhook_action(
                labels::EDIT_MESSAGE,
                "pencil",
                enabled,
                sub_action_ids::WEBHOOK_UPDATE_MESSAGE,
                preset(Vec::new()),
            ),
            webhook_action(
                labels::TEST_WEBHOOK,
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
    use forge_types::UserText;

    #[test]
    fn all_actions_disabled_when_no_webhooks() {
        let c = DiscordClient::new_for_test();
        let actions = c.actions();
        for action in &actions {
            assert!(
                !action.enabled,
                "{} must be disabled with no webhooks",
                action.label.code_or_text()
            );
        }
    }

    #[test]
    fn actions_enabled_after_webhook_registered() {
        let c = DiscordClient::new_for_test();
        {
            let mut snap = c.content_state.lock().unwrap();
            snap.webhook_names.push("alerts".to_owned());
        }
        let actions = c.actions();
        for action in &actions {
            assert!(
                action.enabled,
                "{} must be enabled with webhooks",
                action.label.code_or_text()
            );
        }
    }

    #[test]
    fn test_webhook_template_is_a_post_ready_to_send_to_the_first_webhook() {
        use forge_registry::SubActionRunner;

        let c = DiscordClient::new_for_test();
        {
            let mut snap = c.content_state.lock().unwrap();
            snap.webhook_names.push("alerts".to_owned());
        }
        let actions = c.actions();
        let template = &actions
            .iter()
            .find(|a| a.label == UserText::from(labels::TEST_WEBHOOK))
            .unwrap()
            .subaction_template;
        let runner = crate::runners::PostTextRunner::new(c.clone());

        assert_eq!(template.kind_id, runner.id());
        assert_eq!(
            template.config.get("webhook_name"),
            Some(&Variant::String("alerts".to_owned()))
        );
        assert!(runner.validate_config(&template.config).is_ok());
    }

    #[test]
    fn every_code_shown_with_and_without_webhooks_is_catalogued_in_both_languages() {
        use crate::emitted_codes::{emitted_codes, uncatalogued};
        use crate::vocabulary::messages::DISCORD_MESSAGES;
        use forge_platform_core::BuiltinContent;

        let bare = DiscordClient::new_for_test();
        let configured = DiscordClient::new_for_test();
        configured
            .content_state
            .lock()
            .unwrap()
            .webhook_names
            .push("alerts".to_owned());
        for client in [bare, configured] {
            let emitted = emitted_codes(&serde_json::json!((client.actions(), client.sections())));

            assert!(!emitted.is_empty());
            assert_eq!(
                uncatalogued(&emitted, &[DISCORD_MESSAGES, forge_types::COMMON_MESSAGES]),
                Vec::<String>::new()
            );
        }
    }
}
