//! CORE-9 flat-with-parent-path telemetry: steps run inside a branch / loop / switch
//! body are lifted into the enclosing chain's flat `telemetry` list as NESTED rows,
//! each carrying a `parentIndex.arm/…/localIndex.kindId` path locator in `kind`.
//!
//! These pin the actual regression - per-step debugging telemetry silently losing
//! the rows produced inside composite bodies. The existing `core_logic_flow_control`
//! suite asserts only `signal` + `arg_stack`; NONE of it inspects `ChainRun.telemetry`,
//! so the whole nesting scheme (`retag`, `TelemetrySink`, `is_nested`, the arm tags)
//! was shipped untested.
//!
//! Driven end-to-end through the real `ChainEngine`, so the `TelemetrySink` drain in
//! the sequential driver and the per-composite arm-string choice are both exercised.
//! No services / hardware / network: the condition gate is the in-process rhai
//! evaluator and `core.args.set` is the observable nested-step probe.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use forge_events::{Event, EventPublisher};
use forge_registry::{CancelSignal, SubActionRegistry};
use forge_runtime::sub_action_runners::{
    CoreArgsSetRunner, CoreLogicBreakLoopRunner, CoreLogicContinueLoopRunner,
    CoreLogicIfThenElseRunner, CoreLogicLoopRunner, CoreLogicStopRunner, CoreLogicSwitchCaseRunner,
};
use forge_runtime::{ChainEngine, ChainRun, ConditionGate, Config};
use forge_types::{ArgStack, EventId, SubActionConfig, SubActionStep, SubActionTelemetry, Variant};

struct NullPublisher;
impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

fn engine() -> Arc<ChainEngine> {
    let gate = Arc::new(ConditionGate::new(&Config::default()));
    let mut reg = SubActionRegistry::new();
    reg.register(Box::new(CoreArgsSetRunner)).unwrap();
    reg.register(Box::new(CoreLogicBreakLoopRunner)).unwrap();
    reg.register(Box::new(CoreLogicContinueLoopRunner)).unwrap();
    reg.register(Box::new(CoreLogicStopRunner)).unwrap();
    reg.register(Box::new(CoreLogicSwitchCaseRunner)).unwrap();
    reg.register(Box::new(CoreLogicIfThenElseRunner::new(Arc::clone(&gate))))
        .unwrap();
    reg.register(Box::new(CoreLogicLoopRunner::new(Arc::clone(&gate))))
        .unwrap();
    Arc::new(ChainEngine::new(
        Arc::new(reg),
        Arc::new(NullPublisher),
        gate,
        Config::default(),
    ))
}

async fn run_top(engine: &Arc<ChainEngine>, steps: Vec<SubActionStep>) -> ChainRun {
    engine
        .run_sequential(
            &steps,
            &ArgStack::new(),
            EventId::new(),
            &CancelSignal::new(),
        )
        .await
}

fn step(kind: &str, config: SubActionConfig) -> SubActionStep {
    SubActionStep {
        kind_id: kind.to_owned(),
        config,
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

fn chain_step(kind: &str, config: SubActionConfig) -> Variant {
    let mut m = SubActionConfig::new();
    m.insert("kind_id".to_owned(), Variant::String(kind.to_owned()));
    m.insert("config".to_owned(), Variant::Object(config));
    m.insert("enabled".to_owned(), Variant::Bool(true));
    Variant::Object(m)
}

fn inline(steps: Vec<Variant>) -> Variant {
    Variant::Array(steps)
}

fn args_set(name: &str, value: &str) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert("name".to_owned(), Variant::String(name.to_owned()));
    c.insert("value".to_owned(), Variant::String(value.to_owned()));
    c
}

fn args_set_step(name: &str, value: &str) -> Variant {
    chain_step("core.args.set", args_set(name, value))
}

fn loop_cfg(count: i64, body: Variant) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert("mode".to_owned(), Variant::String("count".to_owned()));
    c.insert("count".to_owned(), Variant::Int(count));
    c.insert("body".to_owned(), body);
    c
}

fn if_cfg(condition: &str, then_chain: Variant, else_chain: Variant) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert(
        "condition".to_owned(),
        Variant::String(condition.to_owned()),
    );
    c.insert("then_chain".to_owned(), then_chain);
    c.insert("else_chain".to_owned(), else_chain);
    c
}

fn case(match_val: Variant, chain: Variant) -> Variant {
    let mut m = SubActionConfig::new();
    m.insert("match".to_owned(), match_val);
    m.insert("chain".to_owned(), chain);
    Variant::Object(m)
}

fn switch_cfg(expression: &str, cases: Vec<Variant>, default_chain: Variant) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert(
        "expression".to_owned(),
        Variant::String(expression.to_owned()),
    );
    c.insert("cases".to_owned(), Variant::Array(cases));
    c.insert("default_chain".to_owned(), default_chain);
    c
}

// ── assertion helpers ────────────────────────────────────────────────────────

fn nested_paths(tel: &[SubActionTelemetry]) -> Vec<String> {
    tel.iter()
        .filter(|t| t.is_nested())
        .map(|t| t.kind.clone())
        .collect()
}

fn top_level(tel: &[SubActionTelemetry]) -> Vec<&SubActionTelemetry> {
    tel.iter().filter(|t| !t.is_nested()).collect()
}

// ── the taken branch's step is lifted as a path-tagged nested row ────────────

