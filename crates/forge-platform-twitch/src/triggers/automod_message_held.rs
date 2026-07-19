use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

pub(crate) struct AutomodMessageHeldDescriptor;

impl TriggerKindDescriptor for AutomodMessageHeldDescriptor {
    fn id(&self) -> &str {
        "twitch.automod.message_held"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "AutoMod message held"
    }

    fn summary(&self) -> &str {
        "Fires when AutoMod holds a chat message for moderator review"
    }

    fn search_text(&self) -> &str {
        "twitch automod message held review moderation"
    }

    fn icon_name(&self) -> &str {
        "shield"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "any".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("channel.automod.message.hold".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let automod = event.payload.get("automod");
        let user = event.payload.get("user");

        // automod.message_id is the key input for approve_message/deny_message sub-actions.
        let message_id = automod
            .and_then(|a| a.get("message_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let category = automod
            .and_then(|a| a.get("category"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let level = automod
            .and_then(|a| a.get("level"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let held_at = automod
            .and_then(|a| a.get("held_at"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_login = user
            .and_then(|u| u.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_id = user
            .and_then(|u| u.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let message_text = event
            .payload
            .get("message_text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("automod.message_id".to_owned(), Variant::String(message_id))
            .set("automod.category".to_owned(), Variant::String(category))
            .set("automod.level".to_owned(), Variant::Int(level))
            .set("held_at".to_owned(), Variant::String(held_at))
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("message_text".to_owned(), Variant::String(message_text))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "automod.message_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Automod message ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "automod.category".to_owned(),
                        kind: VariantKind::String,
                        label: "Automod category".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "automod.level".to_owned(),
                        kind: VariantKind::Int,
                        label: "Automod level".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt { min: 0, max: 4 }),
                    },
                    DeclaredVariable {
                        name: "held_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Held at".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "user_login".to_owned(),
                        kind: VariantKind::String,
                        label: "User login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "user_id".to_owned(),
                        kind: VariantKind::String,
                        label: "User ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "message_text".to_owned(),
                        kind: VariantKind::String,
                        label: "Message text".to_owned(),
                        synthesis: Some(SynthesisHint::Message),
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hold_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.automod.message.hold",
            serde_json::json!({
                "automod": {
                    "message_id": "hold-abc-123",
                    "category": "harassment",
                    "level": 3,
                    "held_at": "2026-06-13T20:00:00Z",
                },
                "user": {
                    "id": "777",
                    "login": "viewer_one",
                    "display_name": "ViewerOne",
                },
                "message_text": "borderline message",
            }),
        )
    }

    #[test]
    fn event_filter_targets_automod_hold_kind_from_twitch() {
        let filter = AutomodMessageHeldDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.automod.message.hold")
        );
    }

    #[test]
    fn build_arg_stack_exposes_message_id_chaining_var_and_typed_level() {
        let stack = AutomodMessageHeldDescriptor.build_arg_stack(&hold_event());
        // automod.message_id is the chaining var feeding approve/deny sub-actions.
        assert_eq!(
            stack.get("automod.message_id"),
            Some(&Variant::String("hold-abc-123".to_owned()))
        );
        // level marshals as Int, not String.
        assert_eq!(stack.get("automod.level"), Some(&Variant::Int(3)));
        // message_text is read from the already-flattened payload field.
        assert_eq!(
            stack.get("message_text"),
            Some(&Variant::String("borderline message".to_owned()))
        );
    }
}
