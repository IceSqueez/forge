use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalCount, CanonicalVariable, DeclaredVariable, PlatformId,
    SynthesisHint, TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, kick_actor};
use crate::payload_fields::host as fields;

pub(crate) struct HostDescriptor;

impl TriggerKindDescriptor for HostDescriptor {
    fn id(&self) -> &str {
        "kick.channel.hosted"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Channel hosted"
    }

    fn summary(&self) -> &str {
        "Fires when another streamer hosts this Kick channel"
    }

    fn search_text(&self) -> &str {
        "kick host raid streamer hosting channel"
    }

    fn icon_name(&self) -> &str {
        "users"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Kick)
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
            source: Some(EventSource::Kick),
            kind_prefix: Some("kick.channel.hosted".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(kick_actor(ActorRole::Principal), host_identity)
                .count(CanonicalCount::ViewerCount, |event| {
                    payload_read::number(event, fields::VIEWER_COUNT)
                })
                .legacy(
                    DeclaredVariable {
                        name: "host_username".to_owned(),
                        kind: VariantKind::String,
                        label: "Hosting channel username".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                    |event| Variant::String(payload_read::login_of(host_identity(event))),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn host_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(fields::HOST))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use serde_json::json;

    fn host_event(payload: serde_json::Value) -> Event {
        Event::new(EventSource::Kick, "kick.channel.hosted", payload)
    }

    fn a_host_bringing_a_crowd() -> Event {
        host_event(json!({
            "host": { "id": null, "username": "hosting_channel" },
            "viewer_count": 250
        }))
    }

    #[test]
    fn a_host_the_wire_gives_no_id_keeps_an_empty_user_id_beside_a_real_login() {
        let stack = HostDescriptor.build_arg_stack(&a_host_bringing_a_crowd());
        assert_eq!(stack.get("user_id"), Some(&Variant::String(String::new())));
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("hosting_channel".to_owned()))
        );
    }

    #[test]
    fn the_legacy_host_username_still_carries_what_user_login_carries() {
        let stack = HostDescriptor.build_arg_stack(&a_host_bringing_a_crowd());
        assert_eq!(stack.get("host_username"), stack.get("user_login"));
        assert_eq!(
            stack.get("host_username"),
            Some(&Variant::String("hosting_channel".to_owned()))
        );
    }

    #[test]
    fn the_viewer_count_is_an_integer_read_from_the_host_payload() {
        for (wire, expected) in [(json!(250), 250), (json!(0), 0), (json!(50_000), 50_000)] {
            let stack = HostDescriptor.build_arg_stack(&host_event(json!({
                "host": { "id": null, "username": "hosting_channel" },
                "viewer_count": wire.clone()
            })));
            assert_eq!(
                stack.get("viewer_count"),
                Some(&Variant::Int(expected)),
                "wire {wire}"
            );
        }
    }
}
