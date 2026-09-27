use std::collections::BTreeMap;

use forge_types::Variant;

use crate::form::FormField;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormRefinement {
    pub selector_key: &'static str,
    pub schema_key: &'static str,
}

pub trait FormSchemaSource {
    fn fields_for(&self, schema_key: &str, selector_value: &str) -> Vec<FormField>;
}

pub fn refined_fields(
    refinement: FormRefinement,
    config: &BTreeMap<String, Variant>,
    source: &dyn FormSchemaSource,
) -> Vec<FormField> {
    let selector = match config.get(refinement.selector_key) {
        None | Some(Variant::Array(_) | Variant::Object(_)) => return Vec::new(),
        Some(scalar) => scalar.to_string(),
    };
    if selector.is_empty() {
        return Vec::new();
    }
    source.fields_for(refinement.schema_key, &selector)
}
