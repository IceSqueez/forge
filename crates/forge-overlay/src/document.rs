use serde::Serialize;
use serde_json::{Map, Value};

use crate::config::effective_overlay_config;
use crate::descriptor::OverlayKindDescriptor;
use crate::error::OverlayError;
use crate::instance::OverlayInstance;
use crate::materialize::GENERATOR_VERSION;

pub const DOCUMENT_VERSION: u32 = 1;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigDocument<'a> {
    document_version: u32,
    generator_version: u32,
    overlay_id: &'a str,
    display_name: &'a str,
    kind_id: &'a str,
    config_schema_version: u32,
    config: Map<String, Value>,
}

/// Emits the effective config as plain JSON, never the tagged [`forge_types::Variant`] form, because the page consumes it directly.
pub fn config_document(
    instance: &OverlayInstance,
    descriptor: &dyn OverlayKindDescriptor,
) -> Result<String, OverlayError> {
    let effective = effective_overlay_config(descriptor, &instance.config);
    let config = effective
        .iter()
        .map(|(key, value)| (key.clone(), value.to_plain_json()))
        .collect();

    let document = ConfigDocument {
        document_version: DOCUMENT_VERSION,
        generator_version: GENERATOR_VERSION,
        overlay_id: instance.id.as_str(),
        display_name: instance.display_name.as_str(),
        kind_id: descriptor.id(),
        config_schema_version: descriptor.config_schema_version(),
        config,
    };

    Ok(serde_json::to_string_pretty(&document)?)
}
