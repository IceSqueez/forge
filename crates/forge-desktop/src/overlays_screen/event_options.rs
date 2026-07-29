use std::collections::BTreeSet;

use forge_registry::TriggerRegistry;

/// The trigger registry is the only runtime roster of observable event kinds - `kind` is a plain
/// string with no central enum. A filter naming a family prefix rather than one kind is skipped,
/// because a prefix is not something a page can bind to.
pub(super) fn event_kind_options(registry: &TriggerRegistry) -> Vec<(String, String)> {
    let kinds: BTreeSet<String> = registry
        .all()
        .filter_map(|descriptor| descriptor.event_filter().kind_prefix)
        .filter(|kind| !kind.is_empty() && !kind.ends_with('.'))
        .collect();

    kinds.into_iter().map(|kind| (kind.clone(), kind)).collect()
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use forge_events::Event;
    use forge_registry::{
        EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
    };
    use forge_types::{ArgStack, TriggerConfig};

    use super::*;

    struct StubDescriptor {
        id: &'static str,
        kind_prefix: Option<&'static str>,
    }

    impl TriggerKindDescriptor for StubDescriptor {
        fn id(&self) -> &str {
            self.id
        }
        fn category(&self) -> TriggerCategory {
            TriggerCategory::Core
        }
        fn label(&self) -> &str {
            "Stub"
        }
        fn summary(&self) -> &str {
            "stub"
        }
        fn search_text(&self) -> &str {
            ""
        }
        fn icon_name(&self) -> &str {
            "bolt"
        }
        fn platform_contract(&self) -> KindPlatformContract {
            KindPlatformContract::Universal
        }
        fn default_config(&self) -> TriggerConfig {
            TriggerConfig::new()
        }
        fn config_fields(&self) -> Vec<FormField> {
            Vec::new()
        }
        fn condition_display(&self, _config: &TriggerConfig) -> String {
            String::new()
        }
        fn event_filter(&self) -> EventFilter {
            EventFilter {
                source: None,
                kind_prefix: self.kind_prefix.map(str::to_owned),
            }
        }
        fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
            false
        }
        fn build_arg_stack(&self, _event: &Event) -> ArgStack {
            ArgStack::new()
        }
    }

    fn registry_of(entries: &[(&'static str, Option<&'static str>)]) -> TriggerRegistry {
        let mut registry = TriggerRegistry::new();
        for (id, kind_prefix) in entries {
            registry
                .register(Box::new(StubDescriptor {
                    id,
                    kind_prefix: *kind_prefix,
                }))
                .expect("each stub descriptor carries its own id");
        }
        registry
    }

    #[test]
    fn a_page_is_offered_every_bindable_kind_once_in_a_stable_order() {
        let registry = registry_of(&[
            ("t.chat", Some("twitch.chat.message")),
            ("t.chat.mirror", Some("twitch.chat.message")),
            ("o.scene", Some("obs.scene.changed")),
            ("k.chat", Some("kick.chat.message")),
        ]);

        let offered: Vec<String> = event_kind_options(&registry)
            .into_iter()
            .map(|(value, _)| value)
            .collect();

        assert_eq!(
            offered,
            vec![
                "kick.chat.message".to_owned(),
                "obs.scene.changed".to_owned(),
                "twitch.chat.message".to_owned(),
            ],
            "two triggers watching one kind must offer it once, and the roster must not shuffle \
             between renders"
        );
    }

    #[test]
    fn a_filter_that_names_no_single_kind_is_kept_out_of_the_roster() {
        let registry = registry_of(&[
            ("family", Some("twitch.")),
            ("deep.family", Some("twitch.channel.")),
            ("empty", Some("")),
            ("unfiltered", None),
            ("real", Some("twitch.raid")),
        ]);

        let offered: Vec<String> = event_kind_options(&registry)
            .into_iter()
            .map(|(value, _)| value)
            .collect();

        assert_eq!(
            offered,
            vec!["twitch.raid".to_owned()],
            "a family prefix is not something a page can bind to"
        );
    }
}
