use std::collections::{BTreeSet, HashMap, HashSet};

use forge_types::{
    Action, ExecutionMode, IntegrationId, SubActionOutcome, SubActionStep, TemplatePiece,
    TemplatePieces, TriggerInstance, Variant, VariantKind, normalize_var_name, shared_date_pattern,
};

use crate::config_problem::ConfigProblem;
use crate::form::FormField;
use crate::registry::{SubActionRegistry, TriggerRegistry};
use crate::step_tree::{StepNode, StepReach, UndecodableReason, nested_chains, walk_steps};
use crate::trigger_seed::{TriggerCoverage, TriggerSeed};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum HealthSeverity {
    Green,
    Yellow,
    Red,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Finding {
    UnknownVariable(String),
    UnverifiableVariable(String),
    ProducedLater(String),
    IsolatedSibling(String),
    SomeTriggersOnly(String),
    LastRunFailed(String),
    IntegrationDisabled(IntegrationId),
    IntegrationFailed(IntegrationId),
    DateFormatOnNonDatetime(String),
    InvalidDatePattern {
        name: String,
        pattern: String,
        reason: String,
    },
    InvalidStepConfig(ConfigProblem),
    UndecodableStep {
        position: usize,
        reason: UndecodableReason,
    },
}

impl Finding {
    fn severity(&self) -> HealthSeverity {
        match self {
            Finding::UnknownVariable(_)
            | Finding::LastRunFailed(_)
            | Finding::InvalidDatePattern { .. }
            | Finding::InvalidStepConfig(_)
            | Finding::UndecodableStep { .. } => HealthSeverity::Red,
            Finding::DateFormatOnNonDatetime(_)
            | Finding::ProducedLater(_)
            | Finding::IsolatedSibling(_)
            | Finding::SomeTriggersOnly(_)
            | Finding::UnverifiableVariable(_)
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
    let mut first = None;
    walk_steps(
        std::slice::from_ref(step),
        registry,
        StepReach::Enabled,
        &mut |node| {
            if first.is_none()
                && let StepNode::Step(step) = node
                && let Some(owner) = registry.owning_integration(&step.kind_id)
                && gated.contains(owner)
            {
                first = Some(owner.clone());
            }
        },
    );
    first
}

struct VariableSeed {
    all: HashSet<String>,
    some_only: HashSet<String>,
    datetime_reach: DatetimeReach,
    unresolved: fn(String) -> Finding,
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
    unresolved: fn(String) -> Finding,
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
    let mut seed = variable_seed(&TriggerSeed::build(triggers, trigger_registry));
    let isolated = action.execution_mode == ExecutionMode::RandomPick;

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
                unresolved: seed.unresolved,
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

fn variable_seed(seed: &TriggerSeed) -> Option<VariableSeed> {
    let unresolved: fn(String) -> Finding = match seed.coverage() {
        TriggerCoverage::NoTriggers => return None,
        TriggerCoverage::Declared => Finding::UnknownVariable,
        TriggerCoverage::PartlyUnknown { .. } => Finding::UnverifiableVariable,
    };
    let mut all = HashSet::new();
    let mut some_only = HashSet::new();
    let mut datetime_reach = DatetimeReach::default();
    for variable in seed.variables() {
        datetime_reach.note(variable.name.clone(), variable.may_hold_datetime);
        if variable.some_triggers_only {
            some_only.insert(variable.name.clone());
        }
        all.insert(variable.name.clone());
    }
    Some(VariableSeed {
        all,
        some_only,
        datetime_reach,
        unresolved,
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
            findings.push((ctx.unresolved)(var));
        }
    }

    let mut produced_by_step = step_output_names(step, ctx.sub_registry);
    let body_locals = body_locals(step, ctx.sub_registry);
    for chain in nested_chains(step, ctx.sub_registry) {
        let mut body_scope = scope.clone();
        body_scope.extend(body_locals.iter().cloned());
        let mut body_produced_before = produced_before.clone();
        body_produced_before.extend(body_locals.iter().cloned());
        let (body_findings, body_produced) =
            walk_chain_seq(ctx, chain.steps(), body_scope, body_produced_before);
        findings.extend(body_findings);
        produced_by_step.extend(body_produced);
    }

    (findings, produced_by_step)
}

fn walk_chain_seq<'s>(
    ctx: &AnalyzeCtx,
    steps: impl Iterator<Item = &'s SubActionStep>,
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
    walk_steps(steps, registry, StepReach::Enabled, &mut |node| {
        if let StepNode::Step(step) = node {
            out.extend(step_output_names(step, registry));
        }
    });
}

fn collect_produced_kinds(
    steps: &[SubActionStep],
    registry: &SubActionRegistry,
    out: &mut DatetimeReach,
) {
    walk_steps(steps, registry, StepReach::Enabled, &mut |node| {
        if let StepNode::Step(step) = node {
            for (name, kind) in step_outputs(step, registry) {
                out.note(name, kind.is_none_or(|kind| kind == VariantKind::Datetime));
            }
            for name in body_locals(step, registry) {
                out.note(name, true);
            }
        }
    });
}

fn collect_pattern_findings(
    step: &SubActionStep,
    registry: &SubActionRegistry,
    out: &mut Vec<Finding>,
) {
    walk_steps(
        std::slice::from_ref(step),
        registry,
        StepReach::Enabled,
        &mut |node| match node {
            StepNode::Step(step) => step_pattern_findings(step, registry, out),
            StepNode::Undecodable(undecodable) => out.push(Finding::UndecodableStep {
                position: undecodable.position,
                reason: undecodable.reason,
            }),
        },
    );
}

fn step_pattern_findings(
    step: &SubActionStep,
    registry: &SubActionRegistry,
    out: &mut Vec<Finding>,
) {
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
        out.push(Finding::InvalidStepConfig(error.into_config_problem()));
    }
}

pub(crate) fn step_outputs(
    step: &SubActionStep,
    registry: &SubActionRegistry,
) -> Vec<(String, Option<VariantKind>)> {
    let Some(runner) = registry.get(&step.kind_id) else {
        return Vec::new();
    };
    let io = runner.scope_io();
    let mut out = Vec::with_capacity(io.produces.len() + io.fixed_outputs.len());
    for produced in io.produces {
        if let Some(name) = step
            .config
            .get(&produced.output_name_key)
            .and_then(Variant::as_str)
            .and_then(normalize_var_name)
        {
            out.push((name, Some(produced.kind)));
        }
    }
    out.extend(io.fixed_outputs.into_iter().map(|name| (name, None)));
    out
}

pub(crate) fn body_locals(step: &SubActionStep, registry: &SubActionRegistry) -> Vec<String> {
    registry
        .get(&step.kind_id)
        .map(|runner| runner.scope_io().body_locals)
        .unwrap_or_default()
}

fn step_output_names(step: &SubActionStep, registry: &SubActionRegistry) -> HashSet<String> {
    step_outputs(step, registry)
        .into_iter()
        .map(|(name, _)| name)
        .collect()
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
