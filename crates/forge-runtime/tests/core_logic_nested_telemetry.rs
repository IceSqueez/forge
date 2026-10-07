#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_registry::{
    CancelSignal, ChainSignal, FormField, RegistryError, RunContext, SubActionCategory,
    SubActionRegistry, SubActionRunner,
};
use forge_runtime::sub_action_runners::{
    CoreArgsSetRunner, CoreLogicBreakLoopRunner, CoreLogicContinueLoopRunner,
    CoreLogicIfThenElseRunner, CoreLogicLoopRunner, CoreLogicStopRunner, CoreLogicSwitchCaseRunner,
};
use forge_runtime::{ChainEngine, ChainRun, ConditionGate, Config};
use forge_types::{
    ArgStack, EventId, SubActionConfig, SubActionOutcome, SubActionStep, SubActionTelemetry,
    Variant,
};
use time::OffsetDateTime;

struct NullPublisher;
impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

const TRIP_KIND: &str = "test.trip";
const TRIP_ITERATION: i64 = 150;

struct TripRunner;

#[async_trait]
impl SubActionRunner for TripRunner {
    fn id(&self) -> &str {
        TRIP_KIND
    }
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }
    fn label(&self) -> &str {
        ""
    }
    fn summary(&self) -> &str {
        ""
    }
    fn search_text(&self) -> &str {
        ""
    }
    fn icon_name(&self) -> &str {
        ""
    }
    fn default_config(&self) -> SubActionConfig {
        SubActionConfig::new()
    }
    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }
    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }
    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let tripped = ctx.arg_stack.get("loop.index") == Some(&Variant::Int(TRIP_ITERATION));
        let outcome = match config.get("on_trip") {
            Some(Variant::String(mode)) if tripped && mode == "fail" => {
                SubActionOutcome::Failed("boom".to_owned())
            }
            Some(Variant::String(mode)) if tripped && mode == "cancel" => {
                ctx.cancel.cancel();
                SubActionOutcome::Success
            }
            _ => SubActionOutcome::Success,
        };
        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                index: ctx.index,
                kind: TRIP_KIND.to_owned(),
                started_at: OffsetDateTime::now_utc(),
                duration_ms: 0,
                outcome,
            },
            None,
        )
    }
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
    reg.register(Box::new(TripRunner)).unwrap();
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

fn nested_paths(tel: &[SubActionTelemetry]) -> Vec<String> {
    tel.iter()
        .filter(|t| t.is_nested())
        .map(|t| t.kind.clone())
        .collect()
}

fn top_level(tel: &[SubActionTelemetry]) -> Vec<&SubActionTelemetry> {
    tel.iter().filter(|t| !t.is_nested()).collect()
}

#[tokio::test]
async fn branch_body_step_is_path_tagged_and_marked_nested_for_the_taken_arm() {
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

#[tokio::test]
async fn consumers_filtering_out_nested_rows_see_only_positional_top_level_steps() {
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

#[tokio::test]
async fn deeply_nested_step_accumulates_the_full_parent_path_across_composites() {
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

const RECORDED_ITERATIONS: usize = 100;

fn recorded_iteration_paths(iterations: usize) -> Vec<String> {
    (0..iterations)
        .map(|n| format!("0.body#{n}/0.core.args.set"))
        .collect()
}

fn index_body() -> Variant {
    inline(vec![args_set_step("x", "%loop.index%")])
}

fn while_true_cfg(max_iterations: i64) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert("mode".to_owned(), Variant::String("while".to_owned()));
    c.insert(
        "while_condition".to_owned(),
        Variant::String("1 == 1".to_owned()),
    );
    c.insert("max_iterations".to_owned(), Variant::Int(max_iterations));
    c.insert("body".to_owned(), index_body());
    c
}

fn foreach_cfg() -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert(
        "mode".to_owned(),
        Variant::String("foreach_array".to_owned()),
    );
    c.insert(
        "array_source".to_owned(),
        Variant::String("items".to_owned()),
    );
    c.insert("body".to_owned(), index_body());
    c
}

async fn run_loop(engine: &Arc<ChainEngine>, cfg: SubActionConfig, items: usize) -> ChainRun {
    let items = (0..items as i64).map(Variant::Int).collect();
    let stack = ArgStack::new().set("items".to_owned(), Variant::Array(items));
    engine
        .run_sequential(
            &[step("core.logic.loop", cfg)],
            &stack,
            EventId::new(),
            &CancelSignal::new(),
        )
        .await
}

fn summary_rows(tel: &[SubActionTelemetry]) -> Vec<(String, bool)> {
    tel.iter()
        .filter(|t| t.is_nested() && !t.kind.contains('/'))
        .map(|t| {
            (
                t.kind.clone(),
                matches!(t.outcome, SubActionOutcome::Skipped(_)),
            )
        })
        .collect()
}

#[tokio::test]
async fn a_loop_of_exactly_the_recorded_iterations_keeps_every_step_and_adds_no_summary() {
    let eng = engine();
    let run = run_loop(&eng, loop_cfg(RECORDED_ITERATIONS as i64, index_body()), 0).await;

    assert_eq!(
        nested_paths(&run.telemetry),
        recorded_iteration_paths(RECORDED_ITERATIONS)
    );
}

#[tokio::test]
async fn iterations_past_the_recorded_ones_collapse_into_one_skipped_summary_in_every_mode() {
    let eng = engine();
    for (label, cfg, items, last) in [
        ("count 101", loop_cfg(101, index_body()), 0, 100),
        ("count 250", loop_cfg(250, index_body()), 0, 249),
        ("while 101", while_true_cfg(101), 0, 100),
        ("foreach 101", foreach_cfg(), 101, 100),
    ] {
        let run = run_loop(&eng, cfg, items).await;

        let mut expected = recorded_iteration_paths(RECORDED_ITERATIONS);
        expected.push(format!("0.body#100-{last}"));
        assert_eq!(nested_paths(&run.telemetry), expected, "{label}");
        assert_eq!(
            summary_rows(&run.telemetry),
            vec![(format!("0.body#100-{last}"), true)],
            "{label}"
        );
    }
}

fn tripping_loop(on_trip: &str) -> SubActionConfig {
    let mut trip = SubActionConfig::new();
    trip.insert("on_trip".to_owned(), Variant::String(on_trip.to_owned()));
    loop_cfg(
        300,
        inline(vec![
            chain_step(TRIP_KIND, trip),
            args_set_step("x", "%loop.index%"),
        ]),
    )
}

#[tokio::test]
async fn a_failure_past_the_recorded_iterations_ends_the_summary_at_the_failing_iteration() {
    let eng = engine();
    let run = run_loop(&eng, tripping_loop("fail"), 0).await;

    assert_eq!(
        (summary_rows(&run.telemetry), run.signal),
        (
            vec![(format!("0.body#100-{TRIP_ITERATION}"), true)],
            ChainSignal::Error("boom".to_owned())
        )
    );
}

#[tokio::test]
async fn a_cancel_past_the_recorded_iterations_ends_the_summary_at_the_aborted_iteration() {
    let eng = engine();
    let run = run_loop(&eng, tripping_loop("cancel"), 0).await;

    assert_eq!(
        summary_rows(&run.telemetry),
        vec![(format!("0.body#100-{TRIP_ITERATION}"), true)]
    );
}
