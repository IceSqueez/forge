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
