use std::collections::HashMap;

use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_types::{Action, IntegrationId, SubActionStep, TriggerInstance};

use crate::actions_screen::nested_chains;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IntegrationReferences {
    pub actions: Vec<String>,
    pub triggers: usize,
}

impl IntegrationReferences {
    pub fn total(&self) -> usize {
        self.actions.len() + self.triggers
    }
}

pub fn tally_references(
    actions: &[Action],
    triggers: &[TriggerInstance],
    sub_actions: &SubActionRegistry,
    trigger_kinds: &TriggerRegistry,
) -> HashMap<IntegrationId, IntegrationReferences> {
    let mut tally: HashMap<IntegrationId, IntegrationReferences> = HashMap::new();
    for action in actions {
        let mut owners: Vec<IntegrationId> = Vec::new();
        collect_owners(&action.sub_actions, sub_actions, &mut owners);
        for owner in owners {
            tally
                .entry(owner)
                .or_default()
                .actions
                .push(action.name.clone());
        }
    }
    for trigger in triggers {
        if let Some(owner) = trigger_kinds.owning_integration(&trigger.kind_id) {
            tally.entry(owner.clone()).or_default().triggers += 1;
        }
    }
    tally
}

fn collect_owners(
    steps: &[SubActionStep],
    registry: &SubActionRegistry,
    owners: &mut Vec<IntegrationId>,
) {
    for step in steps {
        if !step.enabled {
            continue;
        }
        if let Some(owner) = registry.owning_integration(&step.kind_id)
            && !owners.contains(owner)
        {
            owners.push(owner.clone());
        }
        for chain in nested_chains(step, registry) {
            collect_owners(&chain, registry, owners);
        }
    }
}
