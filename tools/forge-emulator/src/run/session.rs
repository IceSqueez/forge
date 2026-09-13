use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

use tokio::time::Instant;

use super::event_checks::{
    Assessment, NamedEvents, Window, assess_causation, observe_absence, observe_event,
};
use super::journal::Journal;
use super::ledger_checks::{
    assess_no_unexpected_requests, assess_request_count, observe_subscription,
};
use super::log_checks::observe_log_line;
use super::log_tail::LogTail;
use super::outcome::{
    ActionDetail, ActionReport, Evidence, ExpectationOutcome, FailureCause, RunClock, StepOutcome,
    StepStatus, Verdict,
};
use super::steps::{ActionIndex, Stimuli};
use crate::control::ControlClient;
use crate::scenario::{Expectation, Scenario, Step, StepAction};
use crate::twitch::FakeTwitch;

/// `journal` must follow `client`'s push stream, subscribed before the first step runs.
pub struct Session<'a> {
    pub client: &'a ControlClient,
    pub journal: &'a Journal,
    pub twitch: Option<&'a FakeTwitch>,
    pub actions: &'a ActionIndex,
    pub log_dir: PathBuf,
    pub clock: RunClock,
    /// When the forge_ready step counts as satisfied.
    pub ready_at: Instant,
    pub ready: ActionDetail,
}

/// Runs steps in order and stops after the first failing step; `stop` interrupts the current step.
pub async fn execute_steps(
    scenario: &Scenario,
    session: &Session<'_>,
    stop: impl Future<Output = ()>,
) -> Vec<StepOutcome> {
    tokio::pin!(stop);
    let mut named = NamedEvents::new();
    let mut outcomes: Vec<StepOutcome> = Vec::with_capacity(scenario.steps.len());
    for (index, step) in scenario.steps.iter().enumerate() {
        let halted = outcomes
            .last()
            .is_some_and(|last| last.status != StepStatus::Passed);
        if halted {
            outcomes.push(not_run(index, step));
            continue;
        }
        let started = Instant::now();
        let outcome = tokio::select! {
            outcome = run_step(session, index, step, &mut named) => outcome,
            () = &mut stop => interrupted(index, step, Some(session.clock.millis(started))),
        };
        outcomes.push(outcome);
    }
    outcomes
}

async fn run_step(
    session: &Session<'_>,
    index: usize,
    step: &Step,
    named: &mut NamedEvents,
) -> StepOutcome {
    let clock = session.clock;
    let is_ready_step = index == 0 && matches!(step.action, StepAction::ForgeReady { .. });
    let (started, from, log_tail) = if is_ready_step {
        (session.ready_at, 0, LogTail::from_start(&session.log_dir))
    } else {
        (
            Instant::now(),
            session.journal.len(),
            LogTail::from_now(&session.log_dir),
        )
    };
    let performed = if is_ready_step {
        Ok(session.ready.clone())
    } else {
        let stimuli = Stimuli {
            client: session.client,
            twitch: session.twitch,
            actions: session.actions,
        };
        stimuli.perform(&step.action).await
    };
    let acted = if is_ready_step {
        session.ready_at
    } else {
        Instant::now()
    };

    let detail = match performed {
        Ok(detail) => detail,
        Err(failure) => {
            return StepOutcome {
                index,
                keyword: step.action.keyword(),
                status: StepStatus::Failed,
                started_ms: Some(clock.millis(started)),
                acted_ms: None,
                action: Some(ActionReport::Failed {
                    reason: failure.reason,
                    ledger: failure.ledger,
                }),
                expectations: unevaluated(step),
            };
        }
    };

    let context = ExpectationContext {
        session,
        from,
        acted,
        log_tail,
    };
    let mut expectations = Vec::with_capacity(step.expect.len());
    for (position, expectation) in step.expect.iter().enumerate() {
        expectations.push(context.evaluate(position, expectation, named).await);
    }
    let status = if expectations
        .iter()
        .all(|expectation| expectation.verdict == Verdict::Passed)
    {
        StepStatus::Passed
    } else {
        StepStatus::Failed
    };
    StepOutcome {
        index,
        keyword: step.action.keyword(),
        status,
        started_ms: Some(clock.millis(started)),
        acted_ms: Some(clock.millis(acted)),
        action: Some(ActionReport::Done(detail)),
        expectations,
    }
}

