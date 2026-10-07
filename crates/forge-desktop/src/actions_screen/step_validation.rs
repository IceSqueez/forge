use forge_registry::SubActionRunner;
use forge_types::{SubActionConfig, Variant, variable_references};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StepRejection {
    pub field_key: Option<String>,
    pub message: String,
}

pub(super) fn step_rejection(
    runner: &dyn SubActionRunner,
    config: &SubActionConfig,
    field_keys: &[String],
) -> Option<StepRejection> {
    let error = runner.validate_config(config).err()?;
    let raw = error.to_string();
    let message = raw
        .strip_prefix(runner.id())
        .and_then(|rest| rest.strip_prefix(':'))
        .map(str::trim_start)
        .unwrap_or(&raw)
        .to_owned();
    let field_key = field_keys
        .iter()
        .find(|key| message_names_key(&message, key))
        .cloned();
    let deferred = match &field_key {
        Some(key) => config.get(key).is_some_and(holds_variable_reference),
        None => config.values().any(holds_variable_reference),
    };
    (!deferred).then_some(StepRejection { field_key, message })
}

fn message_names_key(message: &str, key: &str) -> bool {
    message.contains(&format!("'{key}'"))
        || message.contains(&format!("`{key}`"))
        || message.starts_with(&format!("{key} "))
}

fn holds_variable_reference(value: &Variant) -> bool {
    match value {
        Variant::String(text) => variable_references(text).next().is_some(),
        _ => false,
    }
}
