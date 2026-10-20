use super::*;
use crate::actions_screen::sub_action_modal::SubFormTarget;
use forge_registry::{ScopeLevel, ScopedVariable, variables_in_scope};

impl ScreenActionsView {
    pub(super) fn step_scope(&self, target: SubFormTarget) -> Vec<ScopedVariable> {
        let Some(detail) = &self.detail else {
            return Vec::new();
        };
        scope_at(
            detail,
            &self.nav_path,
            target,
            &self.sub_action_registry,
            &self.trigger_registry,
        )
    }
}

fn scope_at(
    detail: &ActionDetail,
    nav_path: &[nav::NavFrame],
    target: SubFormTarget,
    sub_registry: &SubActionRegistry,
    trigger_registry: &TriggerRegistry,
) -> Vec<ScopedVariable> {
    let root = &detail.action.sub_actions;
    let chains: Vec<Vec<SubActionStep>> = (0..=nav_path.len())
        .map(|depth| nav::resolve_chain(root, &nav_path[..depth]))
        .collect();
    let edited = match target {
        SubFormTarget::Edit(index) => index,
        SubFormTarget::Add => chains.last().map_or(0, Vec::len),
    };
    let levels: Vec<ScopeLevel<'_>> = chains
        .iter()
        .enumerate()
        .map(|(depth, chain)| ScopeLevel {
            chain,
            step_index: nav_path.get(depth).map_or(edited, |frame| frame.step_index),
        })
        .collect();
    variables_in_scope(
        &detail.action,
        &levels,
        &detail.trigger_instances,
        sub_registry,
        trigger_registry,
    )
}
