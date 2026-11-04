use std::collections::BTreeMap;

use forge_registry::{FormField, FormSchemaSource};

use crate::catalog::descriptor::OverlayKindDescriptor;
use crate::catalog::registry::OverlayKindRegistry;
use crate::render::show_media::show_media_fields;

pub const CONTENT_SCHEMA_KEY: &str = "overlay.content_fields";

pub fn content_fields(descriptor: &dyn OverlayKindDescriptor) -> Vec<FormField> {
    let mut fields: Vec<FormField> = descriptor
        .config_fields()
        .into_iter()
        .filter(|sectioned| sectioned.delivered)
        .map(|sectioned| sectioned.field)
        .collect();
    fields.extend(show_media_fields(descriptor));
    fields
}

pub struct ContentSchemaByIdentity<'a> {
    kinds: &'a OverlayKindRegistry,
    kind_by_identity: BTreeMap<String, String>,
}

impl<'a> ContentSchemaByIdentity<'a> {
    pub fn new(
        kinds: &'a OverlayKindRegistry,
        kind_by_identity: impl IntoIterator<Item = (String, String)>,
    ) -> Self {
        Self {
            kinds,
            kind_by_identity: kind_by_identity.into_iter().collect(),
        }
    }
}

impl FormSchemaSource for ContentSchemaByIdentity<'_> {
    fn fields_for(&self, schema_key: &str, selector_value: &str) -> Vec<FormField> {
        if schema_key != CONTENT_SCHEMA_KEY {
            return Vec::new();
        }
        self.kind_by_identity
            .get(selector_value.trim())
            .and_then(|kind_id| self.kinds.get(kind_id))
            .map(content_fields)
            .unwrap_or_default()
    }
}
