use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionConfigExt};
use forge_types::Variant;

const REASON_KEY: &str = "reason";
const REASON_MAX_CHARS: usize = 100;

pub(super) fn default_entry() -> (String, Variant) {
    (REASON_KEY.to_owned(), Variant::String(String::new()))
}

pub(super) fn form_field() -> FormField {
    FormField::Text {
        key: REASON_KEY,
        label: "Reason (optional, max 100 chars)",
        placeholder: "",
    }
}

pub(super) fn validate(kind_id: &str, config: &SubActionConfig) -> Result<(), RegistryError> {
    match config.get(REASON_KEY) {
        None => Ok(()),
        Some(Variant::String(reason)) if reason.chars().count() <= REASON_MAX_CHARS => Ok(()),
        Some(Variant::String(_)) => Err(RegistryError::InvalidConfig(format!(
            "{kind_id}: '{REASON_KEY}' must not exceed {REASON_MAX_CHARS} characters"
        ))),
        Some(_) => Err(RegistryError::InvalidConfig(format!(
            "{kind_id}: '{REASON_KEY}' must be a string"
        ))),
    }
}

pub(super) fn resolve(config: &SubActionConfig, ctx: &RunContext<'_>) -> Option<String> {
    config
        .str(REASON_KEY)
        .map(|template| ctx.arg_stack.interpolate(template).trim().to_owned())
        .filter(|reason| !reason.is_empty())
}
