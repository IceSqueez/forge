use std::collections::HashMap;

use forge_types::{
    Action, ExecutionMode, SubActionStep, TriggerInstance, VariableStanding, VariantKind,
};

use crate::analyzer::{body_locals, names_on_some_triggers_only, step_outputs};
use crate::chain_steps::nested_chains;
use crate::registry::{SubActionRegistry, TriggerRegistry};
use crate::variables::declared_variables;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeOrigin {
    Trigger,
    EarlierStep,
    EnclosingBody,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopedVariable {
    pub name: String,
    pub kind: Option<VariantKind>,
    pub origin: ScopeOrigin,
    pub some_triggers_only: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ScopeLevel<'a> {
    pub chain: &'a [SubActionStep],
    pub step_index: usize,
}

#[derive(Default)]
struct ScopeList {
    variables: Vec<ScopedVariable>,
    index: HashMap<String, usize>,
}

impl ScopeList {
    fn add(&mut self, name: String, kind: Option<VariantKind>, origin: ScopeOrigin) {
        match self.index.get(&name) {
            Some(&at) => {
                let existing = &mut self.variables[at];
                existing.kind = kind.or(existing.kind);
                existing.origin = origin;
                existing.some_triggers_only = false;
            }
            None => {
                self.index.insert(name.clone(), self.variables.len());
                self.variables.push(ScopedVariable {
                    name,
                    kind,
                    origin,
                    some_triggers_only: false,
                });
            }
        }
    }

    fn add_step_outputs(&mut self, step: &SubActionStep, registry: &SubActionRegistry) {
        if !step.enabled {
            return;
        }
        for (name, kind) in step_outputs(step, registry) {
            self.add(name, kind, ScopeOrigin::EarlierStep);
        }
        for chain in nested_chains(step, registry) {
            for nested in &chain {
                self.add_step_outputs(nested, registry);
            }
        }
    }
}

pub fn variables_in_scope(
    action: &Action,
    levels: &[ScopeLevel<'_>],
    triggers: &[TriggerInstance],
    sub_registry: &SubActionRegistry,
    trigger_registry: &TriggerRegistry,
) -> Vec<ScopedVariable> {
    let mut scope = ScopeList::default();
    add_trigger_variables(&mut scope, triggers, trigger_registry);

    let isolated_root = action.execution_mode == ExecutionMode::RandomPick;
    let last = levels.len().saturating_sub(1);
    for (depth, level) in levels.iter().enumerate() {
        let sees_earlier_siblings = depth > 0 || !isolated_root;
        if sees_earlier_siblings {
            let earlier = &level.chain[..level.step_index.min(level.chain.len())];
            for step in earlier {
                scope.add_step_outputs(step, sub_registry);
            }
        }
        if depth < last
            && let Some(enclosing) = level.chain.get(level.step_index)
            && enclosing.enabled
        {
            for name in body_locals(enclosing, sub_registry) {
                scope.add(name, None, ScopeOrigin::EnclosingBody);
            }
        }
    }
    scope.variables
}

fn add_trigger_variables(
    scope: &mut ScopeList,
    triggers: &[TriggerInstance],
    registry: &TriggerRegistry,
) {
    let schemas: Vec<_> = triggers
        .iter()
        .filter_map(|instance| declared_variables(registry.get(&instance.kind_id)?))
        .collect();
    let some_only = names_on_some_triggers_only(&schemas);
    for variable in schemas.into_iter().flatten() {
        if matches!(variable.standing, VariableStanding::Legacy(_)) {
            continue;
        }
        let name = variable.declared.name;
        if scope.index.contains_key(&name) {
            continue;
        }
        let some_triggers_only = some_only.contains(&name);
        scope.add(name, Some(variable.declared.kind), ScopeOrigin::Trigger);
        if let Some(added) = scope.variables.last_mut() {
            added.some_triggers_only = some_triggers_only;
        }
    }
}
