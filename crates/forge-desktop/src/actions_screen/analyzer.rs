use std::collections::HashSet;

use forge_registry::{FormField, SubActionRegistry, TriggerRegistry, declared_variables};
use forge_types::{
    Action, ExecutionMode, SubActionOutcome, SubActionStep, TriggerInstance, Variant,
    normalize_var_name,
};

use super::nav;

const BREAK_LOOP_KIND_ID: &str = "core.logic.break_loop";
const CONTINUE_LOOP_KIND_ID: &str = "core.logic.continue_loop";
const STOP_KIND_ID: &str = "core.logic.stop";

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
    ControlFlowInConcurrentAction,
}

impl Finding {
    fn severity(&self) -> HealthSeverity {
        match self {
            Finding::UnknownVariable(_) | Finding::LastRunFailed(_) => HealthSeverity::Red,
            Finding::ProducedLater(_)
            | Finding::IsolatedSibling(_)
            | Finding::SomeTriggersOnly(_)
            | Finding::ControlFlowInConcurrentAction => HealthSeverity::Yellow,
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

pub(super) fn sends_order_sensitive_overlay(
    steps: &[SubActionStep],
    registry: &SubActionRegistry,
    order_sensitive: &dyn Fn(&str) -> bool,
) -> bool {
    forge_runtime::overlay_send_targets(steps, registry)
        .iter()
        .filter_map(forge_runtime::OverlaySendTarget::overlay)
        .any(order_sensitive)
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

    if action.concurrent {
        for (step, health) in action.sub_actions.iter().zip(result.iter_mut()) {
            if !step.enabled {
                continue;
            }
            if is_control_flow_kind(&step.kind_id) {
                health.findings.push(Finding::ControlFlowInConcurrentAction);
            }
            for chain in nested_chains(step, sub_registry) {
                collect_concurrent_control_flow_findings(
                    &chain,
                    sub_registry,
                    &mut health.findings,
                );
            }
        }
    }

    for (i, health) in result.iter_mut().enumerate() {
        if let Some(Some(SubActionOutcome::Failed(msg))) = last_step_outcomes.get(i) {
            health.findings.push(Finding::LastRunFailed(msg.clone()));
        }
    }

    result
}

fn is_control_flow_kind(kind_id: &str) -> bool {
    matches!(
        kind_id,
        BREAK_LOOP_KIND_ID | CONTINUE_LOOP_KIND_ID | STOP_KIND_ID
    )
}

fn collect_concurrent_control_flow_findings(
    steps: &[SubActionStep],
    registry: &SubActionRegistry,
    out: &mut Vec<Finding>,
) {
    for step in steps {
        if !step.enabled {
            continue;
        }
        if is_control_flow_kind(&step.kind_id) {
            out.push(Finding::ControlFlowInConcurrentAction);
        }
        for chain in nested_chains(step, registry) {
            collect_concurrent_control_flow_findings(&chain, registry, out);
        }
    }
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

    use forge_events::Event;
    use forge_registry::{
        ActorBlock, ActorIdentity, EventFilter, FormField, KindPlatformContract, LoginSlot,
        SubActionRegistry, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
    };
    use forge_runtime::sub_action_runners::CoreLogicIfThenElseRunner;
    use forge_runtime::{ConditionGate, Config};
    use forge_types::{
        ActionId, ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PermissionRung,
        PlatformId, PlatformScope, QueueId, SubActionConfig, TriggerConfig, TriggerInstanceId,
        VariableSchema, VariantKind,
    };

    use super::*;

    struct StubTrigger {
        id: &'static str,
        variables: fn() -> TriggerVariables,
    }

    impl StubTrigger {
        fn declaring(id: &'static str, variables: fn() -> TriggerVariables) -> Self {
            StubTrigger { id, variables }
        }
    }

    impl TriggerKindDescriptor for StubTrigger {
        fn id(&self) -> &str {
            self.id
        }

        fn category(&self) -> TriggerCategory {
            TriggerCategory::Chat
        }

        fn label(&self) -> &str {
            self.id
        }

        fn summary(&self) -> &str {
            self.id
        }

        fn search_text(&self) -> &str {
            self.id
        }

        fn icon_name(&self) -> &str {
            "chat"
        }

        fn platform_contract(&self) -> KindPlatformContract {
            KindPlatformContract::PlatformSpecific(PlatformId::Kick)
        }

        fn default_config(&self) -> TriggerConfig {
            TriggerConfig::new()
        }

        fn config_fields(&self) -> Vec<FormField> {
            Vec::new()
        }

        fn condition_display(&self, _config: &TriggerConfig) -> String {
            String::new()
        }

        fn event_filter(&self) -> EventFilter {
            EventFilter {
                source: None,
                kind_prefix: None,
            }
        }

        fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
            true
        }

        fn variables(&self) -> Option<TriggerVariables> {
            Some((self.variables)())
        }

        fn output_schema(&self) -> Option<VariableSchema> {
            self.variables().map(|variables| variables.schema())
        }
    }

    const BRANCH_KIND: &str = "core.logic.if_then_else";
    const THEN_CHAIN_KEY: &str = "then_chain";
    const ORDERED: &str = "chat-wall";
    const UNORDERED: &str = "alert-box";
    const CONSUMER_KIND: &str = "chat.send";
    const MESSAGE_KEY: &str = "message";
    const LEGACY_TRIGGER: &str = "stub.chat_with_legacy";
    const CANONICAL_TRIGGER: &str = "stub.chat_canonical";
    const LEGACY_NAME: &str = "username";

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

    fn send(identity: &str) -> SubActionStep {
        step(
            forge_runtime::OVERLAY_SEND_KIND_ID,
            SubActionConfig::from([(
                forge_runtime::OVERLAY_TARGET_KEY.to_owned(),
                Variant::String(identity.to_owned()),
            )]),
            true,
        )
    }

    fn branch(body: Vec<SubActionStep>) -> SubActionStep {
        step(
            BRANCH_KIND,
            SubActionConfig::from([(THEN_CHAIN_KEY.to_owned(), nav::encode_chain(&body))]),
            true,
        )
    }

    #[test]
    fn only_a_resolved_order_sensitive_target_raises_the_warning() {
        for (steps, expected, label) in [
            (
                vec![send(ORDERED)],
                true,
                "a step sending to an overlay whose delivery order matters",
            ),
            (
                vec![send(UNORDERED)],
                false,
                "a step sending to an overlay whose delivery order does not matter",
            ),
            (
                vec![branch(vec![send(ORDERED)])],
                true,
                "an ordered overlay inside a branch this screen encoded itself",
            ),
            (
                vec![send("%overlay_target%")],
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

    fn chatter(_event: &forge_events::Event) -> ActorIdentity {
        ActorIdentity {
            id: "51938264".to_owned(),
            display_name: "StreamFan42".to_owned(),
            login: Some("streamfan42".to_owned()),
        }
    }

    fn principal_block() -> ActorBlock {
        ActorBlock {
            role: ActorRole::Principal,
            platform: PlatformId::Kick,
            login: LoginSlot::Declared,
        }
    }

    fn canonical_actor_only() -> TriggerVariables {
        TriggerVariables::new().actor(principal_block(), chatter)
    }

    fn canonical_actor_plus_legacy_username() -> TriggerVariables {
        canonical_actor_only().legacy(
            DeclaredVariable {
                name: LEGACY_NAME.to_owned(),
                kind: VariantKind::String,
                label: "Username".to_owned(),
                synthesis: None,
            },
            CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
            |_| Variant::String("streamfan42".to_owned()),
        )
    }

    fn triggers(descriptors: Vec<StubTrigger>) -> TriggerRegistry {
        let mut reg = TriggerRegistry::new();
        for descriptor in descriptors {
            reg.register(Box::new(descriptor))
                .expect("each stub trigger registers under its own id");
        }
        reg
    }

    fn instance(kind_id: &str) -> TriggerInstance {
        TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: kind_id.to_owned(),
            name: kind_id.to_owned(),
            overrides: TriggerConfig::new(),
            enabled: true,
            user_defined: false,
            platform_scope: PlatformScope::Any,
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: PermissionRung::default(),
        }
    }

    fn action_consuming(templates: &[&str]) -> Action {
        Action {
            id: ActionId::new(),
            name: "stub action".to_owned(),
            group: None,
            queue_id: QueueId::new(),
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::Sequential,
            description: None,
            sub_actions: templates
                .iter()
                .map(|template| {
                    step(
                        CONSUMER_KIND,
                        SubActionConfig::from([(
                            MESSAGE_KEY.to_owned(),
                            Variant::String((*template).to_owned()),
                        )]),
                        true,
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn a_legacy_name_a_step_consumes_is_never_reported_as_unknown() {
        let health = analyze(
            &action_consuming(&["%username%", "%not_declared_anywhere%"]),
            &[instance(LEGACY_TRIGGER)],
            &[None, None],
            &registry(),
            &triggers(vec![StubTrigger::declaring(
                LEGACY_TRIGGER,
                canonical_actor_plus_legacy_username,
            )]),
        );

        assert_eq!(
            health,
            vec![
                StepHealth::default(),
                StepHealth {
                    findings: vec![Finding::UnknownVariable("not_declared_anywhere".to_owned())],
                },
            ],
        );
    }

    #[test]
    fn a_legacy_name_only_one_of_two_triggers_declares_is_reported_as_some_triggers_only() {
        let health = analyze(
            &action_consuming(&["%username%"]),
            &[instance(LEGACY_TRIGGER), instance(CANONICAL_TRIGGER)],
            &[None],
            &registry(),
            &triggers(vec![
                StubTrigger::declaring(LEGACY_TRIGGER, canonical_actor_plus_legacy_username),
                StubTrigger::declaring(CANONICAL_TRIGGER, canonical_actor_only),
            ]),
        );

        assert_eq!(
            health,
            vec![StepHealth {
                findings: vec![Finding::SomeTriggersOnly(LEGACY_NAME.to_owned())],
            }],
        );
    }
}