struct ExpectationContext<'s, 'a> {
    session: &'s Session<'a>,
    from: usize,
    acted: Instant,
    log_tail: LogTail,
}

impl ExpectationContext<'_, '_> {
    async fn evaluate(
        &self,
        index: usize,
        expectation: &Expectation,
        named: &mut NamedEvents,
    ) -> ExpectationOutcome {
        let session = self.session;
        let clock = session.clock;
        let journal = session.journal;
        let deadline = |ms: u64| self.acted + Duration::from_millis(ms);
        let (verdict, evidence, deadline) = match expectation {
            Expectation::Event(expected) => {
                let window = self.window(deadline(expected.within_ms));
                let assessment = observe_event(journal, window, expected, clock).await;
                if let (Some(name), Some(matched)) = (&expected.name, &assessment.matched) {
                    named.insert(name.clone(), matched.clone());
                }
                split(assessment, Some(window.deadline))
            }
            Expectation::EventAbsent(absent) => {
                let window = self.window(deadline(absent.window_ms));
                split(
                    observe_absence(journal, window, absent, clock).await,
                    Some(window.deadline),
                )
            }
            Expectation::CausedBy(causation) => split(
                journal.read(|view| assess_causation(&view, named, causation, clock)),
                None,
            ),
            Expectation::TwitchSubscription(subscription) => {
                let until = deadline(subscription.within_ms);
                let (verdict, evidence) = match session.twitch {
                    Some(twitch) => observe_subscription(twitch, until, subscription).await,
                    None => no_fake_twitch(),
                };
                (verdict, evidence, Some(until))
            }
            Expectation::TwitchNoUnexpectedRequests {} => {
                let (verdict, evidence) = match session.twitch {
                    Some(twitch) => assess_no_unexpected_requests(&twitch.ledger()),
                    None => no_fake_twitch(),
                };
                (verdict, evidence, None)
            }
            Expectation::TwitchRequestCount(count) => {
                let (verdict, evidence) = match session.twitch {
                    Some(twitch) => assess_request_count(&twitch.ledger(), count),
                    None => no_fake_twitch(),
                };
                (verdict, evidence, None)
            }
            Expectation::LogLine(line) => {
                let until = deadline(line.within_ms);
                let (verdict, evidence) =
                    observe_log_line(self.log_tail.clone(), until, line).await;
                (verdict, evidence, Some(until))
            }
        };
        ExpectationOutcome {
            index,
            keyword: expectation.keyword(),
            verdict,
            deadline_ms: deadline.map(|at| clock.millis(at)),
            evaluated_ms: Some(clock.millis(Instant::now())),
            evidence,
        }
    }

    fn window(&self, deadline: Instant) -> Window {
        Window {
            from: self.from,
            deadline,
        }
    }
}

fn split(
    assessment: Assessment,
    deadline: Option<Instant>,
) -> (Verdict, Evidence, Option<Instant>) {
    (assessment.verdict, assessment.evidence, deadline)
}

fn no_fake_twitch() -> (Verdict, Evidence) {
    (Verdict::Failed(FailureCause::NoFakeTwitch), Evidence::None)
}

fn unevaluated(step: &Step) -> Vec<ExpectationOutcome> {
    step.expect
        .iter()
        .enumerate()
        .map(|(index, expectation)| ExpectationOutcome {
            index,
            keyword: expectation.keyword(),
            verdict: Verdict::NotEvaluated,
            deadline_ms: None,
            evaluated_ms: None,
            evidence: Evidence::None,
        })
        .collect()
}

pub(crate) fn not_run(index: usize, step: &Step) -> StepOutcome {
    StepOutcome {
        index,
        keyword: step.action.keyword(),
        status: StepStatus::NotRun,
        started_ms: None,
        acted_ms: None,
        action: None,
        expectations: unevaluated(step),
    }
}

pub(crate) fn interrupted(index: usize, step: &Step, started_ms: Option<u64>) -> StepOutcome {
    StepOutcome {
        status: StepStatus::Interrupted,
        started_ms,
        ..not_run(index, step)
    }
}
