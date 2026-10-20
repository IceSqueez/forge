use std::collections::BTreeMap;

use forge_types::{Action, TriggerInstance, Variant, variable_references};

use crate::chain_steps::{CASE_CHAIN_KEY, STEP_CONFIG_KEY, STEP_KIND_KEY};
use crate::form::FormField;
use crate::references::codec::{UNTAGGED, decode};
use crate::references::listing::{ConfigReference, ReferenceField, ReferenceListing, UnknownKind};
use crate::references::location::ReferenceLocation;
use crate::references::namespace::{InternalNamespace, LOCAL_FILE_NAMESPACE, ReferenceNamespace};
use crate::refinement::{FormRefinement, FormSchemaSource, refined_fields};
use crate::registry::{SubActionRegistry, TriggerRegistry};

pub const ACTION_QUEUE_KEY: &str = "queue_id";
pub const ACTION_STEPS_KEY: &str = "sub_actions";
pub const TRIGGER_OVERRIDES_KEY: &str = "overrides";

type ConfigMap = BTreeMap<String, Variant>;

struct FieldDeclarations {
    fields: Vec<FormField>,
    declared: Vec<ReferenceField>,
    refinement: Option<FormRefinement>,
}

pub struct ReferenceCatalog<'a> {
    sub_actions: &'a SubActionRegistry,
    triggers: &'a TriggerRegistry,
    schema: Option<&'a dyn FormSchemaSource>,
}

impl<'a> ReferenceCatalog<'a> {
    pub fn new(sub_actions: &'a SubActionRegistry, triggers: &'a TriggerRegistry) -> Self {
        Self {
            sub_actions,
            triggers,
            schema: None,
        }
    }

    pub fn with_schema(self, schema: &'a dyn FormSchemaSource) -> Self {
        Self {
            schema: Some(schema),
            ..self
        }
    }

    pub fn sub_action_references(&self, kind_id: &str, config: &ConfigMap) -> ReferenceListing {
        let mut listing = ReferenceListing::default();
        self.walk_step(kind_id, config, &ReferenceLocation::root(), &mut listing);
        listing
    }

    pub fn trigger_references(&self, kind_id: &str, config: &ConfigMap) -> ReferenceListing {
        let mut listing = ReferenceListing::default();
        match self.trigger_declarations(kind_id) {
            Some(declarations) => self.walk_declarations(
                &declarations,
                config,
                &ReferenceLocation::root(),
                &mut listing,
            ),
            None => listing.unknown_kinds.push(UnknownKind {
                kind_id: kind_id.to_owned(),
                location: ReferenceLocation::root(),
            }),
        }
        listing
    }

    pub fn field_references(&self, fields: &[FormField], config: &ConfigMap) -> ReferenceListing {
        let mut listing = ReferenceListing::default();
        for field in fields {
            self.walk_field(field, config, &ReferenceLocation::root(), &mut listing);
        }
        listing
    }

    pub fn action_references(&self, action: &Action) -> ReferenceListing {
        let mut listing = ReferenceListing::default();
        listing.references.push(ConfigReference {
            namespace: ReferenceNamespace::Internal(InternalNamespace::Queue),
            value: action.queue_id.to_string(),
            templated: false,
            location: ReferenceLocation::root().key(ACTION_QUEUE_KEY),
            tag: UNTAGGED,
        });
        let steps = ReferenceLocation::root().key(ACTION_STEPS_KEY);
        for (index, step) in action.sub_actions.iter().enumerate() {
            let prefix = steps.index(index).key(STEP_CONFIG_KEY);
            listing.extend(
                self.sub_action_references(&step.kind_id, &step.config)
                    .within(&prefix),
            );
        }
        listing
    }

    pub fn trigger_instance_references(&self, trigger: &TriggerInstance) -> ReferenceListing {
        self.trigger_references(&trigger.kind_id, &trigger.overrides)
            .within(&ReferenceLocation::root().key(TRIGGER_OVERRIDES_KEY))
    }

    fn sub_action_declarations(&self, kind_id: &str) -> Option<FieldDeclarations> {
        self.sub_actions
            .get(kind_id)
            .map(|runner| FieldDeclarations {
                fields: runner.config_fields(),
                declared: runner.declared_references(),
                refinement: runner.config_refinement(),
            })
    }

