use forge_registry::{ConfigReason, RegistryError, RunContext, SubActionConfigExt};
use forge_types::{QueueId, SubActionConfig};

pub(super) fn resolve_queue_id(config: &SubActionConfig, ctx: &RunContext<'_>) -> Option<QueueId> {
    let raw = config.str("queue_id")?;
    ctx.arg_stack
        .interpolate(raw)
        .trim()
        .parse::<QueueId>()
        .ok()
}

pub(super) fn validate_queue_id(config: &SubActionConfig) -> Result<(), RegistryError> {
    if config.str_nonempty("queue_id").is_some() {
        Ok(())
    } else {
        Err(RegistryError::invalid_field(
            "queue_id",
            ConfigReason::Required,
        ))
    }
}
