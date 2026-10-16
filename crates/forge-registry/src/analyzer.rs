use std::collections::HashSet;

use forge_types::{
    Action, ExecutionMode, IntegrationId, SubActionOutcome, SubActionStep, TriggerInstance,
    Variant, normalize_var_name,
};

use crate::chain_steps::nested_chains;
use crate::flow_kinds::{BREAK_LOOP_KIND_ID, CONTINUE_LOOP_KIND_ID, LOOP_KIND_ID, STOP_KIND_ID};
use crate::form::FormField;
use crate::registry::{SubActionRegistry, TriggerRegistry};
use crate::variables::declared_variables;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum HealthSeverity {
    Green,
    Yellow,
    Red,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Finding {
    UnknownVariable(String),
    ProducedLater(String),
    IsolatedSibling(String),
    SomeTriggersOnly(String),
    LastRunFailed(String),
    ControlFlowInConcurrentAction,
    IntegrationDisabled(IntegrationId),
    IntegrationFailed(IntegrationId),
}

impl Finding {
    fn severity(&self) -> HealthSeverity {
        match self {
            Finding::UnknownVariable(_) | Finding::LastRunFailed(_) => HealthSeverity::Red,
            Finding::ProducedLater(_)
            | Finding::IsolatedSibling(_)
            | Finding::SomeTriggersOnly(_)
            | Finding::ControlFlowInConcurrentAction
            | Finding::IntegrationDisabled(_)
            | Finding::IntegrationFailed(_) => HealthSeverity::Yellow,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StepHealth {
    pub findings: Vec<Finding>,
}

impl StepHealth {
    pub fn severity(&self) -> HealthSeverity {
        self.findings
            .iter()
            .map(Finding::severity)
            .max()
            .unwrap_or(HealthSeverity::Green)
    }

    pub fn disabled_integration(&self) -> Option<&IntegrationId> {
        self.findings.iter().find_map(|finding| match finding {
            Finding::IntegrationDisabled(id) => Some(id),
            _ => None,
        })
    }

    pub fn failed_integration(&self) -> Option<&IntegrationId> {
        self.findings.iter().find_map(|finding| match finding {
            Finding::IntegrationFailed(id) => Some(id),
            _ => None,
        })
    }
}

pub fn flag_switched_off(
    steps: &[SubActionStep],
    registry: &SubActionRegistry,
    switched_off: &HashSet<IntegrationId>,
    health: &mut [StepHealth],
) {
    flag_gated(
        steps,
        registry,
        switched_off,
        health,
        Finding::IntegrationDisabled,
    );
}

pub fn flag_failed(
    steps: &[SubActionStep],
    registry: &SubActionRegistry,
    failed: &HashSet<IntegrationId>,
    health: &mut [StepHealth],
) {
    flag_gated(steps, registry, failed, health, Finding::IntegrationFailed);
}

fn flag_gated(
    steps: &[SubActionStep],
    registry: &SubActionRegistry,
    gated: &HashSet<IntegrationId>,
    health: &mut [StepHealth],
    finding: fn(IntegrationId) -> Finding,
) {
    for (step, health) in steps.iter().zip(health.iter_mut()) {
        if let Some(owner) = gated_owner(step, registry, gated) {
            health.findings.push(finding(owner));
        }
    }
}

fn gated_owner(
    step: &SubActionStep,
    registry: &SubActionRegistry,
    gated: &HashSet<IntegrationId>,
) -> Option<IntegrationId> {
    if !step.enabled {
        return None;
    }
    if let Some(owner) = registry.owning_integration(&step.kind_id)
        && gated.contains(owner)
    {
        return Some(owner.clone());
    }
    nested_chains(step, registry).iter().find_map(|chain| {
        chain
            .iter()
            .find_map(|nested| gated_owner(nested, registry, gated))
    })
}

struct TriggerSeed {
    all: HashSet<String>,
    some_only: HashSet<String>,
}

struct AnalyzeCtx<'a> {
    some_only: &'a HashSet<String>,
    produced_anywhere: &'a HashSet<String>,
    sub_registry: &'a SubActionRegistry,
}

#[derive(Clone, Copy)]
enum DirectMode {
    Sequential,
    Isolated,
}

pub fn analyze(
    action: &Action,
    triggers: &[TriggerInstance],
    last_step_outcomes: &[Option<SubActionOutcome>],
    sub_registry: &SubActionRegistry,
    trigger_registry: &TriggerRegistry,
) -> Vec<StepHealth> {
    let seed = trigger_seed(triggers, trigger_registry);
    let isolated = action.concurrent || action.execution_mode == ExecutionMode::RandomPick;

    let mut produced_anywhere = HashSet::new();
    collect_produced(&action.sub_actions, sub_registry, &mut produced_anywhere);

    let mut result: Vec<StepHealth> = Vec::with_capacity(action.sub_actions.len());

    match &seed {
        Some(seed) => {
            let ctx = AnalyzeCtx {
                some_only: &seed.some_only,
                produced_anywhere: &produced_anywhere,
                sub_registry,
            };
            if isolated {
                for step in &action.sub_actions {
                    let (findings, _) =
                        analyze_step(&ctx, step, &seed.all, &HashSet::new(), DirectMode::Isolated);
                    result.push(StepHealth { findings });
                }
            } else {
                let mut available = seed.all.clone();
                let mut produced_before = HashSet::new();
                for step in &action.sub_actions {
                    let (findings, produced) = analyze_step(
                        &ctx,
                        step,
                        &available,
                        &produced_before,
                        DirectMode::Sequential,
                    );
                    result.push(StepHealth { findings });
                    available.extend(produced.iter().cloned());
                    produced_before.extend(produced);
                }
            }
        }
        None => result.resize(action.sub_actions.len(), StepHealth::default()),
    }

    if action.concurrent {
        for (step, health) in action.sub_actions.iter().zip(result.iter_mut()) {
            if step_holds_unguarded_control_flow(step, sub_registry, false) {
                health.findings.push(Finding::ControlFlowInConcurrentAction);
            }
        }
    }

    for (i, health) in result.iter_mut().enumerate() {
        if let Some(Some(reason)) = last_step_outcomes
            .get(i)
            .map(|outcome| outcome.as_ref().and_then(SubActionOutcome::failure_reason))
        {
            health.findings.push(Finding::LastRunFailed(reason));
        }
    }

    result
}

fn is_unguarded_control_flow(kind_id: &str, enclosed_by_loop: bool) -> bool {
    match kind_id {
        BREAK_LOOP_KIND_ID | CONTINUE_LOOP_KIND_ID => !enclosed_by_loop,
        STOP_KIND_ID => true,
        _ => false,
    }
}

fn step_holds_unguarded_control_flow(
    step: &SubActionStep,
    registry: &SubActionRegistry,
    enclosed_by_loop: bool,
) -> bool {
    if !step.enabled {
        return false;
    }
    if is_unguarded_control_flow(&step.kind_id, enclosed_by_loop) {
        return true;
    }
    let child_enclosed_by_loop = enclosed_by_loop || step.kind_id == LOOP_KIND_ID;
    nested_chains(step, registry).iter().any(|chain| {
        chain
            .iter()
            .any(|child| step_holds_unguarded_control_flow(child, registry, child_enclosed_by_loop))
    })
}

fn trigger_seed(triggers: &[TriggerInstance], registry: &TriggerRegistry) -> Option<TriggerSeed> {
    if triggers.is_empty() {
        return None;
    }
    let mut schemas: Vec<HashSet<String>> = Vec::with_capacity(triggers.len());
    for instance in triggers {
        let declared = declared_variables(registry.get(&instance.kind_id)?)?;
        schemas.push(
            declared
                .into_iter()
                .map(|variable| variable.declared.name)
                .collect(),
        );
    }
    let mut all = HashSet::new();
    for names in &schemas {
        all.extend(names.iter().cloned());
    }
    let mut intersection = all.clone();
    for names in &schemas {
        intersection.retain(|name| names.contains(name));
    }
    let some_only = all.difference(&intersection).cloned().collect();
    Some(TriggerSeed { all, some_only })
}

fn analyze_step(
    ctx: &AnalyzeCtx,
    step: &SubActionStep,
    scope: &HashSet<String>,
    produced_before: &HashSet<String>,
    direct_mode: DirectMode,
) -> (Vec<Finding>, HashSet<String>) {
    let mut findings = Vec::new();
    if !step.enabled {
        return (findings, HashSet::new());
    }

    for var in consumed_vars(step, ctx.sub_registry) {
        if scope.contains(&var) {
            if ctx.some_only.contains(&var) && !produced_before.contains(&var) {
                findings.push(Finding::SomeTriggersOnly(var));
            }
        } else if ctx.produced_anywhere.contains(&var) {
            match direct_mode {
                DirectMode::Sequential => findings.push(Finding::ProducedLater(var)),
                DirectMode::Isolated => findings.push(Finding::IsolatedSibling(var)),
            }
        } else {
            findings.push(Finding::UnknownVariable(var));
        }
    }

    let mut produced_by_step = step_output_names(step, ctx.sub_registry);
    let body_locals = ctx
        .sub_registry
        .get(&step.kind_id)
        .map(|runner| runner.scope_io().body_locals)
        .unwrap_or_default();
    for chain in nested_chains(step, ctx.sub_registry) {
        let mut body_scope = scope.clone();
        body_scope.extend(body_locals.iter().cloned());
        let mut body_produced_before = produced_before.clone();
        body_produced_before.extend(body_locals.iter().cloned());
        let (body_findings, body_produced) =
            walk_chain_seq(ctx, &chain, body_scope, body_produced_before);
        findings.extend(body_findings);
        produced_by_step.extend(body_produced);
    }

    (findings, produced_by_step)
}

fn walk_chain_seq(
    ctx: &AnalyzeCtx,
    steps: &[SubActionStep],
    mut scope: HashSet<String>,
    mut produced_before: HashSet<String>,
) -> (Vec<Finding>, HashSet<String>) {
    let mut findings = Vec::new();
    let mut produced_here = HashSet::new();
    for step in steps {
        let (step_findings, produced) =
            analyze_step(ctx, step, &scope, &produced_before, DirectMode::Sequential);
        findings.extend(step_findings);
        scope.extend(produced.iter().cloned());
        produced_before.extend(produced.iter().cloned());
        produced_here.extend(produced);
    }
    (findings, produced_here)
}

fn collect_produced(
    steps: &[SubActionStep],
    registry: &SubActionRegistry,
    out: &mut HashSet<String>,
) {
    for step in steps {
        if !step.enabled {
            continue;
        }
        out.extend(step_output_names(step, registry));
        for chain in nested_chains(step, registry) {
            collect_produced(&chain, registry, out);
        }
    }
}

fn step_output_names(step: &SubActionStep, registry: &SubActionRegistry) -> HashSet<String> {
    let mut out = HashSet::new();
    let Some(runner) = registry.get(&step.kind_id) else {
        return out;
    };
    let io = runner.scope_io();
    for produced in io.produces {
        if let Some(name) = step
            .config
            .get(&produced.output_name_key)
            .and_then(Variant::as_str)
            .and_then(normalize_var_name)
        {
            out.insert(name);
        }
    }
    out.extend(io.fixed_outputs);
    out
}

fn consumed_vars(step: &SubActionStep, registry: &SubActionRegistry) -> HashSet<String> {
    let mut excluded: HashSet<&'static str> = HashSet::new();
    if let Some(runner) = registry.get(&step.kind_id) {
        collect_excluded_keys(&runner.config_fields(), &mut excluded);
    }
    let mut out = HashSet::new();
    for (key, value) in &step.config {
        if excluded.contains(key.as_str()) {
            continue;
        }
        collect_tokens_from_variant(value, &mut out);
    }
    out
}

fn collect_excluded_keys(fields: &[FormField], out: &mut HashSet<&'static str>) {
    for field in fields {
        match field {
            FormField::Code { key, .. }
            | FormField::SubChain { key, .. }
            | FormField::CaseList { key, .. } => {
                out.insert(key);
            }
            FormField::Optional { inner, .. } => {
                collect_excluded_keys(std::slice::from_ref(inner), out);
            }
            _ => {}
        }
    }
}

fn collect_tokens_from_variant(value: &Variant, out: &mut HashSet<String>) {
    match value {
        Variant::String(text) => extract_tokens(text, out),
        Variant::Array(items) => {
            for item in items {
                collect_tokens_from_variant(item, out);
            }
        }
        Variant::Object(map) => {
            for item in map.values() {
                collect_tokens_from_variant(item, out);
            }
        }
        _ => {}
    }
}

fn extract_tokens(template: &str, out: &mut HashSet<String>) {
    let mut chars = template.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            continue;
        }
        let mut key = String::new();
        let mut closed = false;
        for inner in chars.by_ref() {
            if inner == '%' {
                closed = true;
                break;
            }
            key.push(inner);
        }
        if !closed {
            break;
        }
        let name = key.trim();
        if !name.is_empty() {
            out.insert(name.to_owned());
        }
    }
}
