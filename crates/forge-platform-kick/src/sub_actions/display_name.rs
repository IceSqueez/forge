use forge_registry::runner::SubActionConfig;
use forge_registry::{RunContext, SubActionConfigExt};

pub(super) const DISPLAY_NAME_KEY: &str = "display_name";

pub(super) fn resolve(config: &SubActionConfig, ctx: &RunContext<'_>) -> Option<String> {
    config
        .str(DISPLAY_NAME_KEY)
        .map(|template| ctx.arg_stack.interpolate(template).trim().to_owned())
        .filter(|name| !name.is_empty())
}
