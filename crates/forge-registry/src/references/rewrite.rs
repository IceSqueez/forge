use std::collections::BTreeMap;

use forge_types::{Action, QueueId, TriggerInstance, Variant};

use crate::error::RegistryError;
use crate::form::FormField;
use crate::references::catalog::ReferenceCatalog;
use crate::references::listing::ConfigReference;
use crate::references::namespace::InternalNamespace;
use crate::references::remap::ReferenceRemap;

type ConfigMap = BTreeMap<String, Variant>;

impl ReferenceCatalog<'_> {
    pub fn rewrite_sub_action_config(
        &self,
        kind_id: &str,
        config: &mut ConfigMap,
        remap: &ReferenceRemap,
    ) -> usize {
        let listing = self.sub_action_references(kind_id, config);
        apply(&listing.references, config, remap)
    }

    pub fn rewrite_trigger_config(
        &self,
        kind_id: &str,
        config: &mut ConfigMap,
        remap: &ReferenceRemap,
    ) -> usize {
        let listing = self.trigger_references(kind_id, config);
        apply(&listing.references, config, remap)
    }

    pub fn rewrite_field_config(
        &self,
        fields: &[FormField],
        config: &mut ConfigMap,
        remap: &ReferenceRemap,
    ) -> usize {
        let listing = self.field_references(fields, config);
        apply(&listing.references, config, remap)
    }

    pub fn rewrite_action(
        &self,
        action: &mut Action,
        remap: &ReferenceRemap,
    ) -> Result<usize, RegistryError> {
        let mut rewritten = 0;
        let queue = InternalNamespace::Queue;
        if let Some(to) = remap.get(queue, &action.queue_id.to_string()) {
            action.queue_id = to
                .parse::<QueueId>()
                .map_err(|_| RegistryError::InvalidRemap {
                    namespace: queue.as_str().to_owned(),
                    value: to.to_owned(),
                })?;
            rewritten += 1;
        }
        for step in &mut action.sub_actions {
            rewritten += self.rewrite_sub_action_config(&step.kind_id, &mut step.config, remap);
        }
        Ok(rewritten)
    }

    pub fn rewrite_trigger_instance(
        &self,
        trigger: &mut TriggerInstance,
        remap: &ReferenceRemap,
    ) -> usize {
        self.rewrite_trigger_config(&trigger.kind_id, &mut trigger.overrides, remap)
    }
}

fn apply(references: &[ConfigReference], config: &mut ConfigMap, remap: &ReferenceRemap) -> usize {
    let mut rewritten = 0;
    for reference in references.iter().filter(|reference| !reference.templated) {
        let Some(namespace) = reference.namespace.internal() else {
            continue;
        };
        let Some(to) = remap.get(namespace, &reference.value) else {
            continue;
        };
        let Some(slot) = reference.location.slot_mut(config) else {
            continue;
        };
        *slot = Variant::String(format!("{}{to}", reference.tag));
        rewritten += 1;
    }
    rewritten
}
