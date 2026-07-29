use forge_types::{ArgStack, Variant};

use crate::config::effective_overlay_config;
use crate::descriptor::{ConfigSection, OverlayConfig, OverlayKindDescriptor};

/// A supplied value wins unless it is an empty string; every other content key falls back to the
/// overlay's own configured value, and both sides expand against the same stack. Keys the kind
/// does not declare as content are dropped, and a key neither side holds is omitted rather than
/// invented.
pub fn delivered_content(
    descriptor: &dyn OverlayKindDescriptor,
    stored: &OverlayConfig,
    supplied: &OverlayConfig,
    args: &ArgStack,
) -> OverlayConfig {
    let configured = effective_overlay_config(descriptor, stored);

    descriptor
        .config_fields()
        .iter()
        .filter(|sectioned| sectioned.section == ConfigSection::Content)
        .filter_map(|sectioned| {
            let key = sectioned.field.key();
            let value = supplied
                .get(key)
                .filter(|value| !is_blank(value))
                .or_else(|| configured.get(key))?;
            Some((key.to_owned(), expanded(value, args)))
        })
        .collect()
}

fn is_blank(value: &Variant) -> bool {
    matches!(value, Variant::String(text) if text.is_empty())
}

fn expanded(value: &Variant, args: &ArgStack) -> Variant {
    match value {
        Variant::String(template) => Variant::String(args.interpolate(template)),
        other => other.clone(),
    }
}