    fn trigger_declarations(&self, kind_id: &str) -> Option<FieldDeclarations> {
        self.triggers
            .get(kind_id)
            .map(|descriptor| FieldDeclarations {
                fields: descriptor.config_fields(),
                declared: descriptor.declared_references(),
                refinement: descriptor.config_refinement(),
            })
    }

    fn walk_step(
        &self,
        kind_id: &str,
        config: &ConfigMap,
        at: &ReferenceLocation,
        listing: &mut ReferenceListing,
    ) {
        match self.sub_action_declarations(kind_id) {
            Some(declarations) => self.walk_declarations(&declarations, config, at, listing),
            None => listing.unknown_kinds.push(UnknownKind {
                kind_id: kind_id.to_owned(),
                location: at.clone(),
            }),
        }
    }

    fn walk_declarations(
        &self,
        declarations: &FieldDeclarations,
        config: &ConfigMap,
        at: &ReferenceLocation,
        listing: &mut ReferenceListing,
    ) {
        for field in &declarations.fields {
            self.walk_field(field, config, at, listing);
        }
        for declared in &declarations.declared {
            push_value(declared.options_key, declared.key, config, at, listing);
        }
        if let (Some(refinement), Some(schema)) = (declarations.refinement, self.schema) {
            for field in refined_fields(refinement, config, schema) {
                self.walk_field(&field, config, at, listing);
            }
        }
    }

    fn walk_field(
        &self,
        field: &FormField,
        config: &ConfigMap,
        at: &ReferenceLocation,
        listing: &mut ReferenceListing,
    ) {
        match field {
            FormField::DynamicSelect {
                key, options_key, ..
            } => push_value(options_key, key, config, at, listing),
            FormField::DependentSelect {
                key,
                options_prefix,
                ..
            } => push_value(options_prefix, key, config, at, listing),
            FormField::FilePicker { key, .. } => {
                push_value(LOCAL_FILE_NAMESPACE, key, config, at, listing);
            }
            FormField::Optional { inner, .. } => self.walk_field(inner, config, at, listing),
            FormField::SubChain { key, .. } => {
                self.walk_chain(config.get(*key), &at.key(key), listing);
            }
            FormField::CaseList { key, .. } => {
                let cases = config.get(*key).and_then(Variant::as_array);
                for (index, case) in cases.into_iter().flatten().enumerate() {
                    let chain = case.as_object().and_then(|case| case.get(CASE_CHAIN_KEY));
                    let case_at = at.key(key).index(index).key(CASE_CHAIN_KEY);
                    self.walk_chain(chain, &case_at, listing);
                }
            }
            FormField::Text { .. }
            | FormField::TextArea { .. }
            | FormField::Code { .. }
            | FormField::Integer { .. }
            | FormField::Slider { .. }
            | FormField::Toggle { .. }
            | FormField::DateTime { .. }
            | FormField::Select { .. }
            | FormField::UnitAmount { .. }
            | FormField::Duration { .. }
            | FormField::Swatch { .. } => {}
        }
    }

    fn walk_chain(
        &self,
        chain: Option<&Variant>,
        at: &ReferenceLocation,
        listing: &mut ReferenceListing,
    ) {
        let steps = chain.and_then(Variant::as_array);
        for (index, step) in steps.into_iter().flatten().enumerate() {
            let Some(step) = step.as_object() else {
                continue;
            };
            let Some(kind_id) = step.get(STEP_KIND_KEY).and_then(Variant::as_str) else {
                continue;
            };
            let empty = ConfigMap::new();
            let config = step
                .get(STEP_CONFIG_KEY)
                .and_then(Variant::as_object)
                .unwrap_or(&empty);
            self.walk_step(
                kind_id,
                config,
                &at.index(index).key(STEP_CONFIG_KEY),
                listing,
            );
        }
    }
}

fn push_value(
    options_key: &str,
    key: &str,
    config: &ConfigMap,
    at: &ReferenceLocation,
    listing: &mut ReferenceListing,
) {
    let Some(Variant::String(raw)) = config.get(key) else {
        return;
    };
    let Some(decoded) = decode(options_key, raw) else {
        return;
    };
    listing.references.push(ConfigReference {
        namespace: decoded.namespace,
        value: decoded.value.to_owned(),
        templated: variable_references(raw).next().is_some(),
        location: at.key(key),
        tag: decoded.tag,
    });
}
