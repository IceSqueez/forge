use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, TriggerCategory, TriggerVariables,
};
use forge_types::{DeclaredVariable, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::automod as automod_fields;

fn variables() -> Option<TriggerVariables> {
    Some(
        TriggerVariables::new()
            .actor(twitch_actor(ActorRole::Principal), terms_moderator_identity)
            .actor(twitch_actor(ActorRole::Moderator), terms_moderator_identity)
            .event_specific(
                DeclaredVariable {
                    name: "automod.action".to_owned(),
                    kind: VariantKind::String,
                    synthesis: None,
                },
                |event| Variant::String(payload_read::text(event, automod_fields::ACTION)),
            ),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::AUTOMOD_TERMS_UPDATED,
    category: TriggerCategory::Moderation,
    label: "AutoMod terms updated",
    summary: "Fires when a moderator adds or removes permitted or blocked AutoMod terms",
    search_text: "twitch automod terms blocked permitted moderator added removed updated",
    icon: "shield",
    kind: event_kinds::AUTOMOD_TERMS_UPDATE,
    actors: ActorDeclaration::Actors(&[ActorRole::Moderator]),
    variables,
    ..TwitchTriggerSpec::BASE
};

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
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;

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
    fn build_arg_stack_reads_top_level_action_into_automod_action_var() {
        let stack = SPEC.build_arg_stack(&terms_event());
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
