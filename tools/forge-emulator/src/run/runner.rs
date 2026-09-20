use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

use tokio::time::Instant;

use super::journal::Journal;
use super::log_tail::newest_lines;
use super::outcome::{
    ActionDetail, ForgeEvidence, RunClock, ScenarioOutcome, ScenarioVerdict, StepOutcome,
    StepStatus,
};
use super::session::{Session, execute_steps, interrupted, not_run};
use super::steps::ActionIndex;
use crate::EmulatorError;
use crate::control::EventFilter;
use crate::fixture::Redactions;
use crate::launch::{
    ForgeCommand, GameGuard, LaunchOptions, LaunchedForge, LivePaths, OutputStream, launch_forge,
};
use crate::scenario::{Expectation, Scenario, StepAction};
use crate::twitch::FakeTwitch;

const EVIDENCE_TAIL_LINES: usize = 50;

pub struct RunOptions {
    /// The forge-emulator executable, run as the seeder child.
    pub emulator: PathBuf,
    pub forge: ForgeCommand,
    pub run_root: PathBuf,
    /// forge's `RUST_LOG` filter; `log_line` expectations only see targets it enables.
    pub log_directives: String,
    pub guard: GameGuard,
    pub live: LivePaths,
    pub max_attempts: u32,
    pub shutdown_grace: Duration,
}

/// Always tears forge and the fakes down; launch and harness failures are errors, forge misbehaving is an outcome.
pub async fn run_scenario(
    scenario: &Scenario,
    options: RunOptions,
    stop: impl Future<Output = ()>,
) -> Result<ScenarioOutcome, EmulatorError> {
    let problems = scenario.problems();
    if !problems.is_empty() {
        let listed: Vec<String> = problems.iter().map(ToString::to_string).collect();
        return Err(EmulatorError::InvalidLaunch {
            reason: format!(
                "scenario `{}` is invalid: {}",
                scenario.name,
                listed.join("; ")
            ),
        });
    }
    let Some(StepAction::ForgeReady { within_ms }) = scenario.steps.first().map(|s| &s.action)
    else {
        return Err(EmulatorError::InvalidLaunch {
            reason: "the first step must be forge_ready".to_owned(),
        });
    };
    let clock = RunClock::starting_now();
    tokio::pin!(stop);

    let fake = match (&scenario.fixture.twitch, &scenario.fakes.twitch) {
        (Some(account), Some(setup)) => Some(FakeTwitch::start(setup.config_for(account)).await?),
        _ => None,
    };
    let launch = LaunchOptions {
        emulator: options.emulator,
        forge: options.forge,
        run_root: options.run_root.clone(),
        fixture: scenario.fixture.clone(),
        endpoint_overrides: fake
            .as_ref()
            .map(|fake| fake.endpoint_overrides().to_vec())
            .unwrap_or_default(),
        log_directives: options.log_directives,
        guard: options.guard,
        live: options.live,
        ready_timeout: Duration::from_millis(*within_ms),
        max_attempts: options.max_attempts,
        shutdown_grace: options.shutdown_grace,
    };

    // Why: dropping an unfinished launch drops its ForgeProcess, which kills forge's process group.
    let launched = tokio::select! {
        launched = launch_forge(&launch) => launched,
        () = &mut stop => {
            shut_down_fake(fake).await;
            return Ok(interrupted_before_ready(scenario));
        }
    };
    let LaunchedForge {
        mut process,
        client,
        events,
        seed,
        attempts,
    } = match launched {
        Ok(launched) => launched,
        Err(e) => {
            shut_down_fake(fake).await;
            return Err(e);
        }
    };

    let filters = subscription_filters(scenario);
    if !filters.is_empty()
        && let Err(e) = client.subscribe(&filters).await
    {
        drop(client);
        let _ = process.shutdown(options.shutdown_grace).await;
        shut_down_fake(fake).await;
        return Err(e);
    }
    let version = client.forge_version().await.ok();
    let (journal, feeder) = Journal::follow(events);
    let actions = ActionIndex::from_seed(&seed);
    let log_dir = process.log_dir();
    let session = Session {
        client: &client,
        journal: &journal,
        twitch: fake.as_ref(),
        actions: &actions,
        log_dir: log_dir.clone(),
        clock,
        ready_at: Instant::now(),
        ready: ActionDetail::ForgeReady {
            attempts,
            server_port: seed.server.port,
        },
    };
    let steps = execute_steps(scenario, &session, &mut stop).await;
    drop(session);

    let exited_during_run = !process.is_running();
    let pid = process.pid();
    let data_dir = process.data_dir().to_owned();
    let output = process.output().clone();
    drop(client);
    let (exit, teardown_error) = match process.shutdown(options.shutdown_grace).await {
        Ok(exit) => (Some(exit), None),
        Err(e) => (None, Some(e.to_string())),
    };
    feeder.abort();
    shut_down_fake(fake).await;

    let forge = ForgeEvidence {
        run_root: options.run_root,
        data_dir,
        log_tail: newest_lines(&log_dir, EVIDENCE_TAIL_LINES),
        log_dir,
        pid,
        version,
        attempts,
        exited_during_run,
        exit,
        teardown_error,
        stderr_tail: output.tail(OutputStream::Stderr, EVIDENCE_TAIL_LINES),
        stdout_tail: output.tail(OutputStream::Stdout, EVIDENCE_TAIL_LINES),
    };
    Ok(ScenarioOutcome {
        name: scenario.name.clone(),
        verdict: verdict(&steps, exited_during_run),
        steps,
        forge: Some(forge),
        redactions: Redactions::for_run(&scenario.fixture, Some(&seed)),
    })
}

