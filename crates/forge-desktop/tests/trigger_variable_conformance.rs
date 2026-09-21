#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use forge_events::{Event, EventSource};
use forge_registry::{ActorDeclaration, KindPlatformContract, TriggerRegistry};
use forge_types::{ActorRole, ActorSlot, CanonicalVariable, SlotPresence, VariantKind};

const VOCABULARY_HAS_NOT_REACHED: &[&str] = &[
    "kick.channel.hosted",
    "kick.channel.reward.redemption.updated",
    "kick.channel.subscribed",
    "kick.channel.subscription.gifts",
    "kick.chat.command",
    "kick.chat.message.deleted",
    "kick.chat.message.sent",
    "kick.livestream.metadata.updated",
    "kick.livestream.status.updated",
    "kick.moderation.banned",
    "youtube.channel.member",
    "youtube.channel.member_gift",
    "youtube.channel.member_gift_received",
    "youtube.channel.member_milestone",
    "youtube.channel.user_banned",
    "youtube.chat.command",
    "youtube.chat.message",
    "youtube.chat.message_deleted",
    "youtube.chat.super_chat",
    "youtube.chat.super_sticker",
    "youtube.stream.offline",
    "youtube.stream.online",
    "youtube.stream.title_changed",
];

fn every_registered_trigger() -> TriggerRegistry {
    let mut registry = TriggerRegistry::new();
    forge_platform_twitch::register_twitch_triggers(&mut registry).expect("twitch triggers");
    forge_platform_youtube::register_youtube_triggers(&mut registry).expect("youtube triggers");
    forge_platform_kick::register_kick_triggers(&mut registry).expect("kick triggers");
    forge_obs::register_obs_triggers(&mut registry).expect("obs triggers");
    forge_vtube::register_vtube_triggers(&mut registry).expect("vtube triggers");
    forge_midi::register_midi_triggers(&mut registry).expect("midi triggers");
    forge_hotkey::register_hotkey_triggers(&mut registry).expect("hotkey triggers");
    registry
}

fn declared_names(registry: &TriggerRegistry, trigger_id: &str) -> BTreeSet<String> {
    registry
        .get(trigger_id)
        .unwrap()
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

fn canonical_by_name() -> BTreeMap<&'static str, CanonicalVariable> {
    ActorRole::ALL
        .into_iter()
        .flat_map(|role| {
            ActorSlot::ALL
                .into_iter()
                .map(move |slot| CanonicalVariable::actor(role, slot))
        })
        .chain(std::iter::once(CanonicalVariable::MessageText))
        .chain(
            forge_types::CanonicalCount::ALL
                .into_iter()
                .map(CanonicalVariable::Count),
        )
        .chain(std::iter::once(CanonicalVariable::SubTier))
        .map(|canonical| (canonical.name(), canonical))
        .collect()
}

#[test]
fn the_only_platform_triggers_left_without_an_actor_declaration_are_the_ones_pinned_here() {
    let registry = every_registered_trigger();
    let mut silent: Vec<&str> = registry
        .all()
        .filter(|descriptor| descriptor.actors() == ActorDeclaration::Undeclared)
        .filter(|descriptor| {
            matches!(
                descriptor.platform_contract(),
                KindPlatformContract::PlatformSpecific(_)
            )
        })
        .map(forge_registry::TriggerKindDescriptor::id)
        .collect();
    silent.sort_unstable();

    assert_eq!(
        silent, VOCABULARY_HAS_NOT_REACHED,
        "a platform slice that conforms must shrink this list, and a new platform crate must never grow it"
    );
}

#[test]
fn a_trigger_declares_the_canonical_block_of_every_actor_role_it_names_and_of_no_other() {
    let registry = every_registered_trigger();

    for descriptor in registry.all() {
        let actors = descriptor.actors();
        if actors == ActorDeclaration::Undeclared {
            continue;
        }
        let declared = declared_names(&registry, descriptor.id());

        for role in ActorRole::ALL {
            for slot in ActorSlot::ALL {
                let name = CanonicalVariable::actor(role, slot).name();
                let present = declared.contains(name);
                if !actors.declares(role) {
                    assert!(
                        !present,
                        "'{}' declares no {role:?} but publishes '{name}'",
                        descriptor.id()
                    );
                } else if slot.presence() == SlotPresence::Always {
                    assert!(
                        present,
                        "'{}' names {role:?} but never declares '{name}'",
                        descriptor.id()
                    );
                }
            }
        }
    }
}

#[test]
fn no_trigger_declares_the_same_variable_name_twice() {
    let registry = every_registered_trigger();

    for descriptor in registry.all() {
        let Some(schema) = descriptor.output_schema() else {
            continue;
        };
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for variable in schema.variables {
            assert!(
                seen.insert(variable.name.clone()),
                "'{}' declares '{}' twice",
                descriptor.id(),
                variable.name
            );
        }
    }
}

#[test]
fn a_canonical_name_is_declared_only_with_the_kind_the_vocabulary_gives_it() {
    let registry = every_registered_trigger();
    let vocabulary = canonical_by_name();

    for descriptor in registry.all() {
        let Some(schema) = descriptor.output_schema() else {
            continue;
        };
        for variable in schema.variables {
            let Some(canonical) = vocabulary.get(variable.name.as_str()) else {
                continue;
            };
            assert_eq!(
                variable.kind,
                canonical.kind(),
                "'{}' declares canonical '{}' as {:?}",
                descriptor.id(),
                variable.name,
                variable.kind
            );
        }
    }
}

#[test]
fn every_variable_a_platform_trigger_declares_is_set_with_the_kind_it_declares() {
    let registry = every_registered_trigger();

    for descriptor in registry.all() {
        if !matches!(
            descriptor.platform_contract(),
            KindPlatformContract::PlatformSpecific(_)
        ) {
            continue;
        }
        let Some(schema) = descriptor.output_schema() else {
            continue;
        };
        let source = descriptor
            .event_filter()
            .source
            .unwrap_or(EventSource::Core);
        let event = Event::new(source, descriptor.id(), serde_json::json!({}));
        let stack = descriptor.build_arg_stack(&event).snapshot();

        for declared in schema.variables {
            let actual = stack.get(&declared.name).unwrap_or_else(|| {
                panic!(
                    "'{}' declares '{}' but never sets it",
                    descriptor.id(),
                    declared.name
                )
            });
            assert_eq!(
                VariantKind::from_variant(actual),
                declared.kind,
                "'{}' declares '{}' as {:?} but emits {actual:?}",
                descriptor.id(),
                declared.name,
                declared.kind,
            );
        }
    }
}
