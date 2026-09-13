use serde::{Deserialize, Serialize};

use super::actual::actual;
use super::expected::{event_label, expected, matcher, request_label};
use super::story::{action_expected, action_title, step_short, story};
use super::text::code;
use crate::run::{ActionReport, FailureCause, ForgeEvidence, ScenarioOutcome, Verdict};
use crate::scenario::{Expectation, Scenario, StepAction};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BugEntry {
    /// `None` only for a forge exit that no failed step explains.
    pub step: Option<usize>,
    /// `None` when the step's own action failed rather than one of its expectations.
    pub expectation: Option<usize>,
    pub title: String,
    /// `As <actor> I <stimulus>`, without the closing period.
    pub story: String,
    pub expected: String,
    /// The failed expectation or step action in scenario-file JSON, absent optional fields as null.
    pub matcher: Option<String>,
    pub actual: String,
    pub details: Vec<String>,
}

/// One entry per failed step action and per failed expectation, in run order.
pub fn bug_entries(scenario: &Scenario, outcome: &ScenarioOutcome) -> Vec<BugEntry> {
    let mut bugs = Vec::new();
    for step_outcome in &outcome.steps {
        let Some(step) = scenario.steps.get(step_outcome.index) else {
            continue;
        };
        if let Some(ActionReport::Failed { reason, .. }) = &step_outcome.action {
            bugs.push(BugEntry {
                step: Some(step_outcome.index),
                expectation: None,
                title: action_title(&step.action),
                story: story(&step.action),
                expected: action_expected(&step.action),
                matcher: Some(matcher(&step.action)),
                actual: reason.clone(),
                details: Vec::new(),
            });
        }
        for expectation_outcome in &step_outcome.expectations {
            let (Verdict::Failed(cause), Some(expectation)) = (
                &expectation_outcome.verdict,
                step.expect.get(expectation_outcome.index),
            ) else {
                continue;
            };
            let actual = actual(cause, expectation, expectation_outcome);
            bugs.push(BugEntry {
                step: Some(step_outcome.index),
                expectation: Some(expectation_outcome.index),
                title: title(cause, expectation, &step.action),
                story: story(&step.action),
                expected: expected(expectation, &step.action),
                matcher: Some(matcher(expectation)),
                actual: actual.summary,
                details: actual.details,
            });
        }
    }
    if let Some(forge) = outcome
        .forge
        .as_ref()
        .filter(|forge| forge.exited_during_run)
    {
        let exit = exit_line(forge);
        if bugs.is_empty() {
            bugs.push(BugEntry {
                step: None,
                expectation: None,
                title: "forge exited during the run".to_owned(),
                story: format!(
                    "As the streamer I run scenario {} with forge open",
                    code(&outcome.name)
                ),
                expected: "forge keeps running until the emulator shuts it down".to_owned(),
                matcher: None,
                actual: exit,
                details: Vec::new(),
            });
        } else {
            for bug in &mut bugs {
                bug.details.push(exit.clone());
            }
        }
    }
    bugs
}

fn exit_line(forge: &ForgeEvidence) -> String {
    let status = forge
        .exit
        .as_ref()
        .map_or("status unknown", |exit| exit.status.as_str());
    format!("forge exited on its own during the run ({status})")
}

fn title(cause: &FailureCause, expectation: &Expectation, action: &StepAction) -> String {
    let after = step_short(action);
    match (cause, expectation) {
        (FailureCause::NotObserved { .. }, Expectation::Event(event)) => format!(
            "No {} after {after}",
            event_label(&event.kind, &event.payload)
        ),
        (FailureCause::WrongCount { expected, observed }, Expectation::Event(event)) => format!(
            "{} seen {observed} times instead of {expected} after {after}",
            event_label(&event.kind, &event.payload)
        ),
        (FailureCause::Present { .. }, Expectation::EventAbsent(absent)) => {
            format!("Unexpected {} after {after}", code(&absent.kind))
        }
        (FailureCause::StreamGap { .. }, _) => format!(
            "Event stream gap while checking {} after {after}",
            subject(expectation)
        ),
        (FailureCause::StreamClosed, _) => format!(
            "Control connection closed while checking {} after {after}",
            subject(expectation)
        ),
        (FailureCause::UnresolvedName { name }, _) => {
            format!("No event named {} to check causation", code(name))
        }
        (FailureCause::WrongCause { .. }, Expectation::CausedBy(causation)) => format!(
            "{} not caused by {}",
            code(&causation.effect),
            code(&causation.cause)
        ),
        (FailureCause::NoSubscription, Expectation::TwitchSubscription(subscription)) => format!(
            "No live {} subscription after {after}",
            code(&subscription.subscription_type)
        ),
        (FailureCause::UnexpectedRequests { .. }, _) => {
            "forge sent Twitch requests the fake does not model".to_owned()
        }
        (
            FailureCause::RequestCountOutOfRange { observed, .. },
            Expectation::TwitchRequestCount(count),
        ) => format!("{} sent {observed} times", code(&request_label(count))),
        (FailureCause::NoFakeTwitch, _) => {
            format!("{} check has no fake Twitch", code(expectation.keyword()))
        }
        (FailureCause::NoLogLine, Expectation::LogLine(line)) => {
            format!("No {} log line after {after}", code(&line.target))
        }
        (FailureCause::LogUnreadable { .. }, _) => format!("forge log unreadable after {after}"),
        (_, _) => format!("{} failed after {after}", code(expectation.keyword())),
    }
}

fn subject(expectation: &Expectation) -> String {
    match expectation {
        Expectation::Event(event) => event_label(&event.kind, &event.payload),
        Expectation::EventAbsent(absent) => code(&absent.kind),
        _ => code(expectation.keyword()),
    }
}
