use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::shield as shield_fields;

pub(crate) struct ShieldModeStartedDescriptor;

impl TriggerKindDescriptor for ShieldModeStartedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.shield_mode_started"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "Shield mode started"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator activates shield mode in the channel"
    }

    fn search_text(&self) -> &str {
        "twitch shield mode protection moderation started activated"
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
            kind_prefix: Some("channel.shield_mode.begin".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let moderator = event.payload.get(shield_fields::MODERATOR);

        let moderator_login = moderator
            .and_then(|m| m.get(shield_fields::MODERATOR_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let moderator_id = moderator
            .and_then(|m| m.get(shield_fields::MODERATOR_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let started_at = event
            .payload
            .get(shield_fields::STARTED_AT)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "moderator_login".to_owned(),
                Variant::String(moderator_login),
            )
            .set("moderator_id".to_owned(), Variant::String(moderator_id))
            .set("started_at".to_owned(), Variant::String(started_at))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "moderator_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Moderator login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "moderator_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Moderator ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "started_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Started at".to_owned(),
                        synthesis: None,
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shield_begin_event() -> Event {
        let payload = serde_json::json!({
            "moderator": { "id": "42", "login": "mod_jane", "display_name": "ModJane" },
            "started_at": "2026-06-13T18:00:00Z",
        });
        Event::new(EventSource::Twitch, "channel.shield_mode.begin", payload)
    }

    #[test]
    fn event_filter_targets_shield_mode_begin_topic_from_twitch() {
        let filter = ShieldModeStartedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.shield_mode.begin")
        );
    }

    #[test]
    fn build_arg_stack_maps_moderator_fields_and_started_at() {
        let stack = ShieldModeStartedDescriptor.build_arg_stack(&shield_begin_event());
        assert_eq!(
            stack.get("moderator_login"),
            Some(&Variant::String("mod_jane".to_owned()))
        );
        assert_eq!(
            stack.get("moderator_id"),
            Some(&Variant::String("42".to_owned()))
        );
        assert_eq!(
            stack.get("started_at"),
            Some(&Variant::String("2026-06-13T18:00:00Z".to_owned()))
        );
    }
}
