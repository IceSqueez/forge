pub(crate) mod ban;
pub(crate) mod chat;
pub(crate) mod chat_command;
pub(crate) mod host;
pub(crate) mod livestream_metadata;
pub(crate) mod livestream_status;
pub(crate) mod message_deleted;
mod payload_read;
pub(crate) mod reward_redeemed;
pub(crate) mod sub;
pub(crate) mod sub_gift;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_events::{Event, EventSource};
    use forge_registry::{TriggerKindDescriptor, TriggerRegistry};
    use forge_types::{ActorRole, ActorSlot, CanonicalVariable, Variant};
    use serde_json::json;

    use super::payload_read;
    use crate::register_kick_triggers;

    const LIVESTREAM_TRIGGERS: [&str; 2] = [
        "kick.livestream.status.updated",
        "kick.livestream.metadata.updated",
    ];

    fn kick_registry() -> TriggerRegistry {
        let mut registry = TriggerRegistry::new();
        register_kick_triggers(&mut registry).unwrap();
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

    fn field_event(value: serde_json::Value) -> Event {
        Event::new(EventSource::Kick, "kick.probe", json!({ "field": value }))
    }

    fn hostile_payloads() -> [serde_json::Value; 5] {
        [
            json!({}),
            json!(null),
            json!([]),
            json!("a bare string where an object belongs"),
            json!({
                "sender": 1,
                "content": [],
                "message_id": {},
                "reply_to_message_id": [],
                "deleted_by": "d",
                "banned_user": true,
                "moderator": 0,
                "duration_secs": [],
                "reason": 1,
                "subscriber": [],
                "months": "3",
                "tier": 1,
                "gifter": "x",
                "giftees": {},
                "count": {},
                "host": 1.5,
                "viewer_count": null,
                "is_live": "yes",
                "stream_title": true,
                "category": "c",
                "reward": 7,
                "redeemer": [],
                "user_input": 9
            }),
        ]
    }

    #[test]
    fn text_reads_only_json_strings_and_falls_back_to_empty() {
        for (wire, expected) in [
            (json!("hello stream"), "hello stream"),
            (json!(""), ""),
            (json!(null), ""),
            (json!(7), ""),
            (json!(true), ""),
            (json!([]), ""),
            (json!({}), ""),
        ] {
            assert_eq!(
                payload_read::text(&field_event(wire.clone()), "field"),
                expected,
                "wire {wire}"
            );
            assert_eq!(
                payload_read::nested_text(Some(&json!({ "field": wire.clone() })), "field"),
                expected,
                "nested wire {wire}"
            );
        }
        assert_eq!(payload_read::text(&field_event(json!(1)), "absent"), "");
        assert_eq!(payload_read::nested_text(None, "field"), "");
    }

    #[test]
    fn number_reads_only_json_integers_and_falls_back_to_zero() {
        for (wire, expected) in [
            (json!(300), 300),
            (json!(0), 0),
            (json!(-5), -5),
            (json!(3.7), 0),
            (json!("300"), 0),
            (json!(null), 0),
            (json!(true), 0),
            (json!([1]), 0),
            (json!({}), 0),
        ] {
            assert_eq!(
                payload_read::number(&field_event(wire.clone()), "field"),
                expected,
                "wire {wire}"
            );
        }
        assert_eq!(payload_read::number(&field_event(json!(1)), "absent"), 0);
    }

    #[test]
    fn flag_reads_only_json_booleans_and_falls_back_to_false() {
        for (wire, expected) in [
            (json!(true), true),
            (json!(false), false),
            (json!("true"), false),
            (json!(1), false),
            (json!(null), false),
            (json!([]), false),
        ] {
            assert_eq!(
                payload_read::flag(&field_event(wire.clone()), "field"),
                expected,
                "wire {wire}"
            );
        }
        assert!(!payload_read::flag(&field_event(json!(true)), "absent"));
    }

    #[test]
    fn numeric_id_stringifies_only_unsigned_json_numbers() {
        for (wire, expected) in [
            (json!(42), "42"),
            (json!(0), "0"),
            (json!(u64::MAX), "18446744073709551615"),
            (json!(-1), ""),
            (json!("42"), ""),
            (json!(4.2), ""),
            (json!(null), ""),
            (json!({}), ""),
        ] {
            assert_eq!(
                payload_read::numeric_id(Some(&json!({ "id": wire.clone() })), "id"),
                expected,
                "wire {wire}"
            );
        }
        assert_eq!(payload_read::numeric_id(Some(&json!({})), "id"), "");
        assert_eq!(payload_read::numeric_id(Some(&json!(7)), "id"), "");
        assert_eq!(payload_read::numeric_id(None, "id"), "");
    }

    #[test]
    fn an_identity_always_offers_a_login_slot_because_kick_issues_logins() {
        for entity in [
            json!({ "id": 9, "username": "slug", "display_name": "Shown" }),
            json!({ "id": null }),
            json!({}),
            json!(7),
        ] {
            let identity = payload_read::identity(Some(&entity));
            assert!(identity.login.is_some(), "entity {entity}");
        }
        assert!(payload_read::identity(None).login.is_some());
        assert_eq!(
            payload_read::login_of(payload_read::identity(Some(&json!({})))),
            ""
        );
    }

    #[test]
    fn every_declared_variable_holds_a_value_whatever_shape_the_wire_sends() {
        for descriptor in kick_registry().all() {
            let declared = declared_names(descriptor);
            for payload in hostile_payloads() {
                let event = Event::new(EventSource::Kick, descriptor.id(), payload.clone());
                let stack = descriptor.build_arg_stack(&event).snapshot();
                for name in &declared {
                    assert!(
                        stack.contains_key(name),
                        "'{}' declares '{name}' but leaves it unset for {payload}",
                        descriptor.id()
                    );
                }
                for name in stack.keys() {
                    assert!(
                        declared.contains(name),
                        "'{}' sets undeclared '{name}' for {payload}",
                        descriptor.id()
                    );
                }
            }
        }
    }

    #[test]
    fn every_actor_a_kick_trigger_names_carries_a_login_because_kick_issues_them() {
        for descriptor in kick_registry().all() {
            let declared = declared_names(descriptor);
            let actors = descriptor.actors();
            for role in ActorRole::ALL {
                let login = CanonicalVariable::actor(role, ActorSlot::Login)
                    .name()
                    .to_owned();
                assert_eq!(
                    declared.contains(&login),
                    actors.declares(role),
                    "'{}' declares '{login}' but names {role:?}: {}",
                    descriptor.id(),
                    actors.declares(role)
                );
            }
        }
    }

    #[test]
    fn every_actor_a_kick_trigger_names_is_stamped_with_the_kick_platform() {
        for descriptor in kick_registry().all() {
            let event = Event::new(EventSource::Kick, descriptor.id(), json!({}));
            let stack = descriptor.build_arg_stack(&event);
            for role in ActorRole::ALL
                .into_iter()
                .filter(|role| descriptor.actors().declares(*role))
            {
                let name = CanonicalVariable::actor(role, ActorSlot::Platform).name();
                assert_eq!(
                    stack.get(name),
                    Some(&Variant::String("kick".to_owned())),
                    "'{}' stamps '{name}'",
                    descriptor.id()
                );
            }
        }
    }

    #[test]
    fn a_livestream_trigger_names_no_actor_because_the_wire_attributes_it_to_nobody() {
        let registry = kick_registry();
        for id in LIVESTREAM_TRIGGERS {
            let descriptor = registry.get(id).unwrap();
            let declared = declared_names(descriptor);
            let stack = descriptor.build_arg_stack(&Event::new(EventSource::Kick, id, json!({})));
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
