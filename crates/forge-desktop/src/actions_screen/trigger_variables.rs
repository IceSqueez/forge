use forge_registry::TriggerKindDescriptor;
use forge_types::{DeclaredVariable, VariableStanding};

pub(super) struct ListedVariable {
    pub declared: DeclaredVariable,
    pub standing: VariableStanding,
}

pub(super) fn declared_variables(
    descriptor: &dyn TriggerKindDescriptor,
) -> Option<Vec<ListedVariable>> {
    if let Some(variables) = descriptor.variables() {
        return Some(
            variables
                .declarations()
                .into_iter()
                .map(|variable| ListedVariable {
                    declared: variable.declared.clone(),
                    standing: variable.standing,
                })
                .collect(),
        );
    }
    descriptor.output_schema().map(|schema| {
        schema
            .variables
            .into_iter()
            .map(|declared| ListedVariable {
                declared,
                standing: VariableStanding::EventSpecific,
            })
            .collect()
    })
}
