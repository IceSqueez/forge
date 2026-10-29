pub(crate) mod channel_member;
pub(crate) mod channel_member_milestone;
pub(crate) mod channel_user_banned;
pub(crate) mod chat_command;
pub(crate) mod chat_message;
pub(crate) mod chat_super_chat;
pub(crate) mod chat_super_sticker;
pub(crate) mod member_gift;
pub(crate) mod member_gift_received;
pub(crate) mod message_deleted;
mod payload_read;
pub(crate) mod stream_offline;
pub(crate) mod stream_online;
pub(crate) mod title_changed;

#[cfg(test)]
mod tests {
    use forge_events::{Event, EventSource};
    use forge_registry::{
        ActorRole, ActorSlot, CanonicalCount, CanonicalVariable, TriggerKindDescriptor,
        TriggerRegistry,
    };
    use forge_types::Variant;

    use crate::register_youtube_triggers;

    const BROADCAST_TRIGGERS: [&str; 3] = [
        "youtube.stream.online",
        "youtube.stream.offline",
        "youtube.stream.title_changed",
    ];

    fn youtube_registry() -> TriggerRegistry {
        let mut registry = TriggerRegistry::new();
        register_youtube_triggers(&mut registry).unwrap();
        registry
    }

    fn declared_names(descriptor: &dyn TriggerKindDescriptor) -> Vec<String> {
        descriptor
            .output_schema()
            .map(|schema| {
                schema
                    .variables
                    .into_iter()
                    .map(|variable| variable.name)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn empty_event(descriptor: &dyn TriggerKindDescriptor) -> Event {
        Event::new(EventSource::YouTube, descriptor.id(), serde_json::json!({}))
    }

    fn legacy_twins(
        descriptor: &dyn TriggerKindDescriptor,
    ) -> Vec<(forge_registry::TriggerVariable, CanonicalVariable)> {
        forge_registry::declared_variables(descriptor)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|variable| {
                let canonical = variable.standing.superseded_by()?;
                Some((variable, canonical))
            })
            .collect()
    }

    #[test]
    fn every_legacy_name_is_declared_with_the_kind_and_hint_of_a_canonical_twin_on_its_trigger() {
        for descriptor in youtube_registry().all() {
            let declarations = forge_registry::declared_variables(descriptor).unwrap_or_default();
            for (legacy, canonical) in legacy_twins(descriptor) {
                let twin = declarations.iter().find(|variable| {
                    variable.standing == forge_registry::VariableStanding::Canonical(canonical)
                });
                assert_eq!(
                    twin.map(|twin| (twin.declared.kind, twin.declared.synthesis.clone())),
                    Some((legacy.declared.kind, legacy.declared.synthesis.clone())),
                    "'{}' legacy '{}' defers to '{}'",
                    descriptor.id(),
                    legacy.declared.name,
                    canonical.name()
                );
            }
        }
    }

    fn twin_its_name_describes(legacy: &str) -> Option<CanonicalVariable> {
        let role = [
            ("moderator", ActorRole::Moderator),
            ("gifter", ActorRole::Gifter),
            ("recipient", ActorRole::Recipient),
        ]
        .into_iter()
        .find(|(word, _)| legacy.contains(word))
        .map_or(ActorRole::Principal, |(_, role)| role);
        let actor = |slot| Some(CanonicalVariable::actor(role, slot));
        let ends = |suffixes: &[&str]| suffixes.iter().any(|suffix| legacy.ends_with(suffix));
        if ends(&["tier", "level_name"]) {
            Some(CanonicalVariable::SubTier)
        } else if ends(&["display_name"]) {
            actor(ActorSlot::Name)
        } else if ends(&["login", "username"]) {
            actor(ActorSlot::Login)
        } else if ends(&["message_id"]) {
            Some(CanonicalVariable::MessageId)
        } else if ends(&["id"]) {
            actor(ActorSlot::Id)
        } else if ends(&["bits"]) {
            Some(CanonicalVariable::Count(CanonicalCount::BitsAmount))
        } else if ends(&["viewer_count"]) {
            Some(CanonicalVariable::Count(CanonicalCount::ViewerCount))
        } else if ends(&["count"]) {
            Some(CanonicalVariable::Count(CanonicalCount::GiftCount))
        } else if ends(&["month", "months"]) {
            Some(CanonicalVariable::Count(
                CanonicalCount::SubCumulativeMonths,
            ))
        } else if ends(&["content", "input", "text", "message"]) {
            Some(CanonicalVariable::MessageText)
        } else {
            None
        }
    }

    #[test]
    fn every_legacy_name_defers_to_the_canonical_its_name_describes() {
        for descriptor in youtube_registry().all() {
            for (legacy, canonical) in legacy_twins(descriptor) {
                assert_eq!(
                    twin_its_name_describes(&legacy.declared.name),
                    Some(canonical),
                    "'{}' legacy '{}'",
                    descriptor.id(),
                    legacy.declared.name
                );
            }
        }
    }

    #[test]
    fn no_youtube_actor_carries_a_login_because_youtube_issues_none() {
        for descriptor in youtube_registry().all() {
            let declared = declared_names(descriptor);
            let stack = descriptor.build_arg_stack(&empty_event(descriptor));
            for role in ActorRole::ALL {
                let login = CanonicalVariable::actor(role, ActorSlot::Login).name();
                assert!(
                    !declared.contains(&login.to_owned()),
                    "'{}' declares '{login}'",
                    descriptor.id()
                );
                assert_eq!(
                    stack.get(login),
                    None,
                    "'{}' sets '{login}'",
                    descriptor.id()
                );
            }
        }
    }

    #[test]
    fn every_actor_a_youtube_trigger_names_is_stamped_with_the_youtube_platform() {
        for descriptor in youtube_registry().all() {
            let actors = descriptor.actors();
            let stack = descriptor.build_arg_stack(&empty_event(descriptor));
            for role in ActorRole::ALL
                .into_iter()
                .filter(|role| actors.declares(*role))
            {
                let name = CanonicalVariable::actor(role, ActorSlot::Platform).name();
                assert_eq!(
                    stack.get(name),
                    Some(&Variant::String("youtube".to_owned())),
                    "'{}' stamps '{name}'",
                    descriptor.id()
                );
            }
        }
    }

    #[test]
    fn a_broadcast_trigger_names_no_actor_because_the_wire_attributes_it_to_nobody() {
        let registry = youtube_registry();
        for id in BROADCAST_TRIGGERS {
            let descriptor = registry.get(id).unwrap();
            let declared = declared_names(descriptor);
            let stack = descriptor.build_arg_stack(&empty_event(descriptor));
            for role in ActorRole::ALL {
                for slot in ActorSlot::ALL {
                    let name = CanonicalVariable::actor(role, slot).name();
                    assert!(
                        !declared.contains(&name.to_owned()),
                        "'{id}' declares '{name}'"
                    );
                    assert_eq!(stack.get(name), None, "'{id}' sets '{name}'");
                }
            }
        }
    }
}