#[tokio::test]
async fn branch_body_step_is_path_tagged_and_marked_nested_for_the_taken_arm() {
    // The single composite sits at top-level index 0, so its body step's locator
    // is `0.<arm>/0.core.args.set`. The arm string is the runner's own choice -
    // "then" vs "else" - which is exactly what regressed when nested rows vanished.
    let eng = engine();
    for (condition, arm) in [("1 == 1", "then"), ("1 == 2", "else")] {
        let cfg = if_cfg(
            condition,
            inline(vec![args_set_step("marker", "T")]),
            inline(vec![args_set_step("marker", "E")]),
        );
        let run = run_top(&eng, vec![step("core.logic.if_then_else", cfg)]).await;

        assert_eq!(
            nested_paths(&run.telemetry),
            vec![format!("0.{arm}/0.core.args.set")],
            "condition {condition}: nested body step must carry the arm-tagged locator",
        );
    }
}

// ── top-level rows keep their positional index; nested rows are excluded ──────

#[tokio::test]
async fn consumers_filtering_out_nested_rows_see_only_positional_top_level_steps() {
    // A top-level args.set (index 0) then an if whose taken branch runs TWO steps.
    // The "surfaces keyed by top-level position" contract: exactly the two real
    // top-level steps survive an `!is_nested()` filter, each keeping its index;
    // the two branch steps are present in the flat list but marked nested.
    let eng = engine();
    let branch = inline(vec![args_set_step("a", "1"), args_set_step("b", "2")]);
    let steps = vec![
        step("core.args.set", args_set("top", "0")),
        step(
            "core.logic.if_then_else",
            if_cfg("1 == 1", branch, inline(vec![])),
        ),
    ];
    let run = run_top(&eng, steps).await;

    let top = top_level(&run.telemetry);
    assert_eq!(
        top.len(),
        2,
        "only the two positional top-level steps survive"
    );
    assert_eq!(top[0].index, 0);
    assert_eq!(top[0].kind, "core.args.set");
    assert_eq!(top[1].index, 1);
    assert_eq!(top[1].kind, "core.logic.if_then_else");
    assert_eq!(
        nested_paths(&run.telemetry).len(),
        2,
        "both branch steps stay as nested rows"
    );
}

// ── each loop iteration tags its body with body#{iter} ───────────────────────

#[tokio::test]
async fn each_loop_iteration_tags_its_body_step_with_the_zero_based_iteration_number() {
    let eng = engine();
    let body = inline(vec![args_set_step("x", "%loop.index%")]);
    let run = run_top(&eng, vec![step("core.logic.loop", loop_cfg(2, body))]).await;

    assert_eq!(
        nested_paths(&run.telemetry),
        vec![
            "0.body#0/0.core.args.set".to_owned(),
            "0.body#1/0.core.args.set".to_owned(),
        ],
        "each iteration lifts its body step under a distinct body#N arm",
    );
}

// ── switch arm tag is case{N} on a match, `default` on the fallthrough ────────

#[tokio::test]
async fn switch_tags_the_nested_step_with_the_matched_case_index_or_default() {
    let eng = engine();
    for (selector, arm) in [("a", "case0"), ("zzz", "default")] {
        let cfg = switch_cfg(
            selector,
            vec![case(
                Variant::String("a".to_owned()),
                inline(vec![args_set_step("hit", "X")]),
            )],
            inline(vec![args_set_step("hit", "D")]),
        );
        let run = run_top(&eng, vec![step("core.logic.switch_case", cfg)]).await;

        assert_eq!(
            nested_paths(&run.telemetry),
            vec![format!("0.{arm}/0.core.args.set")],
            "selector {selector}: nested step must carry the {arm} arm tag",
        );
    }
}

// ── deep nesting: an already-nested row keeps its trail; the parent path folds ─

#[tokio::test]
async fn deeply_nested_step_accumulates_the_full_parent_path_across_composites() {
    // if(then) -> loop -> args.set. The loop row is folded once by the if
    // (`0.then/0.core.logic.loop`). The args.set row is ALREADY nested from the
    // loop's own retag (`0.body#0/0.core.args.set`), so the if must PREPEND its
    // path rather than re-fold the local index - the deepest locator threads
    // every enclosing arm.
    let eng = engine();
    let inner_loop = chain_step(
        "core.logic.loop",
        loop_cfg(1, inline(vec![args_set_step("deep", "1")])),
    );
    let cfg = if_cfg("1 == 1", inline(vec![inner_loop]), inline(vec![]));
    let run = run_top(&eng, vec![step("core.logic.if_then_else", cfg)]).await;

    let paths = nested_paths(&run.telemetry);
    assert!(
        paths.contains(&"0.then/0.core.logic.loop".to_owned()),
        "the loop step folds once under the if, got {paths:?}",
    );
    assert!(
        paths.contains(&"0.then/0.body#0/0.core.args.set".to_owned()),
        "the loop body step keeps its body#0 trail with the if path prepended, got {paths:?}",
    );
}

// ── boundary: an empty branch body produces no nested rows ────────────────────

#[tokio::test]
async fn empty_branch_body_leaves_only_the_non_nested_composite_row() {
    let eng = engine();
    let cfg = if_cfg("1 == 1", inline(vec![]), inline(vec![]));
    let run = run_top(&eng, vec![step("core.logic.if_then_else", cfg)]).await;

    assert!(
        nested_paths(&run.telemetry).is_empty(),
        "an empty branch lifts nothing",
    );
    let top = top_level(&run.telemetry);
    assert_eq!(top.len(), 1);
    assert_eq!(top[0].kind, "core.logic.if_then_else");
}
