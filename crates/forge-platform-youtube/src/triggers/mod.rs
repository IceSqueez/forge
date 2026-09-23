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
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_events::{Event, EventSource};
    use forge_registry::{TriggerKindDescriptor, TriggerRegistry};
    use forge_types::{ActorRole, ActorSlot, CanonicalVariable, Variant};

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
