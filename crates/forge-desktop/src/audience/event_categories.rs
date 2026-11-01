use forge_events::{Event, EventSource};
use forge_registry::{TriggerCategory, TriggerRegistry, kind_matches_prefix};
use forge_types::IntegrationId;

struct CategorizedPrefix {
    source: Option<EventSource>,
    prefix: String,
    category: TriggerCategory,
    owner: Option<IntegrationId>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct EventClasses {
    pub categories: Vec<TriggerCategory>,
    pub owners: Vec<IntegrationId>,
}

#[derive(Default)]
pub(crate) struct EventCategories {
    prefixes: Vec<CategorizedPrefix>,
}

impl EventCategories {
    pub(crate) fn from_registry(registry: &TriggerRegistry) -> Self {
        let prefixes = registry
            .all()
            .filter_map(|descriptor| {
                let filter = descriptor.event_filter();
                Some(CategorizedPrefix {
                    source: filter.source,
                    prefix: filter.kind_prefix?,
                    category: descriptor.category(),
                    owner: registry.owning_integration(descriptor.id()).cloned(),
                })
            })
            .collect();
        Self { prefixes }
    }

    pub(crate) fn of(&self, event: &Event) -> EventClasses {
        let candidates = || {
            self.prefixes.iter().filter(|entry| {
                entry.source.is_none_or(|source| source == event.source)
                    && kind_matches_prefix(&event.kind, &entry.prefix)
            })
        };
        let Some(longest) = candidates().map(|entry| entry.prefix.len()).max() else {
            return EventClasses::default();
        };
        let mut classes = EventClasses::default();
        for entry in candidates().filter(|entry| entry.prefix.len() == longest) {
            if !classes.categories.contains(&entry.category) {
                classes.categories.push(entry.category);
            }
            if entry.category == TriggerCategory::Integration
                && let Some(owner) = &entry.owner
                && !classes.owners.contains(owner)
            {
                classes.owners.push(owner.clone());
            }
        }
        classes
    }
}

#[cfg(test)]
mod tests {
    use forge_events::{Event, EventSource};
    use forge_registry::{TriggerCategory, TriggerRegistry};

    use super::EventCategories;
    use crate::test_support::{Declares, StubTrigger};

    type Listener<'a> = (&'a str, Option<EventSource>, &'a str, TriggerCategory);

    fn categories(listeners: &[Listener<'_>]) -> EventCategories {
        let mut registry = TriggerRegistry::new();
        for (id, source, prefix, category) in listeners {
            registry
                .register(Box::new(
                    StubTrigger::new(id, id, *category, Declares::Nothing)
                        .listening_to(*source, prefix),
                ))
                .unwrap();
        }
        EventCategories::from_registry(&registry)
    }

    fn of(categories: &EventCategories, source: EventSource, kind: &str) -> Vec<TriggerCategory> {
        categories
            .of(&Event::new(source, kind, serde_json::Value::Null))
            .categories
    }

    #[test]
    fn the_longest_matching_prefix_decides_the_category() {
        let registry = categories(&[
            (
                "moderation",
                None,
                "twitch.channel",
                TriggerCategory::Moderation,
            ),
            ("cheer", None, "twitch.channel.cheer", TriggerCategory::Bits),
        ]);

        for (kind, expected) in [
            ("twitch.channel.cheer", vec![TriggerCategory::Bits]),
            ("twitch.channel.cheer.combo", vec![TriggerCategory::Bits]),
            ("twitch.channel.ban", vec![TriggerCategory::Moderation]),
            ("twitch.channel.cheers", vec![TriggerCategory::Moderation]),
            ("youtube.chat.message", Vec::new()),
        ] {
            assert_eq!(of(&registry, EventSource::Twitch, kind), expected, "{kind}");
        }
    }

    #[test]
    fn a_source_filtered_prefix_applies_only_to_its_own_source() {
        let registry = categories(&[(
            "kick-chat",
            Some(EventSource::Kick),
            "chat.message",
            TriggerCategory::Chat,
        )]);

        assert_eq!(
            (
                of(&registry, EventSource::Kick, "chat.message"),
                of(&registry, EventSource::Twitch, "chat.message"),
            ),
            (vec![TriggerCategory::Chat], Vec::new())
        );
    }

    #[test]
    fn a_longer_prefix_for_another_source_does_not_shadow_a_matching_shorter_one() {
        let registry = categories(&[
            (
                "scene-switch",
                Some(EventSource::Obs),
                "obs.scene.changed",
                TriggerCategory::Integration,
            ),
            ("scene-any", None, "obs.scene", TriggerCategory::Streams),
        ]);

        assert_eq!(
            of(&registry, EventSource::Core, "obs.scene.changed"),
            vec![TriggerCategory::Streams]
        );
    }

    #[test]
    fn descriptors_sharing_a_prefix_contribute_each_distinct_category_once() {
        let registry = categories(&[
            (
                "gift-sub",
                None,
                "kick.subscription.gifted",
                TriggerCategory::Subscriptions,
            ),
            (
                "gift-hype",
                None,
                "kick.subscription.gifted",
                TriggerCategory::Hype,
            ),
            (
                "gift-sub-again",
                None,
                "kick.subscription.gifted",
                TriggerCategory::Subscriptions,
            ),
        ]);

        let found = of(&registry, EventSource::Kick, "kick.subscription.gifted");

        assert_eq!(found.len(), 2, "{found:?}");
        assert!(
            found.contains(&TriggerCategory::Subscriptions)
                && found.contains(&TriggerCategory::Hype),
            "{found:?}"
        );
    }

    #[test]
    fn only_integration_grouped_kinds_report_their_owner() {
        let mut registry = TriggerRegistry::new();
        let mut owned = registry.owned_by(forge_obs::OBS_INTEGRATION.id);
        owned
            .register(Box::new(
                StubTrigger::new(
                    "grouped",
                    "grouped",
                    TriggerCategory::Integration,
                    Declares::Nothing,
                )
                .listening_to(None, "obs.scene"),
            ))
            .unwrap();
        owned
            .register(Box::new(
                StubTrigger::new(
                    "functional",
                    "functional",
                    TriggerCategory::Streams,
                    Declares::Nothing,
                )
                .listening_to(None, "obs.stream"),
            ))
            .unwrap();
        let categories = EventCategories::from_registry(&registry);
        let classes_of = |kind: &str| {
            categories.of(&Event::new(
                EventSource::Core,
                kind,
                serde_json::Value::Null,
            ))
        };

        assert_eq!(
            (
                classes_of("obs.scene.changed").owners,
                classes_of("obs.stream.started").owners,
            ),
            (vec![forge_obs::OBS_INTEGRATION.id], Vec::new())
        );
    }

    #[test]
    fn descriptors_without_a_kind_prefix_categorise_nothing() {
        let mut registry = TriggerRegistry::new();
        registry
            .register(Box::new(StubTrigger::new(
                "anything",
                "anything",
                TriggerCategory::Core,
                Declares::Nothing,
            )))
            .unwrap();
        let categories = EventCategories::from_registry(&registry);

        assert!(of(&categories, EventSource::Core, "core.started").is_empty());
    }
}