/// Only the kinds some expectation names; source stays open so a wrong source shows as a near miss.
pub fn subscription_filters(scenario: &Scenario) -> Vec<EventFilter> {
    let mut filters: Vec<EventFilter> = Vec::new();
    let kinds = scenario
        .steps
        .iter()
        .flat_map(|step| &step.expect)
        .filter_map(|expectation| match expectation {
            Expectation::Event(expected) => Some(&expected.kind),
            Expectation::EventAbsent(absent) => Some(&absent.kind),
            _ => None,
        });
    for kind in kinds {
        let filter = EventFilter {
            source: None,
            kind: Some(kind.clone()),
        };
        if !filters.contains(&filter) {
            filters.push(filter);
        }
    }
    filters
}

/// A forge that exited on its own fails the run even when every step passed.
pub fn verdict(steps: &[StepOutcome], forge_exited_during_run: bool) -> ScenarioVerdict {
    if steps
        .iter()
        .any(|step| step.status == StepStatus::Interrupted)
    {
        ScenarioVerdict::Interrupted
    } else if !forge_exited_during_run && steps.iter().all(|step| step.status == StepStatus::Passed)
    {
        ScenarioVerdict::Passed
    } else {
        ScenarioVerdict::Failed
    }
}

async fn shut_down_fake(fake: Option<FakeTwitch>) {
    if let Some(fake) = fake {
        fake.shutdown().await;
    }
}

fn interrupted_before_ready(scenario: &Scenario) -> ScenarioOutcome {
    let steps = scenario
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| match index {
            0 => interrupted(index, step, None),
            _ => not_run(index, step),
        })
        .collect();
    ScenarioOutcome {
        name: scenario.name.clone(),
        verdict: ScenarioVerdict::Interrupted,
        steps,
        forge: None,
        redactions: Redactions::for_run(&scenario.fixture, None),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::scenario::parse_scenario;

    fn step(status: StepStatus) -> StepOutcome {
        StepOutcome {
            index: 0,
            keyword: "pause".to_owned(),
            status,
            started_ms: None,
            acted_ms: None,
            action: None,
            expectations: Vec::new(),
        }
    }

    #[test]
    fn run_verdict_needs_every_step_passed_and_forge_alive_until_teardown() {
        use StepStatus::{Failed, Interrupted, NotRun, Passed};
        for (statuses, exited, expected) in [
            (vec![Passed, Passed], false, ScenarioVerdict::Passed),
            (vec![Passed, Passed], true, ScenarioVerdict::Failed),
            (vec![Passed, Failed, NotRun], false, ScenarioVerdict::Failed),
            (
                vec![Passed, Interrupted, NotRun],
                false,
                ScenarioVerdict::Interrupted,
            ),
            (
                vec![Passed, Interrupted, NotRun],
                true,
                ScenarioVerdict::Interrupted,
            ),
        ] {
            let steps: Vec<StepOutcome> = statuses.iter().copied().map(step).collect();
            assert_eq!(
                verdict(&steps, exited),
                expected,
                "{statuses:?} exited={exited}"
            );
        }
    }

    #[test]
    fn subscription_covers_each_expected_kind_once_across_all_steps_with_any_source() {
        let scenario = parse_scenario(
            Path::new("inline.json"),
            r#"{
              "name": "n", "purpose": "p", "fixture": {},
              "steps": [
                { "do": { "forge_ready": { "within_ms": 1000 } },
                  "expect": [
                    { "event": { "source": "core", "kind": "action.start", "within_ms": 10 } },
                    { "event_absent": { "kind": "trigger.blocked", "window_ms": 10 } }
                  ] },
                { "do": { "pause": { "ms": 1, "reason": "r" } },
                  "expect": [
                    { "event": { "kind": "action.start", "within_ms": 10 } },
                    { "log_line": { "target": "forge::trigger", "fields": { "reason": "x" }, "within_ms": 10 } }
                  ] }
              ]
            }"#,
        )
        .unwrap();
        let kinds: Vec<(Option<forge_events::EventSource>, Option<String>)> =
            subscription_filters(&scenario)
                .into_iter()
                .map(|filter| (filter.source, filter.kind))
                .collect();
        assert_eq!(
            kinds,
            [
                (None, Some("action.start".to_owned())),
                (None, Some("trigger.blocked".to_owned())),
            ]
        );
    }
}
