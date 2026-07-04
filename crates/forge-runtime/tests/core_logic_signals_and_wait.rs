//! Signal-leaf runners (`break_loop` / `continue_loop` / `stop`) and the
//! `wait_until` delay runner, tested at the runner boundary with a hand-built
//! `RunContext` so the `ControlCell`, cancellation, and timeout behaviours can be
//! observed directly. End-to-end signal flow (absorption / propagation) lives in
//! `core_logic_flow_control.rs`; here we pin what each leaf writes and how
//! `wait_until` terminates.
//!
//! No services, hardware, or network: the condition gate is the in-process rhai
//! evaluator and the timeout case uses a paused tokio clock.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_registry::{
    CancelSignal, ChainExecutor, ChainSignal, ChildChainOutcome, ControlCell, ControlSignal,
    RegistryError, RunContext, StopMark, SubActionRunner, TelemetrySink,
};
use forge_runtime::sub_action_runners::{
    CoreLogicBreakLoopRunner, CoreLogicContinueLoopRunner, CoreLogicStopRunner,
    CoreLogicWaitUntilRunner,
};
use forge_runtime::{ConditionGate, Config};
use forge_types::{ArgStack, EventId, SubActionConfig, SubActionOutcome, SubActionStep, Variant};

struct NullPublisher;
impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

/// A `ChainExecutor` that runs nothing — the signal/wait runners never launch a
/// child chain, so its only job is to satisfy the `RunContext` field.
struct NoopExec;

#[async_trait]
impl ChainExecutor for NoopExec {
    async fn run_child_chain(
        &self,
        _steps: &[SubActionStep],
        arg_stack: &ArgStack,
        _parent_event_id: EventId,
    ) -> Result<ChildChainOutcome, RegistryError> {
        Ok(ChildChainOutcome {
            signal: ChainSignal::Completed,
            arg_stack: arg_stack.clone(),
            telemetry: Vec::new(),
        })
    }
    fn cancel_signal(&self) -> CancelSignal {
        CancelSignal::new()
    }
}

/// Builds a `RunContext` with caller-controlled `control` and `cancel`, so a test
/// can read the cell a leaf writes and drive cancellation.
fn ctx_with<'a>(
    stack: &'a ArgStack,
    publisher: &'a NullPublisher,
    executor: &'a NoopExec,
    control: ControlCell,
    cancel: CancelSignal,
) -> RunContext<'a> {
    RunContext {
        arg_stack: stack,
        index: 0,
        parent_event_id: EventId::new(),
        publisher,
        executor,
        cancel,
        control,
        telemetry: TelemetrySink::new(),
    }
}

// ── signal leaves write their control signal onto the cell ───────────────────

#[tokio::test]
async fn break_and_continue_runners_emit_their_control_signal() {
    let stack = ArgStack::new();
    let cases: [(Box<dyn SubActionRunner>, ControlSignal); 2] = [
        (Box::new(CoreLogicBreakLoopRunner), ControlSignal::Break),
        (
            Box::new(CoreLogicContinueLoopRunner),
            ControlSignal::Continue,
        ),
    ];
    for (runner, expected) in cases {
        let cell = ControlCell::new();
        let ctx = ctx_with(
            &stack,
            &NullPublisher,
            &NoopExec,
            cell.clone(),
            CancelSignal::new(),
        );
        let (tel, _) = runner.execute(&SubActionConfig::new(), &ctx).await;

        assert!(matches!(tel.outcome, SubActionOutcome::Success));
        assert_eq!(
            cell.take(),
            Some(expected),
            "{} must arm its control signal",
            runner.id(),
        );
    }
}

