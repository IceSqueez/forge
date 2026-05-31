use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig};

use super::chat::build_standard_arg_stack;

pub(crate) struct SpellDescriptor;

impl TriggerKindDescriptor for SpellDescriptor {
    fn id(&self) -> &str {
        "trovo.spell"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Bits
    }

    fn label(&self) -> &str {
        "Spell"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer casts a spell in Trovo chat"
    }

    fn search_text(&self) -> &str {
        "trovo spell cast magic donation bits virtual currency"
    }

    fn icon_name(&self) -> &str {
        "sparkles"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Trovo)
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
            source: Some(EventSource::Trovo),
            kind_prefix: Some("trovo.spell".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_standard_arg_stack(event)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_types::Variant;

    fn spell_event() -> Event {
        Event::new(
            EventSource::Trovo,
            "trovo.spell",
            serde_json::json!({
                "content": "FieryDragon",
                "nick_name": "Caster",
                "user_name": "caster_login",
                "sender_id": "uid_caster"
            }),
        )
    }

    #[test]
    fn kind_id_matches_canonical() {
        assert_eq!(SpellDescriptor.id(), "trovo.spell");
    }

    #[test]
    fn category_is_bits() {
        assert_eq!(SpellDescriptor.category(), TriggerCategory::Bits);
    }

    #[test]
    fn is_platform_specific_trovo() {
        assert_eq!(
            SpellDescriptor.event_filter().source,
            Some(EventSource::Trovo)
        );
    }

    #[test]
    fn build_arg_stack_extracts_content() {
        let stack = SpellDescriptor.build_arg_stack(&spell_event());
        assert_eq!(
            stack.get("content"),
            Some(&Variant::String("FieryDragon".to_owned()))
        );
        assert_eq!(
            stack.get("nick_name"),
            Some(&Variant::String("Caster".to_owned()))
        );
    }
}
