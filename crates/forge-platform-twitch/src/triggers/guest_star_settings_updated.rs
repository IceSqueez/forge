use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::guest_star as guest_star_fields;

pub(crate) struct GuestStarSettingsUpdatedDescriptor;

impl TriggerKindDescriptor for GuestStarSettingsUpdatedDescriptor {
    fn id(&self) -> &str {
        "twitch.guest_star.settings_updated"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Guest Star settings updated"
    }

    fn summary(&self) -> &str {
        "Fires when the broadcaster's Guest Star preferences change"
    }

    fn search_text(&self) -> &str {
        "twitch guest star settings preferences layout slots moderator audio"
    }

    fn icon_name(&self) -> &str {
        "settings"
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
            kind_prefix: Some("channel.guest_star_settings.update".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let settings = event.payload.get(guest_star_fields::SETTINGS);

        let slot_count = settings
            .and_then(|s| s.get(guest_star_fields::SLOT_COUNT))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let group_layout = settings
            .and_then(|s| s.get(guest_star_fields::GROUP_LAYOUT))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let is_moderator_send_live_enabled = settings
            .and_then(|s| s.get(guest_star_fields::IS_MODERATOR_SEND_LIVE_ENABLED))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        ArgStack::new()
            .set("guest_star.slot_count".to_owned(), Variant::Int(slot_count))
            .set(
                "guest_star.group_layout".to_owned(),
                Variant::String(group_layout),
            )
            .set(
                "guest_star.is_moderator_send_live_enabled".to_owned(),
                Variant::Bool(is_moderator_send_live_enabled),
            )
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "guest_star.slot_count".to_owned(),
                        kind: VariantKind::Int,
                        label: "Slot count".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt { min: 0, max: 6 }),
                    },
                    DeclaredVariable {
                        name: "guest_star.group_layout".to_owned(),
                        kind: VariantKind::String,
                        label: "Group layout".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "guest_star.is_moderator_send_live_enabled".to_owned(),
                        kind: VariantKind::Bool,
                        label: "Moderator can send live".to_owned(),
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

    fn settings_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.guest_star_settings.update",
            serde_json::json!({
                "settings": {
                    "slot_count": 5,
                    "group_layout": "TILED_LAYOUT",
                    "is_moderator_send_live_enabled": true,
                },
            }),
        )
    }

    #[test]
    fn event_filter_targets_settings_update_kind_from_twitch() {
        let filter = GuestStarSettingsUpdatedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.guest_star_settings.update")
        );
    }

    #[test]
    fn build_arg_stack_marshals_slot_count_as_int_and_moderator_flag_as_bool() {
        let stack = GuestStarSettingsUpdatedDescriptor.build_arg_stack(&settings_event());
        assert_eq!(stack.get("guest_star.slot_count"), Some(&Variant::Int(5)));
        assert_eq!(
            stack.get("guest_star.group_layout"),
            Some(&Variant::String("TILED_LAYOUT".to_owned()))
        );
        assert_eq!(
            stack.get("guest_star.is_moderator_send_live_enabled"),
            Some(&Variant::Bool(true))
        );
    }
}
