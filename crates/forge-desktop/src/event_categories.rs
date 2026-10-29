use forge_events::{Event, EventSource};
use forge_registry::{TriggerCategory, TriggerRegistry, kind_matches_prefix};

struct CategorizedPrefix {
    source: Option<EventSource>,
    prefix: String,
    category: TriggerCategory,
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
                })
            })
            .collect();
        Self { prefixes }
    }

    pub(crate) fn of(&self, event: &Event) -> Vec<TriggerCategory> {
        let candidates = || {
            self.prefixes.iter().filter(|entry| {
                entry.source.is_none_or(|source| source == event.source)
                    && kind_matches_prefix(&event.kind, &entry.prefix)
            })
        };
        let Some(longest) = candidates().map(|entry| entry.prefix.len()).max() else {
            return Vec::new();
        };
        let mut categories = Vec::new();
        for entry in candidates().filter(|entry| entry.prefix.len() == longest) {
            if !categories.contains(&entry.category) {
                categories.push(entry.category);
            }
        }
        categories
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
        categories.of(&Event::new(source, kind, serde_json::Value::Null))
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
                TriggerCategory::Obs,
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
