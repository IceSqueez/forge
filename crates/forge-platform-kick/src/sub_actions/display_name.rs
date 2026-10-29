use forge_registry::runner::SubActionConfig;
use forge_registry::{RunContext, SubActionConfigExt};

use crate::vocabulary::config_keys;

pub(super) fn resolve(config: &SubActionConfig, ctx: &RunContext<'_>) -> Option<String> {
    config
        .str(config_keys::DISPLAY_NAME)
        .map(|template| ctx.arg_stack.interpolate(template).trim().to_owned())
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_events::{Event, EventPublisher};
    use forge_types::{ArgStack, EventId, Variant};

    use super::*;

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    fn name_config(name: Variant) -> SubActionConfig {
        BTreeMap::from([(config_keys::DISPLAY_NAME.to_owned(), name)])
    }

    fn text(name: &str) -> SubActionConfig {
        name_config(Variant::String(name.to_owned()))
    }

    #[test]
    fn resolve_interpolates_trims_and_drops_blank_or_non_string_names() {
        let stack = ArgStack::new()
            .set(
                "user_name".to_owned(),
                Variant::String("  Alice ".to_owned()),
            )
            .set("blank".to_owned(), Variant::String("   ".to_owned()));
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NoopPublisher);

        for (config, expected) in [
            (text("%user_name%"), Some("Alice")),
            (text("  Олена  "), Some("Олена")),
            (text("%blank%"), None),
            (text("   "), None),
            (text(""), None),
            (name_config(Variant::Int(7)), None),
            (BTreeMap::new(), None),
        ] {
            assert_eq!(
                resolve(&config, &ctx).as_deref(),
                expected,
                "config {config:?}"
            );
        }
    }
}
