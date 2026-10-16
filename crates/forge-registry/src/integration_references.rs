use std::collections::HashMap;

use forge_types::{Action, ActionId, IntegrationId, SubActionStep, TriggerInstance, Variant};

use crate::chain_steps::nested_chains;
use crate::registry::{SubActionRegistry, TriggerRegistry};

const SERVICE_FILTER_KEY: &str = "provider";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencingAction {
    pub id: ActionId,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IntegrationReferences {
    pub actions: Vec<ReferencingAction>,
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
    unowned_kind_candidates: &dyn Fn(&str) -> Vec<IntegrationId>,
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
                .push(ReferencingAction {
                    id: action.id,
                    name: action.name.clone(),
                });
        }
    }
    for trigger in triggers {
        for owner in trigger_owners(trigger, trigger_kinds, unowned_kind_candidates) {
            tally.entry(owner).or_default().triggers += 1;
        }
    }
    tally
}

fn trigger_owners(
    trigger: &TriggerInstance,
    trigger_kinds: &TriggerRegistry,
    unowned_kind_candidates: &dyn Fn(&str) -> Vec<IntegrationId>,
) -> Vec<IntegrationId> {
    if let Some(owner) = trigger_kinds.owning_integration(&trigger.kind_id) {
        return vec![owner.clone()];
    }
    let filtered = match trigger.overrides.get(SERVICE_FILTER_KEY) {
        Some(Variant::String(service)) if !service.trim().is_empty() => Some(service.trim()),
        _ => None,
    };
    unowned_kind_candidates(&trigger.kind_id)
        .into_iter()
        .filter(|candidate| filtered.is_none_or(|service| candidate.as_str() == service))
        .collect()
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