#[tokio::test]
async fn stop_runner_maps_config_to_the_control_stop_signal() {
    let stack = ArgStack::new().set("who".to_owned(), Variant::String("bob".to_owned()));

    // (mark_as, reason) → expected StopMark. Covers the completed/failed split,
    // case-insensitive matching, reason interpolation, and empty-reason filtering.
    let rows: &[(Option<&str>, Option<&str>, StopMark)] = &[
        (
            None,
            None,
            StopMark {
                failed: false,
                reason: None,
            },
        ),
        (
            Some("completed"),
            Some(""),
            StopMark {
                failed: false,
                reason: None,
            },
        ),
        (
            Some("failed"),
            None,
            StopMark {
                failed: true,
                reason: None,
            },
        ),
        (
            Some("FAILED"),
            None,
            StopMark {
                failed: true,
                reason: None,
            },
        ),
        (
            Some("failed"),
            Some("%who%"),
            StopMark {
                failed: true,
                reason: Some("bob".to_owned()),
            },
        ),
    ];

    for (mark_as, reason, expected) in rows {
        let mut cfg = SubActionConfig::new();
        if let Some(m) = mark_as {
            cfg.insert("mark_as".to_owned(), Variant::String((*m).to_owned()));
        }
        if let Some(r) = reason {
            cfg.insert("reason".to_owned(), Variant::String((*r).to_owned()));
        }
        let cell = ControlCell::new();
        let ctx = ctx_with(
            &stack,
            &NullPublisher,
            &NoopExec,
            cell.clone(),
            CancelSignal::new(),
        );
        let (tel, _) = CoreLogicStopRunner.execute(&cfg, &ctx).await;

        assert!(matches!(tel.outcome, SubActionOutcome::Success));
        assert_eq!(
            cell.take(),
            Some(ControlSignal::Stop(expected.clone())),
            "mark_as={mark_as:?} reason={reason:?}",
        );
    }
}

// ── wait_until termination ───────────────────────────────────────────────────

fn wait_runner() -> CoreLogicWaitUntilRunner {
    CoreLogicWaitUntilRunner::new(Arc::new(ConditionGate::new(&Config::default())))
}

fn wait_cfg(condition: &str, timeout_ms: i64) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert(
        "condition".to_owned(),
        Variant::String(condition.to_owned()),
    );
    c.insert("poll_interval_ms".to_owned(), Variant::Int(100));
    c.insert("timeout_ms".to_owned(), Variant::Int(timeout_ms));
    c
}

#[tokio::test]
async fn wait_until_returns_immediately_when_the_condition_already_holds() {
    let stack = ArgStack::new();
    let ctx = ctx_with(
        &stack,
        &NullPublisher,
        &NoopExec,
        ControlCell::new(),
        CancelSignal::new(),
    );
    let (tel, out) = wait_runner()
        .execute(&wait_cfg("1 == 1", 30_000), &ctx)
        .await;

    assert!(matches!(tel.outcome, SubActionOutcome::Success));
    assert_eq!(
        out.unwrap().get("wait.timed_out"),
        Some(&Variant::Bool(false)),
        "a satisfied condition must not be reported as a timeout",
    );
}

#[tokio::test(start_paused = true)]
async fn wait_until_times_out_with_success_and_a_timed_out_flag() {
    // A never-true condition must end as Success with wait.timed_out=true — a
    // timeout is a normal terminal state, NOT a Failed outcome. The paused clock
    // auto-advances over the poll sleep so the test does not wait in real time.
    let stack = ArgStack::new();
    let ctx = ctx_with(
        &stack,
        &NullPublisher,
        &NoopExec,
        ControlCell::new(),
        CancelSignal::new(),
    );
    let (tel, out) = wait_runner().execute(&wait_cfg("1 == 2", 100), &ctx).await;

    assert!(matches!(tel.outcome, SubActionOutcome::Success));
    assert_eq!(
        out.unwrap().get("wait.timed_out"),
        Some(&Variant::Bool(true)),
    );
}

#[tokio::test]
async fn wait_until_short_circuits_when_cancellation_is_observed() {
    // With an always-false condition the runner would otherwise time out; a
    // pre-tripped cancel signal must break the poll loop first, so timed_out stays
    // false. This is what distinguishes "cancelled" from "timed out".
    let stack = ArgStack::new();
    let cancel = CancelSignal::new();
    cancel.cancel();
    let ctx = ctx_with(
        &stack,
        &NullPublisher,
        &NoopExec,
        ControlCell::new(),
        cancel,
    );
    let (tel, out) = wait_runner()
        .execute(&wait_cfg("1 == 2", 30_000), &ctx)
        .await;

    assert!(matches!(tel.outcome, SubActionOutcome::Success));
    assert_eq!(
        out.unwrap().get("wait.timed_out"),
        Some(&Variant::Bool(false)),
        "cancellation must short-circuit before the timeout fires",
    );
}
