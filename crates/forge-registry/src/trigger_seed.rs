use std::collections::{HashMap, HashSet};

use forge_types::{TriggerInstance, VariantKind};

use crate::registry::TriggerRegistry;
use crate::variables::TriggerVariable;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerCoverage {
    NoTriggers,
    Declared,
    PartlyUnknown { undeclared_kinds: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeedVariable {
    pub name: String,
    pub kind: VariantKind,
    pub superseded: bool,
    pub some_triggers_only: bool,
    pub may_hold_datetime: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerSeed {
    variables: Vec<SeedVariable>,
    coverage: TriggerCoverage,
}

impl TriggerSeed {
    pub fn build(triggers: &[TriggerInstance], registry: &TriggerRegistry) -> Self {
        if triggers.is_empty() {
            return TriggerSeed {
                variables: Vec::new(),
                coverage: TriggerCoverage::NoTriggers,
            };
        }

        let mut declared: Vec<&[TriggerVariable]> = Vec::with_capacity(triggers.len());
        let mut undeclared_kinds: Vec<String> = Vec::new();
        for instance in triggers {
            match registry.declared_variables(&instance.kind_id) {
                Some(variables) => declared.push(variables),
                None if !undeclared_kinds.contains(&instance.kind_id) => {
                    undeclared_kinds.push(instance.kind_id.clone());
                }
                None => {}
            }
        }

        let coverage = if undeclared_kinds.is_empty() {
            TriggerCoverage::Declared
        } else {
            TriggerCoverage::PartlyUnknown { undeclared_kinds }
        };
        let every_name_partial = matches!(coverage, TriggerCoverage::PartlyUnknown { .. });
        let some_only = names_on_some_triggers_only(&declared);

        let (current, superseded): (Vec<&TriggerVariable>, Vec<&TriggerVariable>) = declared
            .iter()
            .flat_map(|variables| variables.iter())
            .partition(|variable| variable.standing.superseded_by().is_none());

        let mut variables: Vec<SeedVariable> = Vec::new();
        let mut index: HashMap<&str, usize> = HashMap::new();
        for variable in current.into_iter().chain(superseded) {
            let name = variable.declared.name.as_str();
            let is_datetime = variable.declared.kind == VariantKind::Datetime;
            match index.get(name) {
                Some(&at) => variables[at].may_hold_datetime |= is_datetime,
                None => {
                    index.insert(name, variables.len());
                    variables.push(SeedVariable {
                        name: name.to_owned(),
                        kind: variable.declared.kind,
                        superseded: variable.standing.superseded_by().is_some(),
                        some_triggers_only: every_name_partial || some_only.contains(name),
                        may_hold_datetime: is_datetime,
                    });
                }
            }
        }

        TriggerSeed {
            variables,
            coverage,
        }
    }

    pub fn variables(&self) -> &[SeedVariable] {
        &self.variables
    }

    pub fn coverage(&self) -> &TriggerCoverage {
        &self.coverage
    }
}

fn names_on_some_triggers_only<'a>(declared: &[&'a [TriggerVariable]]) -> HashSet<&'a str> {
    let sets: Vec<HashSet<&str>> = declared
        .iter()
        .map(|variables| {
            variables
                .iter()
                .map(|variable| variable.declared.name.as_str())
                .collect()
        })
        .collect();
    sets.iter()
        .flatten()
        .filter(|name| sets.iter().any(|other| !other.contains(*name)))
        .copied()
        .collect()
}
