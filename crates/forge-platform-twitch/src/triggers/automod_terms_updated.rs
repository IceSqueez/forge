use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{ActorRole, DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::automod as automod_fields;

pub(crate) struct AutomodTermsUpdatedDescriptor;

impl TriggerKindDescriptor for AutomodTermsUpdatedDescriptor {
    fn id(&self) -> &str {
        "twitch.automod.terms_updated"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "AutoMod terms updated"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator adds or removes permitted or blocked AutoMod terms"
    }

    fn search_text(&self) -> &str {
        "twitch automod terms blocked permitted moderator added removed updated"
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
            kind_prefix: Some("twitch.automod.terms.update".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), terms_moderator_identity)
                .actor(twitch_actor(ActorRole::Moderator), terms_moderator_identity)
                .event_specific(
                    DeclaredVariable {
                        name: "automod.action".to_owned(),
                        kind: VariantKind::String,
                        label: "Automod terms action".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, automod_fields::ACTION)),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actors(&[ActorRole::Moderator])
    }
}

fn terms_moderator_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(automod_fields::MODERATOR),
        automod_fields::MODERATOR_ID,
        automod_fields::MODERATOR_LOGIN,
        automod_fields::MODERATOR_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.automod.terms.update",
            serde_json::json!({
                "moderator": {
                    "id": "mod-7",
                    "login": "mod_login",
                    "display_name": "ModLogin",
                },
                "action": "add_blocked",
                "terms": ["badword"],
            }),
        )
    }

    #[test]
    fn event_filter_targets_automod_terms_update_kind_from_twitch() {
        let filter = AutomodTermsUpdatedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.automod.terms.update")
        );
    }

    #[test]
    fn build_arg_stack_reads_top_level_action_into_automod_action_var() {
        let stack = AutomodTermsUpdatedDescriptor.build_arg_stack(&terms_event());
        assert_eq!(
            stack.get("moderator_login"),
            Some(&Variant::String("mod_login".to_owned()))
        );
        assert_eq!(
            stack.get("automod.action"),
            Some(&Variant::String("add_blocked".to_owned()))
        );
    }
}
