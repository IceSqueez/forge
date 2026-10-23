use forge_components::tr_find;
use forge_registry::{SubActionRunner, TriggerKindDescriptor};

const NAME_PREFIX: &str = "kind_name_";
const SUMMARY_PREFIX: &str = "kind_summary_";
const KIND_ID_SEPARATOR: char = '.';
const KEY_SEPARATOR: &str = "_";

pub(crate) fn step_name(runner: &dyn SubActionRunner) -> String {
    kind_copy(NAME_PREFIX, runner.id(), runner.label())
}

pub(crate) fn step_summary(runner: &dyn SubActionRunner) -> String {
    kind_copy(SUMMARY_PREFIX, runner.id(), runner.summary())
}

pub(crate) fn trigger_name(descriptor: &dyn TriggerKindDescriptor) -> String {
    kind_copy(NAME_PREFIX, descriptor.id(), descriptor.label())
}

pub(crate) fn trigger_summary(descriptor: &dyn TriggerKindDescriptor) -> String {
    kind_copy(SUMMARY_PREFIX, descriptor.id(), descriptor.summary())
}

pub(crate) fn trigger_instance_name(
    stored: &str,
    descriptor: Option<&dyn TriggerKindDescriptor>,
) -> String {
    match descriptor {
        Some(descriptor) if stored == descriptor.label() => trigger_name(descriptor),
        _ => stored.to_owned(),
    }
}

fn kind_copy(prefix: &str, kind_id: &str, english: &str) -> String {
    let key = format!(
        "{prefix}{}",
        kind_id.replace(KIND_ID_SEPARATOR, KEY_SEPARATOR)
    );
    tr_find(&key, None).unwrap_or_else(|| english.to_owned())
}
