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
