use std::collections::BTreeMap;

use forge_platform_core::{ConnectionState, QuickAction, QuickActions, SectionIcon};
use forge_types::{SubActionStep, Variant};

use crate::client::DiscordClient;

impl QuickActions for DiscordClient {
    fn actions(&self) -> Vec<QuickAction> {
        let snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
        let has_webhooks = !snap.webhook_names.is_empty();
        let first_webhook = snap.webhook_names.first().cloned().unwrap_or_default();
        drop(snap);

        let enabled = has_webhooks && self.connection_state() != ConnectionState::Disconnected;

        vec![
            QuickAction {
                label: "Post Text".to_owned(),
                icon: SectionIcon::new("message"),
                enabled,
                subaction_template: SubActionStep {
                    kind_id: "discord.webhook.send_message".to_owned(),
                    config: BTreeMap::from([
                        (
                            "webhook_name".to_owned(),
                            Variant::String(first_webhook.clone()),
                        ),
                        ("content".to_owned(), Variant::String(String::new())),
                    ]),
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
                picker: None,
            },
            QuickAction {
                label: "Post Embed".to_owned(),
                icon: SectionIcon::new("layout-cards"),
                enabled,
                subaction_template: SubActionStep {
                    kind_id: "discord.webhook.send_embed".to_owned(),
                    config: BTreeMap::from([
                        (
                            "webhook_name".to_owned(),
                            Variant::String(first_webhook.clone()),
                        ),
                        ("embed_title".to_owned(), Variant::String(String::new())),
                        (
                            "embed_description".to_owned(),
                            Variant::String(String::new()),
                        ),
                    ]),
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
                picker: None,
            },
            QuickAction {
                label: "Edit Message".to_owned(),
                icon: SectionIcon::new("pencil"),
                enabled,
                subaction_template: SubActionStep {
                    kind_id: "discord.webhook.update_message".to_owned(),
                    config: BTreeMap::from([
                        (
                            "webhook_name".to_owned(),
                            Variant::String(first_webhook.clone()),
                        ),
                        ("message_id".to_owned(), Variant::String(String::new())),
                        ("content".to_owned(), Variant::String(String::new())),
                    ]),
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
                picker: None,
            },
            QuickAction {
                label: "Test Webhook".to_owned(),
                icon: SectionIcon::new("send"),
                enabled,
                subaction_template: SubActionStep {
                    kind_id: "discord.webhook.send_message".to_owned(),
                    config: BTreeMap::from([
                        ("webhook_name".to_owned(), Variant::String(first_webhook)),
                        (
                            "content".to_owned(),
                            Variant::String("forge test post".to_owned()),
                        ),
                    ]),
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
                picker: None,
            },
        ]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
