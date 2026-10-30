#![allow(clippy::unwrap_used)]
mod common;
use common::{code_of, detail_of, message_of};
use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_registry::{
    CancelSignal, ChainSignal, FormField, RegistryError, RunContext, StepTimer, SubActionCategory,
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
                outcome: SubActionOutcome::Failed(forge_types::VerbatimText::new("boom").into()),
            },
            None,
        )
    }
}

struct CancelsItsRunRunner;

#[async_trait]
impl SubActionRunner for CancelsItsRunRunner {
    fn id(&self) -> &str {
        "test.cancels_its_run"
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
        ctx.cancel.cancel();
        (
            StepTimer::start(ctx, "test.cancels_its_run").success(),
            None,
        )
    }
}

fn engine() -> Arc<ChainEngine> {
    let config = Config {
        condition_wall_time_ms: 30_000,
        ..Config::default()
    };
    let gate = Arc::new(ConditionGate::new(&config));
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
    reg.register(Box::new(CancelsItsRunRunner)).unwrap();
    Arc::new(ChainEngine::new(
        Arc::new(reg),
        Arc::new(NullPublisher),
        gate,
        config,
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
async fn if_branch_with_non_array_body_fails_the_step_without_taking_a_branch() {
    let eng = engine();
    let cfg = if_cfg("1 == 1", Variant::Int(7), inline(vec![]));
    let run = run_top(&eng, vec![step("core.logic.if_then_else", cfg)]).await;

    assert!(
        matches!(&run.signal, ChainSignal::Error(m) if common::rejected_field(m) == Some("then_chain")),
        "{:?}",
        run.signal
    );
    assert!(run.arg_stack.get("branch.taken").is_none());
}

fn marker_branches(condition: &str) -> SubActionConfig {
    if_cfg(
        condition,
        inline(vec![chain_step(
            "core.args.set",
            args_set("marker", "THEN"),
        )]),
        inline(vec![chain_step(
            "core.args.set",
            args_set("marker", "ELSE"),
        )]),
    )
}

async fn run_with(
    engine: &Arc<ChainEngine>,
    steps: Vec<SubActionStep>,
    args: &[(&str, &str)],
) -> ChainRun {
    let stack = args.iter().fold(ArgStack::new(), |acc, (k, v)| {
        acc.set((*k).to_owned(), s(v))
    });
    engine
        .run_sequential(&steps, &stack, EventId::new(), &CancelSignal::new())
        .await
}

const INJECTION: &str = r#"a" == "a" || ""#;

#[tokio::test]
async fn if_undefined_variable_falls_to_else_when_treating_undefined_as_false() {
    let eng = engine();
    let cfg = marker_branches("%missing% > 5");
    let run = run_top(&eng, vec![step("core.logic.if_then_else", cfg)]).await;

    assert_eq!(run.signal, ChainSignal::Completed);
    assert_eq!(run.arg_stack.get("marker"), Some(&s("ELSE")));
}

#[tokio::test]
async fn if_undefined_variable_fails_when_not_treating_undefined_as_false() {
    let eng = engine();
    let mut cfg = marker_branches("%missing% > 5");
    cfg.insert("treat_undefined_as_false".to_owned(), Variant::Bool(false));
    let run = run_top(&eng, vec![step("core.logic.if_then_else", cfg)]).await;

    let ChainSignal::Error(reason) = &run.signal else {
        panic!(
            "an unresolved variable must fail the chain, got {:?}",
            run.signal
        );
    };
    let reason = message_of(reason);
    assert_eq!(code_of(reason), "core.condition_failed");
    assert!(
        detail_of(reason).is_some_and(|d| d.contains("missing")),
        "{reason:?}"
    );
}

#[tokio::test]
async fn if_non_undefined_condition_errors_fail_even_when_treating_undefined_as_false() {
    let eng = engine();
    for condition in ["1 + 1", "1 +", "let x = 0; while true { x += 1; } x > 0"] {
        let run = run_top(
            &eng,
            vec![step("core.logic.if_then_else", marker_branches(condition))],
        )
        .await;
        let ChainSignal::Error(reason) = &run.signal else {
            panic!("{condition}: must fail the step, got {:?}", run.signal);
        };
        assert_eq!(
            code_of(message_of(reason)),
            "core.condition_failed",
            "{condition}"
        );
        assert!(run.arg_stack.get("marker").is_none(), "{condition}");
    }
}

#[tokio::test]
async fn if_condition_argument_text_cannot_pick_the_branch() {
    let eng = engine();
    let cfg = marker_branches(r#""%message%" == "!secret""#);
    let run = run_with(
        &eng,
        vec![step("core.logic.if_then_else", cfg)],
        &[("message", INJECTION)],
    )
    .await;

    assert_eq!(run.signal, ChainSignal::Completed);
    assert_eq!(run.arg_stack.get("marker"), Some(&s("ELSE")));
}

#[tokio::test]
async fn if_condition_compares_quoted_chat_text_literally() {
    let eng = engine();
    let cfg = marker_branches(r#""%message%" == "he said \"hi\" C:\\temp""#);
    let run = run_with(
        &eng,
        vec![step("core.logic.if_then_else", cfg)],
        &[("message", r#"he said "hi" C:\temp"#)],
    )
    .await;

    assert_eq!(run.signal, ChainSignal::Completed);
    assert_eq!(run.arg_stack.get("marker"), Some(&s("THEN")));
}

fn while_cfg(condition: &str, max_iterations: i64) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert("mode".to_owned(), s("while"));
    c.insert("while_condition".to_owned(), s(condition));
    c.insert("max_iterations".to_owned(), Variant::Int(max_iterations));
    c.insert("body".to_owned(), inline(vec![]));
    c
}

#[tokio::test]
async fn loop_while_argument_text_cannot_keep_the_loop_running() {
    let eng = engine();
    let run = run_with(
        &eng,
        vec![step(
            "core.logic.loop",
            while_cfg(r#""%message%" == "go""#, 5),
        )],
        &[("message", INJECTION)],
    )
    .await;

    assert_eq!(run.signal, ChainSignal::Completed);
    assert_eq!(
        run.arg_stack.get("loop.iterations_completed"),
        Some(&Variant::Int(0)),
    );
}

#[tokio::test]
async fn loop_while_condition_error_fails_the_loop_step_with_a_reason() {
    let eng = engine();
    for condition in ["1 +", "%missing% > 5"] {
        let run = run_top(&eng, vec![step("core.logic.loop", while_cfg(condition, 5))]).await;
        let ChainSignal::Error(reason) = &run.signal else {
            panic!("{condition}: must fail the loop, got {:?}", run.signal);
        };
        assert_eq!(
            code_of(message_of(reason)),
            "core.condition_failed",
            "{condition}"
        );
    }
}

#[tokio::test]
async fn step_condition_argument_text_cannot_force_the_step_to_run() {
    let eng = engine();
    let mut guarded = step("core.args.set", args_set("marker", "RAN"));
    guarded.condition = Some(r#""%message%" == "!secret""#.to_owned());
    let run = run_with(&eng, vec![guarded], &[("message", INJECTION)]).await;

    assert_eq!(run.signal, ChainSignal::Completed);
    assert!(run.arg_stack.get("marker").is_none(), "guarded step ran");
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
async fn loop_names_why_it_ended_for_every_early_exit() {
    let eng = engine();
    for (loop_config, expected) in [
        (
            loop_cfg(
                5,
                inline(vec![chain_step("core.logic.stop", SubActionConfig::new())]),
            ),
            "stopped",
        ),
        (
            loop_cfg(
                5,
                inline(vec![chain_step("test.always_fail", SubActionConfig::new())]),
            ),
            "failed",
        ),
        (while_cfg("1 +", 5), "failed"),
        (
            loop_cfg(
                5,
                inline(vec![
                    chain_step("test.cancels_its_run", SubActionConfig::new()),
                    chain_step("core.logic.stop", SubActionConfig::new()),
                ]),
            ),
            "cancelled",
        ),
    ] {
        let run = run_top(&eng, vec![step("core.logic.loop", loop_config)]).await;

        assert_eq!(run.arg_stack.get("loop.exit_reason"), Some(&s(expected)));
    }
}

#[tokio::test]
async fn a_loop_entered_after_the_run_was_cancelled_exits_as_cancelled_without_iterating() {
    let config = Config::default();
    let runner = CoreLogicLoopRunner::new(Arc::new(ConditionGate::new(&config)));
    let stack = ArgStack::new();
    let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
    ctx.cancel.cancel();

    let (telemetry, produced) = runner.execute(&loop_cfg(5, inline(vec![])), &ctx).await;

    let produced = produced.unwrap();
    assert_eq!(produced.get("loop.exit_reason"), Some(&s("cancelled")));
    assert_eq!(
        produced.get("loop.iterations_completed"),
        Some(&Variant::Int(0))
    );
    assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
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

    assert_eq!(
        run.signal,
        ChainSignal::Error(forge_types::VerbatimText::new("boom").into())
    );
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

const FOREACH_LIMIT: usize = 10_000;

fn foreach_cfg(body: Variant) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert("mode".to_owned(), s("foreach_array"));
    c.insert("array_source".to_owned(), s("items"));
    c.insert("body".to_owned(), body);
    c
}

async fn run_foreach(engine: &Arc<ChainEngine>, len: usize, steps: Vec<SubActionStep>) -> ChainRun {
    let items = (0..len as i64).map(Variant::Int).collect();
    let stack = ArgStack::new().set("items".to_owned(), Variant::Array(items));
    engine
        .run_sequential(&steps, &stack, EventId::new(), &CancelSignal::new())
        .await
}

#[tokio::test]
async fn foreach_hands_each_array_item_to_the_body_in_order() {
    let eng = engine();
    let body = inline(vec![chain_step(
        "core.args.set",
        args_set("seen", "%seen%%loop.item%"),
    )]);
    let items = Variant::Array(vec![s("a"), s("b"), s("c")]);
    let stack = ArgStack::new()
        .set("items".to_owned(), items)
        .set("seen".to_owned(), s(""));
    let run = eng
        .run_sequential(
            &[step("core.logic.loop", foreach_cfg(body))],
            &stack,
            EventId::new(),
            &CancelSignal::new(),
        )
        .await;

    assert_eq!(run.arg_stack.get("seen"), Some(&s("abc")));
}

#[tokio::test]
async fn foreach_up_to_the_iteration_limit_visits_every_item() {
    let eng = engine();
    for len in [FOREACH_LIMIT - 1, FOREACH_LIMIT] {
        let steps = vec![step("core.logic.loop", foreach_cfg(inline(vec![])))];
        let run = run_foreach(&eng, len, steps).await;

        assert_eq!(
            (
                run.signal,
                run.arg_stack.get("loop.iterations_completed"),
                run.arg_stack.get("loop.exit_reason"),
            ),
            (
                ChainSignal::Completed,
                Some(&Variant::Int(len as i64)),
                Some(&s("completed")),
            ),
            "len={len}",
        );
    }
}

#[tokio::test]
async fn foreach_one_item_past_the_limit_fails_the_step_after_the_limit() {
    let eng = engine();
    let steps = vec![step("core.logic.loop", foreach_cfg(inline(vec![])))];
    let run = run_foreach(&eng, FOREACH_LIMIT + 1, steps).await;

    assert!(
        matches!(run.signal, ChainSignal::Error(_)),
        "got {:?}",
        run.signal
    );
    assert_eq!(
        (
            run.arg_stack.get("loop.iterations_completed"),
            run.arg_stack.get("loop.exit_reason"),
        ),
        (
            Some(&Variant::Int(FOREACH_LIMIT as i64)),
            Some(&s("foreach_limit")),
        ),
    );
}

#[tokio::test]
async fn foreach_limit_exit_reason_reaches_the_next_step_when_the_loop_continues_on_error() {
    let eng = engine();
    let mut loop_step = step("core.logic.loop", foreach_cfg(inline(vec![])));
    loop_step.continue_on_error = true;
    let steps = vec![
        loop_step,
        step("core.args.set", args_set("observed", "%loop.exit_reason%")),
    ];
    let run = run_foreach(&eng, FOREACH_LIMIT + 1, steps).await;

    assert_eq!(run.arg_stack.get("observed"), Some(&s("foreach_limit")));
}

#[tokio::test]
async fn logic_steps_skip_unreadable_nested_entries_and_run_the_rest_of_the_chain() {
    let eng = engine();
    let chain = || {
        let mut wrong_flag = SubActionConfig::new();
        wrong_flag.insert("kind_id".to_owned(), s("core.args.set"));
        wrong_flag.insert(
            "config".to_owned(),
            Variant::Object(args_set("skipped", "ran")),
        );
        wrong_flag.insert("enabled".to_owned(), s("yes"));
        let mut no_kind = SubActionConfig::new();
        no_kind.insert(
            "config".to_owned(),
            Variant::Object(args_set("skipped", "ran")),
        );
        inline(vec![
            chain_step("core.args.set", args_set("before", "ran")),
            Variant::Object(wrong_flag),
            s("not a step"),
            Variant::Object(no_kind),
            chain_step("core.args.set", args_set("after", "ran")),
        ])
    };
    for (arm, logic) in [
        (
            "then",
            step(
                "core.logic.if_then_else",
                if_cfg("1 == 1", chain(), inline(vec![])),
            ),
        ),
        (
            "else",
            step(
                "core.logic.if_then_else",
                if_cfg("1 == 2", inline(vec![]), chain()),
            ),
        ),
        ("loop body", step("core.logic.loop", loop_cfg(1, chain()))),
        (
            "case",
            step(
                "core.logic.switch_case",
                switch_cfg("a", vec![case(s("a"), chain())], inline(vec![])),
            ),
        ),
        (
            "default",
            step("core.logic.switch_case", switch_cfg("zzz", vec![], chain())),
        ),
    ] {
        let run = run_top(&eng, vec![logic]).await;

        assert_eq!(run.signal, ChainSignal::Completed, "{arm}");
        assert_eq!(run.arg_stack.get("before"), Some(&s("ran")), "{arm}");
        assert_eq!(run.arg_stack.get("after"), Some(&s("ran")), "{arm}");
        assert!(run.arg_stack.get("skipped").is_none(), "{arm}");
    }
}

const CANCELLED_CODE: &str = "core.action_run_cancelled";

fn cancels() -> Variant {
    chain_step("test.cancels_its_run", SubActionConfig::new())
}

fn marker(name: &str) -> Variant {
    chain_step("core.args.set", args_set(name, "yes"))
}

fn cancelled_child_cases(after_cancel: Vec<Variant>) -> [(&'static str, SubActionStep); 3] {
    let mut body = vec![cancels()];
    body.extend(after_cancel);
    [
        (
            "if",
            step(
                "core.logic.if_then_else",
                if_cfg("1 == 1", inline(body.clone()), inline(vec![])),
            ),
        ),
        (
            "switch",
            step(
                "core.logic.switch_case",
                switch_cfg("zzz", vec![], inline(body.clone())),
            ),
        ),
        ("loop", step("core.logic.loop", loop_cfg(3, inline(body)))),
    ]
}

async fn run_cancellable(
    engine: &Arc<ChainEngine>,
    steps: Vec<SubActionStep>,
) -> (ChainRun, CancelSignal) {
    let cancel = CancelSignal::new();
    let run = engine
        .run_sequential(&steps, &ArgStack::new(), EventId::new(), &cancel)
        .await;
    (run, cancel)
}

fn top_level_outcome(run: &ChainRun) -> &SubActionOutcome {
    &run.telemetry
        .iter()
        .find(|telemetry| !telemetry.is_nested())
        .unwrap()
        .outcome
}

#[tokio::test]
async fn a_composite_whose_child_was_interrupted_reports_cancelled_not_success() {
    let eng = engine();
    for (label, composite) in cancelled_child_cases(vec![marker("inner")]) {
        let after = step("core.args.set", args_set("after", "yes"));

        let (run, cancel) = run_cancellable(&eng, vec![composite, after]).await;

        assert!(cancel.is_cancelled(), "{label}");
        assert!(
            matches!(top_level_outcome(&run), SubActionOutcome::Failed(m) if m.code_or_text() == CANCELLED_CODE),
            "{label}: got {:?}",
            top_level_outcome(&run)
        );
        assert_eq!(run.arg_stack.get("inner"), None, "{label}");
        assert_eq!(run.arg_stack.get("after"), None, "{label}");
    }
}

#[tokio::test]
async fn a_continue_on_error_composite_cannot_carry_a_cancelled_run_into_the_next_step() {
    let eng = engine();
    for (label, composite) in cancelled_child_cases(vec![marker("inner")]) {
        let lenient = SubActionStep {
            continue_on_error: true,
            ..composite
        };
        let after = step("core.args.set", args_set("after", "yes"));

        let (run, _cancel) = run_cancellable(&eng, vec![lenient, after]).await;

        assert!(
            matches!(run.signal, ChainSignal::Aborted),
            "{label}: got {:?}",
            run.signal
        );
        assert_eq!(run.arg_stack.get("after"), None, "{label}");
    }
}

#[tokio::test]
async fn a_cancel_as_a_branchs_last_step_still_stops_the_steps_after_the_branch() {
    let eng = engine();
    for (label, composite) in cancelled_child_cases(Vec::new())
        .into_iter()
        .filter(|(label, _)| *label != "loop")
    {
        let after = step("core.args.set", args_set("after", "yes"));

        let (run, _cancel) = run_cancellable(&eng, vec![composite, after]).await;

        assert!(
            matches!(run.signal, ChainSignal::Aborted),
            "{label}: got {:?}",
            run.signal
        );
        assert_eq!(run.arg_stack.get("after"), None, "{label}");
    }
}

#[tokio::test]
async fn a_loop_cancelled_between_iterations_counts_the_finished_one_and_reports_cancelled() {
    let eng = engine();
    let looped = step("core.logic.loop", loop_cfg(5, inline(vec![cancels()])));

    let (run, _cancel) = run_cancellable(&eng, vec![looped]).await;

    assert_eq!(run.arg_stack.get("loop.exit_reason"), Some(&s("cancelled")));
    assert_eq!(
        run.arg_stack.get("loop.iterations_completed"),
        Some(&Variant::Int(1))
    );
    assert!(
        matches!(top_level_outcome(&run), SubActionOutcome::Failed(m) if m.code_or_text() == CANCELLED_CODE),
        "got {:?}",
        top_level_outcome(&run)
    );
}

#[tokio::test]
async fn a_loop_body_interrupted_mid_way_does_not_count_as_a_finished_iteration() {
    let eng = engine();
    let body = inline(vec![cancels(), marker("inner")]);
    let looped = step("core.logic.loop", loop_cfg(5, body));

    let (run, _cancel) = run_cancellable(&eng, vec![looped]).await;

    assert_eq!(run.arg_stack.get("loop.exit_reason"), Some(&s("cancelled")));
    assert_eq!(
        run.arg_stack.get("loop.iterations_completed"),
        Some(&Variant::Int(0))
    );
}
