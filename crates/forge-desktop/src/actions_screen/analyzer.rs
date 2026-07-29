use std::collections::HashSet;

use forge_registry::{FormField, SubActionRegistry, TriggerRegistry};
use forge_types::{
    Action, ExecutionMode, SubActionOutcome, SubActionStep, TriggerInstance, Variant,
    normalize_var_name,
};

use super::nav;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(super) enum HealthSeverity {
    Green,
    Yellow,
    Red,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Finding {
    UnknownVariable(String),
    ProducedLater(String),
    IsolatedSibling(String),
    SomeTriggersOnly(String),
    LastRunFailed(String),
}

impl Finding {
    fn severity(&self) -> HealthSeverity {
        match self {
            Finding::UnknownVariable(_) | Finding::LastRunFailed(_) => HealthSeverity::Red,
            Finding::ProducedLater(_)
            | Finding::IsolatedSibling(_)
            | Finding::SomeTriggersOnly(_) => HealthSeverity::Yellow,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct StepHealth {
    pub findings: Vec<Finding>,
}

impl StepHealth {
    pub(super) fn severity(&self) -> HealthSeverity {
        self.findings
            .iter()
            .map(Finding::severity)
            .max()
            .unwrap_or(HealthSeverity::Green)
    }
}

const OVERLAY_SEND_KIND: &str = "overlay.send";
const OVERLAY_TARGET_KEY: &str = "overlay_id";

pub(super) fn sends_order_sensitive_overlay(
    steps: &[SubActionStep],
    registry: &SubActionRegistry,
    order_sensitive: &dyn Fn(&str) -> bool,
) -> bool {
    steps.iter().any(|step| {
        if !step.enabled {
            return false;
        }
        let hits = step.kind_id == OVERLAY_SEND_KIND
            && step
                .config
                .get(OVERLAY_TARGET_KEY)
                .and_then(Variant::as_str)
                .is_some_and(order_sensitive);
        hits || nested_chains(step, registry)
            .iter()
            .any(|chain| sends_order_sensitive_overlay(chain, registry, order_sensitive))
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

pub(super) fn analyze(
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

    for (i, health) in result.iter_mut().enumerate() {
        if let Some(Some(SubActionOutcome::Failed(msg))) = last_step_outcomes.get(i) {
            health.findings.push(Finding::LastRunFailed(msg.clone()));
        }
    }

    result
}

fn trigger_seed(triggers: &[TriggerInstance], registry: &TriggerRegistry) -> Option<TriggerSeed> {
    if triggers.is_empty() {
        return None;
    }
    let mut schemas: Vec<HashSet<String>> = Vec::with_capacity(triggers.len());
    for instance in triggers {
        let schema = registry.get(&instance.kind_id)?.output_schema()?;
        schemas.push(schema.variables.into_iter().map(|v| v.name).collect());
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
    let body_locals = body_local_vars(&step.kind_id);
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

fn nested_chains(step: &SubActionStep, registry: &SubActionRegistry) -> Vec<Vec<SubActionStep>> {
    let Some(runner) = registry.get(&step.kind_id) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for field in runner.config_fields() {
        match field {
            FormField::SubChain { key, .. } => {
                out.push(nav::decode_chain_value(nav::chain_value_at(
                    &step.config,
                    key,
                    None,
                )));
            }
            FormField::CaseList { key, .. } => {
                let count = step
                    .config
                    .get(key)
                    .and_then(Variant::as_array)
                    .map_or(0, <[Variant]>::len);
                for ci in 0..count {
                    out.push(nav::decode_chain_value(nav::chain_value_at(
                        &step.config,
                        key,
                        Some(ci),
                    )));
                }
            }
            _ => {}
        }
    }
    out
}

fn step_output_names(step: &SubActionStep, registry: &SubActionRegistry) -> HashSet<String> {
    let mut out = HashSet::new();
    if let Some(runner) = registry.get(&step.kind_id) {
        for produced in runner.scope_io().produces {
            if let Some(name) = step
                .config
                .get(&produced.output_name_key)
                .and_then(Variant::as_str)
                .and_then(normalize_var_name)
            {
                out.insert(name);
            }
        }
    }
    for key in dynamic_output_keys(&step.kind_id) {
        if let Some(name) = step
            .config
            .get(*key)
            .and_then(Variant::as_str)
            .and_then(normalize_var_name)
        {
            out.insert(name);
        }
    }
    for name in fixed_after_outputs(&step.kind_id) {
        out.insert((*name).to_owned());
    }
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

fn body_local_vars(kind_id: &str) -> Vec<String> {
    if kind_id == "core.logic.loop" {
        vec!["loop.index".to_owned(), "loop.item".to_owned()]
    } else {
        Vec::new()
    }
}

fn dynamic_output_keys(kind_id: &str) -> &'static [&'static str] {
    match kind_id {
        "script.run.named" => &["target_var"],
        "core.globals.get" => &["into_arg"],
        "core.users.get_var" | "core.math.evaluate" => &["into_var"],
        "core.args.set" => &["name"],
        _ => &[],
    }
}

fn fixed_after_outputs(kind_id: &str) -> &'static [&'static str] {
    match kind_id {
        "core.logic.loop" => &["loop.iterations_completed", "loop.exit_reason"],
        "core.logic.wait_until" => &["wait.elapsed_ms", "wait.timed_out"],
        "server.broadcast" => &["broadcast.delivered_count"],
        "core.time.now" => &["time.formatted", "time.unix_seconds"],
        "core.file.write" => &["file.bytes_written"],
        k if k.starts_with("core.http.") => &["http.status_code", "http.headers", "http.body"],
        _ => &[],
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use forge_registry::SubActionRegistry;
    use forge_runtime::sub_action_runners::CoreLogicIfThenElseRunner;
    use forge_runtime::{ConditionGate, Config};
    use forge_types::SubActionConfig;

    use super::*;

    const BRANCH_KIND: &str = "core.logic.if_then_else";
    const THEN_CHAIN_KEY: &str = "then_chain";
    const ORDERED: &str = "chat-wall";
    const UNORDERED: &str = "alert-box";

    fn registry() -> SubActionRegistry {
        let mut reg = SubActionRegistry::new();
        reg.register(Box::new(CoreLogicIfThenElseRunner::new(Arc::new(
            ConditionGate::new(&Config::default()),
        ))))
        .expect("the branching runner registers");
        reg
    }

    fn step(kind_id: &str, config: SubActionConfig, enabled: bool) -> SubActionStep {
        SubActionStep {
            kind_id: kind_id.to_owned(),
            config,
            enabled,
            continue_on_error: false,
            condition: None,
            label: None,
        }
    }

    fn send(identity: &str, enabled: bool) -> SubActionStep {
        step(
            OVERLAY_SEND_KIND,
            SubActionConfig::from([(
                OVERLAY_TARGET_KEY.to_owned(),
                Variant::String(identity.to_owned()),
            )]),
            enabled,
        )
    }

    fn branch(body: Vec<SubActionStep>, enabled: bool) -> SubActionStep {
        step(
            BRANCH_KIND,
            SubActionConfig::from([(THEN_CHAIN_KEY.to_owned(), nav::encode_chain(&body))]),
            enabled,
        )
    }

    #[test]
    fn an_ordered_overlay_counts_through_nested_branches_and_never_through_a_disabled_step() {
        for (steps, expected, label) in [
            (
                vec![send(ORDERED, true)],
                true,
                "a step sending to an overlay whose delivery order matters",
            ),
            (
                vec![send(UNORDERED, true)],
                false,
                "a step sending to an overlay whose delivery order does not matter",
            ),
            (
                vec![send(ORDERED, false)],
                false,
                "a disabled step that will never deliver anything",
            ),
            (
                vec![branch(vec![send(ORDERED, true)], true)],
                true,
                "an ordered overlay one branch deep",
            ),
            (
                vec![branch(vec![branch(vec![send(ORDERED, true)], true)], true)],
                true,
                "an ordered overlay two branches deep",
            ),
            (
                vec![branch(vec![send(ORDERED, true)], false)],
                false,
                "an ordered overlay inside a disabled branch",
            ),
            (
                vec![branch(vec![send(ORDERED, false)], true)],
                false,
                "a disabled step inside a live branch",
            ),
            (
                vec![send("%overlay_target%", true)],
                false,
                "an overlay named by a variable no stored identity matches",
            ),
        ] {
            assert_eq!(
                sends_order_sensitive_overlay(&steps, &registry(), &|id| id == ORDERED),
                expected,
                "{label}"
            );
        }
    }
}
