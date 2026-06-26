use forge_registry::{RegistryError, RunContext};
use forge_types::{QueueId, SubActionConfig};

pub(super) fn resolve_queue_id(config: &SubActionConfig, ctx: &RunContext<'_>) -> Option<QueueId> {
    let raw = config.get("queue_id").and_then(|v| v.as_str())?;
    ctx.arg_stack
        .interpolate(raw)
        .trim()
        .parse::<QueueId>()
        .ok()
}

pub(super) fn validate_queue_id(
    config: &SubActionConfig,
    kind_id: &str,
) -> Result<(), RegistryError> {
    match config.get("queue_id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => Ok(()),
        _ => Err(RegistryError::UnknownKindId(format!(
            "{kind_id}: queue_id is required"
        ))),
    }
}
