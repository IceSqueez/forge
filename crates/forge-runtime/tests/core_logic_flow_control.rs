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

struct AlwaysFailRunner;

#[async_trait]
impl SubActionRunner for AlwaysFailRunner {
    fn id(&self) -> &str {
        "test.always_fail"
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
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                index: ctx.index,
                kind: "test.always_fail".to_owned(),
                started_at: OffsetDateTime::now_utc(),
                duration_ms: 0,
                outcome: SubActionOutcome::Failed("boom".to_owned()),
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
    reg.register(Box::new(AlwaysFailRunner)).unwrap();
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

fn s(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

#[tokio::test]
async fn if_then_else_runs_the_taken_branchs_inline_chain() {
    let eng = engine();
    for (condition, branch, marker) in [
        ("1 == 1", "then", "then_hit"),
        ("1 == 2", "else", "else_hit"),
    ] {
        let cfg = if_cfg(
            condition,
            inline(vec![chain_step(
                "core.args.set",
                args_set("marker", "then_hit"),
            )]),
            inline(vec![chain_step(
                "core.args.set",
                args_set("marker", "else_hit"),
            )]),
        );
        let run = run_top(&eng, vec![step("core.logic.if_then_else", cfg)]).await;
        assert_eq!(run.signal, ChainSignal::Completed, "condition {condition}");
        assert_eq!(
            run.arg_stack.get("marker"),
            Some(&s(marker)),
            "branch body effect must land for condition {condition}",
        );
        assert_eq!(run.arg_stack.get("branch.taken"), Some(&s(branch)));
    }
}

#[tokio::test]
async fn loop_runs_its_inline_body_each_iteration_threading_the_index() {
    let eng = engine();
    let body = inline(vec![chain_step(
        "core.args.set",
        args_set("last_index", "%loop.index%"),
    )]);
    let run = run_top(&eng, vec![step("core.logic.loop", loop_cfg(3, body))]).await;

    assert_eq!(run.signal, ChainSignal::Completed);
    assert_eq!(
        run.arg_stack.get("loop.iterations_completed"),
        Some(&Variant::Int(3)),
    );
    assert_eq!(run.arg_stack.get("last_index"), Some(&Variant::Int(2)));
}

#[tokio::test]
async fn if_branch_with_non_array_body_is_a_noop_and_still_succeeds() {
    let eng = engine();
    let cfg = if_cfg("1 == 1", Variant::Int(7), inline(vec![]));
    let run = run_top(&eng, vec![step("core.logic.if_then_else", cfg)]).await;

    assert_eq!(run.signal, ChainSignal::Completed);
    assert_eq!(run.arg_stack.get("branch.taken"), Some(&s("then")));
    assert!(
        run.arg_stack.get("marker").is_none(),
        "an empty branch must produce no body effect",
    );
}

#[tokio::test]
async fn if_condition_error_falls_to_else_when_treating_undefined_as_false() {
    let eng = engine();
    let cfg = if_cfg(
        "1 + 1",
        inline(vec![chain_step(
            "core.args.set",
            args_set("marker", "THEN"),
        )]),
        inline(vec![chain_step(
            "core.args.set",
            args_set("marker", "ELSE"),
        )]),
    );
    let run = run_top(&eng, vec![step("core.logic.if_then_else", cfg)]).await;

    assert_eq!(run.signal, ChainSignal::Completed);
    assert_eq!(run.arg_stack.get("marker"), Some(&s("ELSE")));
}

#[tokio::test]
async fn if_condition_error_fails_when_not_treating_undefined_as_false() {
    let eng = engine();
    let mut cfg = if_cfg("1 + 1", inline(vec![]), inline(vec![]));
    cfg.insert("treat_undefined_as_false".to_owned(), Variant::Bool(false));
    let run = run_top(&eng, vec![step("core.logic.if_then_else", cfg)]).await;

    assert!(
        matches!(&run.signal, ChainSignal::Error(m) if m.contains("if_then_else")),
        "an unhandled condition error must fail the chain, got {:?}",
        run.signal,
    );
}

#[tokio::test]
async fn loop_absorbs_break_and_stops_iterating() {
    let eng = engine();
    let body = inline(vec![chain_step(
        "core.logic.break_loop",
        SubActionConfig::new(),
    )]);
    let run = run_top(&eng, vec![step("core.logic.loop", loop_cfg(5, body))]).await;

    assert_eq!(
        run.signal,
        ChainSignal::Completed,
        "break must be absorbed by the loop, not leaked to the root",
    );
    assert_eq!(run.arg_stack.get("loop.exit_reason"), Some(&s("break")));
    assert_eq!(
        run.arg_stack.get("loop.iterations_completed"),
        Some(&Variant::Int(1)),
        "break exits after the first iteration, not all five",
    );
}

#[tokio::test]
async fn loop_absorbs_continue_and_skips_the_rest_of_the_body() {
    let eng = engine();
    let body = inline(vec![
        chain_step("core.logic.continue_loop", SubActionConfig::new()),
        chain_step("core.args.set", args_set("after_continue", "reached")),
    ]);
    let run = run_top(&eng, vec![step("core.logic.loop", loop_cfg(3, body))]).await;

    assert_eq!(run.signal, ChainSignal::Completed);
    assert_eq!(
        run.arg_stack.get("loop.iterations_completed"),
        Some(&Variant::Int(3)),
        "continue advances the loop, it does not stop it",
    );
    assert_eq!(run.arg_stack.get("loop.exit_reason"), Some(&s("completed")));
    assert!(
        run.arg_stack.get("after_continue").is_none(),
        "the post-continue body step must be skipped every iteration",
    );
}

#[tokio::test]
async fn if_then_else_re_propagates_break_to_the_enclosing_loop() {
    let eng = engine();
    let inner_if = chain_step(
        "core.logic.if_then_else",
        if_cfg(
            "1 == 1",
            inline(vec![chain_step(
                "core.logic.break_loop",
                SubActionConfig::new(),
            )]),
            inline(vec![]),
        ),
    );
    let run = run_top(
        &eng,
        vec![step("core.logic.loop", loop_cfg(5, inline(vec![inner_if])))],
    )
    .await;

    assert_eq!(run.signal, ChainSignal::Completed);
    assert_eq!(
        run.arg_stack.get("loop.exit_reason"),
        Some(&s("break")),
        "break raised inside the if must travel through it to the loop",
    );
    assert_eq!(
        run.arg_stack.get("loop.iterations_completed"),
        Some(&Variant::Int(1)),
    );
}

#[tokio::test]
async fn switch_case_re_propagates_break_to_the_enclosing_loop() {
    let eng = engine();
    let inner_switch = chain_step(
        "core.logic.switch_case",
        switch_cfg(
            "go",
            vec![case(
                s("go"),
                inline(vec![chain_step(
                    "core.logic.break_loop",
                    SubActionConfig::new(),
                )]),
            )],
            inline(vec![]),
        ),
    );
    let run = run_top(
        &eng,
        vec![step(
            "core.logic.loop",
            loop_cfg(5, inline(vec![inner_switch])),
        )],
    )
    .await;

    assert_eq!(run.signal, ChainSignal::Completed);
    assert_eq!(run.arg_stack.get("loop.exit_reason"), Some(&s("break")));
    assert_eq!(
        run.arg_stack.get("loop.iterations_completed"),
        Some(&Variant::Int(1)),
    );
}

#[tokio::test]
async fn stop_propagates_through_the_loop_to_the_action_root() {
    let eng = engine();
    let body = inline(vec![chain_step("core.logic.stop", SubActionConfig::new())]);
    let run = run_top(&eng, vec![step("core.logic.loop", loop_cfg(5, body))]).await;

    assert!(
        matches!(run.signal, ChainSignal::Stop(_)),
        "stop must escape the loop to the root, got {:?}",
        run.signal,
    );
    assert_eq!(
        run.arg_stack.get("loop.iterations_completed"),
        Some(&Variant::Int(1)),
    );
}

#[tokio::test]
async fn child_chain_error_propagates_through_the_loop_to_the_action_root() {
    let eng = engine();
    let body = inline(vec![chain_step("test.always_fail", SubActionConfig::new())]);
    let run = run_top(&eng, vec![step("core.logic.loop", loop_cfg(5, body))]).await;

    assert_eq!(run.signal, ChainSignal::Error("boom".to_owned()));
}

#[tokio::test]
async fn break_in_an_inner_loop_does_not_escape_to_the_outer_loop() {
    let eng = engine();
    let inner = chain_step(
        "core.logic.loop",
        loop_cfg(
            3,
            inline(vec![chain_step(
                "core.logic.break_loop",
                SubActionConfig::new(),
            )]),
        ),
    );
    let run = run_top(
        &eng,
        vec![step("core.logic.loop", loop_cfg(2, inline(vec![inner])))],
    )
    .await;

    assert_eq!(run.signal, ChainSignal::Completed);
    assert_eq!(
        run.arg_stack.get("loop.iterations_completed"),
        Some(&Variant::Int(2)),
    );
    assert_eq!(run.arg_stack.get("loop.exit_reason"), Some(&s("completed")));
}

#[tokio::test]
async fn switch_runs_the_first_matching_cases_chain() {
    let eng = engine();
    let cfg = switch_cfg(
        "b",
        vec![
            case(
                s("a"),
                inline(vec![chain_step("core.args.set", args_set("hit", "A"))]),
            ),
            case(
                s("b"),
                inline(vec![chain_step("core.args.set", args_set("hit", "B"))]),
            ),
            case(
                s("b"),
                inline(vec![chain_step("core.args.set", args_set("hit", "B2"))]),
            ),
        ],
        inline(vec![]),
    );
    let run = run_top(&eng, vec![step("core.logic.switch_case", cfg)]).await;

    assert_eq!(run.signal, ChainSignal::Completed);
    assert_eq!(
        run.arg_stack.get("hit"),
        Some(&s("B")),
        "the first matching case wins, not a later duplicate",
    );
    assert_eq!(
        run.arg_stack.get("switch.matched_case_index"),
        Some(&Variant::Int(1)),
    );
}

#[tokio::test]
async fn switch_matches_a_value_in_a_case_value_list_by_display_form() {
    let eng = engine();
    let cfg = switch_cfg(
        "42",
        vec![case(
            Variant::Array(vec![Variant::Int(1), Variant::Int(42)]),
            inline(vec![chain_step("core.args.set", args_set("hit", "list"))]),
        )],
        inline(vec![]),
    );
    let run = run_top(&eng, vec![step("core.logic.switch_case", cfg)]).await;

    assert_eq!(run.arg_stack.get("hit"), Some(&s("list")));
    assert_eq!(
        run.arg_stack.get("switch.matched_case_index"),
        Some(&Variant::Int(0)),
    );
}

#[tokio::test]
async fn switch_runs_default_chain_with_index_minus_one_when_no_case_matches() {
    let eng = engine();
    let cfg = switch_cfg(
        "zzz",
        vec![case(
            s("a"),
            inline(vec![chain_step("core.args.set", args_set("hit", "A"))]),
        )],
        inline(vec![chain_step(
            "core.args.set",
            args_set("hit", "DEFAULT"),
        )]),
    );
    let run = run_top(&eng, vec![step("core.logic.switch_case", cfg)]).await;

    assert_eq!(run.signal, ChainSignal::Completed);
    assert_eq!(run.arg_stack.get("hit"), Some(&s("DEFAULT")));
    assert_eq!(
        run.arg_stack.get("switch.matched_case_index"),
        Some(&Variant::Int(-1)),
    );
}
