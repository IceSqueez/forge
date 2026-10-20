use std::collections::{BTreeSet, HashMap, HashSet};

use forge_types::{
    Action, ExecutionMode, IntegrationId, SubActionOutcome, SubActionStep, TemplatePiece,
    TemplatePieces, TriggerInstance, Variant, VariantKind, normalize_var_name, shared_date_pattern,
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
    DateFormatOnNonDatetime(String),
    InvalidDatePattern {
        name: String,
        pattern: String,
        reason: String,
    },
    InvalidStepConfig(String),
}

impl Finding {
    fn severity(&self) -> HealthSeverity {
        match self {
            Finding::UnknownVariable(_)
            | Finding::LastRunFailed(_)
            | Finding::InvalidDatePattern { .. }
            | Finding::InvalidStepConfig(_) => HealthSeverity::Red,
            Finding::DateFormatOnNonDatetime(_)
            | Finding::ProducedLater(_)
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
    datetime_reach: DatetimeReach,
}

#[derive(Default)]
struct DatetimeReach(HashMap<String, bool>);

impl DatetimeReach {
    fn note(&mut self, name: String, may_hold_datetime: bool) {
        *self.0.entry(name).or_insert(false) |= may_hold_datetime;
    }

    fn never_datetime(&self, name: &str) -> bool {
        self.0.get(name) == Some(&false)
    }
}

#[derive(Default)]
struct ConsumedReferences {
    names: HashSet<String>,
    formatted: BTreeSet<(String, String)>,
}

struct AnalyzeCtx<'a> {
    some_only: &'a HashSet<String>,
    produced_anywhere: &'a HashSet<String>,
    datetime_reach: &'a DatetimeReach,
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
    let mut seed = trigger_seed(triggers, trigger_registry);
    let isolated = action.concurrent || action.execution_mode == ExecutionMode::RandomPick;

    let mut produced_anywhere = HashSet::new();
    collect_produced(&action.sub_actions, sub_registry, &mut produced_anywhere);
    if let Some(seed) = seed.as_mut() {
        collect_produced_kinds(&action.sub_actions, sub_registry, &mut seed.datetime_reach);
    }

    let mut result: Vec<StepHealth> = Vec::with_capacity(action.sub_actions.len());

    match &seed {
        Some(seed) => {
            let ctx = AnalyzeCtx {
                some_only: &seed.some_only,
                produced_anywhere: &produced_anywhere,
                datetime_reach: &seed.datetime_reach,
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

    for (step, health) in action.sub_actions.iter().zip(result.iter_mut()) {
        collect_pattern_findings(step, sub_registry, &mut health.findings);
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
    let mut datetime_reach = DatetimeReach::default();
    for instance in triggers {
        let declared = declared_variables(registry.get(&instance.kind_id)?)?;
        let mut names = HashSet::with_capacity(declared.len());
        for variable in declared {
            datetime_reach.note(
                variable.declared.name.clone(),
                variable.declared.kind == VariantKind::Datetime,
            );
            names.insert(variable.declared.name);
        }
        schemas.push(names);
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
    Some(TriggerSeed {
        all,
        some_only,
        datetime_reach,
    })
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

    let consumed = consumed_references(step, ctx.sub_registry);
    let resolvable = |name: &str| scope.contains(name) || ctx.produced_anywhere.contains(name);
    let formatted_names: BTreeSet<&str> = consumed
        .formatted
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    for name in formatted_names {
        if resolvable(name) && ctx.datetime_reach.never_datetime(name) {
            findings.push(Finding::DateFormatOnNonDatetime(name.to_owned()));
        }
    }

    for var in consumed.names {
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

fn collect_produced_kinds(
    steps: &[SubActionStep],
    registry: &SubActionRegistry,
    out: &mut DatetimeReach,
) {
    for step in steps {
        if !step.enabled {
            continue;
        }
        if let Some(runner) = registry.get(&step.kind_id) {
            let io = runner.scope_io();
            for produced in io.produces {
                if let Some(name) = step
                    .config
                    .get(&produced.output_name_key)
                    .and_then(Variant::as_str)
                    .and_then(normalize_var_name)
                {
                    out.note(name, produced.kind == VariantKind::Datetime);
                }
            }
            for name in io.fixed_outputs.into_iter().chain(io.body_locals) {
                out.note(name, true);
            }
        }
        for chain in nested_chains(step, registry) {
            collect_produced_kinds(&chain, registry, out);
        }
    }
}

fn collect_pattern_findings(
    step: &SubActionStep,
    registry: &SubActionRegistry,
    out: &mut Vec<Finding>,
) {
    if !step.enabled {
        return;
    }
    for (name, pattern) in consumed_references(step, registry).formatted {
        if let Err(error) = shared_date_pattern(&pattern).as_ref() {
            out.push(Finding::InvalidDatePattern {
                name,
                pattern,
                reason: error.to_string(),
            });
        }
    }
    if let Some(runner) = registry.get(&step.kind_id)
        && runner.checks_config_while_editing()
        && let Err(error) = runner.validate_config(&step.config)
    {
        out.push(Finding::InvalidStepConfig(error.to_string()));
    }
    for chain in nested_chains(step, registry) {
        for nested in &chain {
            collect_pattern_findings(nested, registry, out);
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

fn consumed_references(step: &SubActionStep, registry: &SubActionRegistry) -> ConsumedReferences {
    let mut excluded: HashSet<&'static str> = HashSet::new();
    if let Some(runner) = registry.get(&step.kind_id) {
        collect_excluded_keys(&runner.config_fields(), &mut excluded);
    }
    let mut out = ConsumedReferences::default();
    for (key, value) in &step.config {
        if excluded.contains(key.as_str()) {
            continue;
        }
        collect_references_from_variant(value, &mut out);
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

fn collect_references_from_variant(value: &Variant, out: &mut ConsumedReferences) {
    match value {
        Variant::String(text) => collect_template_references(text, out),
        Variant::Array(items) => {
            for item in items {
                collect_references_from_variant(item, out);
            }
        }
        Variant::Object(map) => {
            for item in map.values() {
                collect_references_from_variant(item, out);
            }
        }
        _ => {}
    }
}

fn collect_template_references(template: &str, out: &mut ConsumedReferences) {
    for piece in TemplatePieces::new(template) {
        if let TemplatePiece::Reference { name, format, .. } = piece {
            out.names.insert(name.to_owned());
            if let Some(format) = format {
                out.formatted.insert((name.to_owned(), format.to_owned()));
            }
        }
    }
}
