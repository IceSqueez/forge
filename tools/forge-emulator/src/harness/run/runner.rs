use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio::time::Instant;

use super::forge_host::{ForgeHost, LiveForge, Relaunch, abandon};
use super::journal::Journal;
use super::log_tail::newest_lines;
use super::outcome::{
    ActionDetail, ForgeEvidence, RunClock, ScenarioOutcome, ScenarioVerdict, StepOutcome,
    StepStatus,
};
use super::session::{Session, execute_steps, interrupted, not_run};
use super::steps::{ActionIndex, DonationFakes};
use crate::EmulatorError;
use crate::fakes::discord::FakeDiscord;
use crate::fakes::donatello::{FakeDonatello, FakeDonatelloConfig};
use crate::fakes::kick::{FakeKick, FakeKickConfig};
use crate::fakes::monobank::{FakeJar, FakeMonobank, FakeMonobankConfig};
use crate::fakes::obs::FakeObs;
use crate::fakes::twitch::FakeTwitch;
use crate::fakes::vtube::FakeVTube;
use crate::fakes::youtube::{FakeYouTube, FakeYouTubeConfig};
use crate::fixture::{Fixture, Redactions, SeedReport};
use crate::harness::control::{ControlClient, EventFilter, EventStream};
use crate::harness::launch::{
    CapturedOutput, ForgeCommand, ForgeExit, GameGuard, LaunchOptions, LaunchedForge, LivePaths,
    OutputStream, launch_forge,
};
use crate::harness::overlay::OverlayPages;
use crate::harness::scenario::{Expectation, Scenario, StepAction};

const EVIDENCE_TAIL_LINES: usize = 50;

pub struct RunOptions {
    pub emulator: PathBuf,
    pub forge: ForgeCommand,
    pub run_root: PathBuf,
    pub log_directives: String,
    pub guard: GameGuard,
    pub live: LivePaths,
    pub max_attempts: u32,
    pub shutdown_grace: Duration,
}

pub async fn run_scenario(
    scenario: &Scenario,
    options: RunOptions,
    stop: impl Future<Output = ()>,
) -> Result<ScenarioOutcome, EmulatorError> {
    let ready_ms = validated_ready_ms(scenario)?;
    let clock = RunClock::starting_now();
    tokio::pin!(stop);

    let fakes = RunFakes::start(scenario).await?;
    let launch = launch_options(scenario, &options, &fakes, ready_ms);
    let launched = tokio::select! {
        launched = launch_forge(&launch) => launched,
        () = &mut stop => {
            fakes.shutdown().await;
            return Ok(interrupted_before_ready(scenario));
        }
    };
    let launched = match launched {
        Ok(launched) => launched,
        Err(e) => {
            fakes.shutdown().await;
            return Err(e);
        }
    };
    let grace = options.shutdown_grace;
    let LaunchedForge {
        process,
        client,
        events,
        seed,
        attempts,
        spec,
        emotes,
    } = launched;
    let attached = match attach(scenario, &client, events, &seed).await {
        Ok(attached) => attached,
        Err(e) => {
            drop(client);
            let error = abandon(process, grace, e).await;
            emotes.shutdown().await;
            fakes.shutdown().await;
            return Err(error);
        }
    };
    let log_dir = process.log_dir();
    let data_dir = process.data_dir().to_owned();
    let forge = ForgeHost::new(
        process,
        client,
        attached.feeder,
        attached.journal.clone(),
        Relaunch {
            spec,
            guard: options.guard,
            live: options.live,
            server_port: seed.server.port,
            bearer_token: seed.server.bearer_token.clone(),
            filters: attached.filters,
            shutdown_grace: grace,
        },
    );
    let session = Session {
        forge: &forge,
        journal: &attached.journal,
        twitch: fakes.twitch.as_ref(),
        discord: fakes.discord.as_ref(),
        kick: fakes.kick.as_ref(),
        youtube: fakes.youtube.as_ref(),
        obs: fakes.obs.as_ref(),
        vtube: fakes.vtube.as_ref(),
        donations: fakes.donations(&scenario.fixture),
        actions: &attached.actions,
        pages: &attached.pages,
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
    attached.pages.close();

    let pid = forge.last_pid();
    let teardown = tear_down(forge, grace).await;
    emotes.shutdown().await;
    fakes.shutdown().await;

    let exited_during_run = teardown.exited_during_run;
    let forge = teardown.evidence(ForgeIdentity {
        run_root: options.run_root,
        data_dir,
        log_dir,
        pid,
        version: attached.version,
        attempts,
    });
    Ok(ScenarioOutcome {
        name: scenario.name.clone(),
        verdict: verdict(&steps, exited_during_run),
        steps,
        forge: Some(forge),
        redactions: Redactions::for_run(&scenario.fixture, Some(&seed)),
    })
}

fn validated_ready_ms(scenario: &Scenario) -> Result<u64, EmulatorError> {
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
    match scenario.steps.first().map(|step| &step.action) {
        Some(StepAction::ForgeReady { within_ms }) => Ok(*within_ms),
        _ => Err(EmulatorError::InvalidLaunch {
            reason: "the first step must be forge_ready".to_owned(),
        }),
    }
}

fn launch_options(
    scenario: &Scenario,
    options: &RunOptions,
    fakes: &RunFakes,
    ready_ms: u64,
) -> LaunchOptions {
    LaunchOptions {
        emulator: options.emulator.clone(),
        forge: options.forge.clone(),
        run_root: options.run_root.clone(),
        fixture: fakes.addressed(&scenario.fixture),
        endpoint_overrides: fakes.endpoint_overrides(),
        log_directives: options.log_directives.clone(),
        guard: options.guard.clone(),
        live: options.live.clone(),
        ready_timeout: Duration::from_millis(ready_ms),
        max_attempts: options.max_attempts,
        shutdown_grace: options.shutdown_grace,
    }
}

struct Attached {
    feeder: JoinHandle<()>,
    journal: Journal,
    actions: ActionIndex,
    pages: OverlayPages,
    filters: Vec<EventFilter>,
    version: Option<String>,
}

async fn attach(
    scenario: &Scenario,
    client: &ControlClient,
    events: EventStream,
    seed: &SeedReport,
) -> Result<Attached, EmulatorError> {
    let filters = subscription_filters(scenario);
    if !filters.is_empty() {
        client.subscribe(&filters).await?;
    }
    let version = client.forge_version().await.ok();
    let (journal, feeder) = Journal::follow(events);
    let pages = OverlayPages::for_seed(seed)?;
    Ok(Attached {
        feeder,
        journal,
        actions: ActionIndex::from_seed(seed),
        pages,
        filters,
        version,
    })
}

struct Teardown {
    exited_during_run: bool,
    output: Option<CapturedOutput>,
    exit: Option<ForgeExit>,
    error: Option<String>,
}

async fn tear_down(forge: ForgeHost, grace: Duration) -> Teardown {
    let Some((Some(mut process), client, feeder)) = forge.into_live().map(LiveForge::into_parts)
    else {
        return Teardown {
            exited_during_run: true,
            output: None,
            exit: None,
            error: Some("forge was not running at the end of the run".to_owned()),
        };
    };
    let exited_during_run = !process.is_running();
    let output = process.output().clone();
    drop(client);
    let (exit, error) = match process.shutdown(grace).await {
        Ok(exit) => (Some(exit), None),
        Err(e) => (None, Some(e.to_string())),
    };
    if let Some(feeder) = feeder {
        feeder.abort();
    }
    Teardown {
        exited_during_run,
        output: Some(output),
        exit,
        error,
    }
}

struct ForgeIdentity {
    run_root: PathBuf,
    data_dir: PathBuf,
    log_dir: PathBuf,
    pid: u32,
    version: Option<String>,
    attempts: u32,
}

impl Teardown {
    fn evidence(self, identity: ForgeIdentity) -> ForgeEvidence {
        let tail = |stream| {
            self.output
                .as_ref()
                .map(|output| output.tail(stream, EVIDENCE_TAIL_LINES))
                .unwrap_or_default()
        };
        ForgeEvidence {
            run_root: identity.run_root,
            data_dir: identity.data_dir,
            log_tail: newest_lines(&identity.log_dir, EVIDENCE_TAIL_LINES),
            log_dir: identity.log_dir,
            pid: identity.pid,
            version: identity.version,
            attempts: identity.attempts,
            exited_during_run: self.exited_during_run,
            stderr_tail: tail(OutputStream::Stderr),
            stdout_tail: tail(OutputStream::Stdout),
            exit: self.exit,
            teardown_error: self.error,
        }
    }
}

struct RunFakes {
    twitch: Option<FakeTwitch>,
    discord: Option<FakeDiscord>,
    kick: Option<FakeKick>,
    youtube: Option<FakeYouTube>,
    obs: Option<FakeObs>,
    vtube: Option<FakeVTube>,
    donatello: Option<FakeDonatello>,
    monobank: Option<FakeMonobank>,
}

impl RunFakes {
    async fn start(scenario: &Scenario) -> Result<Self, EmulatorError> {
        let mut fakes = Self {
            twitch: None,
            discord: None,
            kick: None,
            youtube: None,
            obs: None,
            vtube: None,
            donatello: None,
            monobank: None,
        };
        if let Err(e) = fakes.start_each(scenario).await {
            fakes.shutdown().await;
            return Err(e);
        }
        Ok(fakes)
    }

    async fn start_each(&mut self, scenario: &Scenario) -> Result<(), EmulatorError> {
        if let (Some(account), Some(setup)) = (&scenario.fixture.twitch, &scenario.fakes.twitch) {
            self.twitch = Some(FakeTwitch::start(setup.config_for(account)).await?);
        }
        if scenario.fakes.discord.is_some() {
            let names: Vec<String> = scenario
                .fixture
                .discord_webhooks
                .iter()
                .map(|webhook| webhook.name.clone())
                .collect();
            self.discord = Some(FakeDiscord::start(&names).await?);
        }
        if let (Some(account), Some(setup)) = (&scenario.fixture.kick, &scenario.fakes.kick) {
            self.kick = Some(FakeKick::start(FakeKickConfig::for_account(account, setup)).await?);
        }
        if let (Some(account), Some(setup)) = (&scenario.fixture.youtube, &scenario.fakes.youtube) {
            self.youtube =
                Some(FakeYouTube::start(FakeYouTubeConfig::for_account(account, setup)).await?);
        }
        if let Some(config) = &scenario.fakes.obs {
            self.obs = Some(FakeObs::start(config.clone()).await?);
        }
        if let Some(config) = &scenario.fakes.vtube {
            self.vtube = Some(FakeVTube::start(config.clone()).await?);
        }
        let now = time::OffsetDateTime::now_utc();
        if let Some(setup) = &scenario.fakes.donatello {
            let donations = setup
                .history
                .iter()
                .map(|gift| gift.to_fake(now))
                .collect::<Result<Vec<_>, _>>()?;
            self.donatello = Some(
                FakeDonatello::start(FakeDonatelloConfig {
                    donations,
                    ..FakeDonatelloConfig::default()
                })
                .await?,
            );
        }
        if let (Some(account), Some(setup)) = (&scenario.fixture.monobank, &scenario.fakes.monobank)
        {
            let transactions = setup
                .history
                .iter()
                .map(|gift| (account.jar_id.clone(), gift.to_fake(now)))
                .collect();
            self.monobank = Some(
                FakeMonobank::start(FakeMonobankConfig {
                    jars: vec![FakeJar::new(&account.jar_id, &account.jar_id)],
                    transactions,
                    ..FakeMonobankConfig::default()
                })
                .await?,
            );
        }
        Ok(())
    }

    fn addressed(&self, fixture: &Fixture) -> Fixture {
        let fixture = match &self.discord {
            Some(discord) => discord.addressed(fixture),
            None => fixture.clone(),
        };
        let fixture = match &self.obs {
            Some(obs) => obs.addressed(&fixture),
            None => fixture,
        };
        match &self.vtube {
            Some(vtube) => vtube.addressed(&fixture),
            None => fixture,
        }
    }

    fn endpoint_overrides(&self) -> Vec<(&'static str, String)> {
        let mut overrides = self
            .twitch
            .as_ref()
            .map(|twitch| twitch.endpoint_overrides().to_vec())
            .unwrap_or_default();
        overrides.extend(
            self.donatello
                .as_ref()
                .map(FakeDonatello::endpoint_override),
        );
        overrides.extend(self.monobank.as_ref().map(FakeMonobank::endpoint_override));
        overrides.extend(self.discord.as_ref().map(FakeDiscord::endpoint_override));
        overrides.extend(self.kick.iter().flat_map(|kick| kick.endpoint_overrides()));
        overrides.extend(
            self.youtube
                .iter()
                .flat_map(|youtube| youtube.endpoint_overrides()),
        );
        overrides
    }

    fn donations<'a>(&'a self, fixture: &'a Fixture) -> DonationFakes<'a> {
        DonationFakes {
            donatello: self.donatello.as_ref(),
            monobank: self
                .monobank
                .as_ref()
                .zip(fixture.monobank.as_ref())
                .map(|(fake, account)| (fake, account.jar_id.as_str())),
        }
    }

    async fn shutdown(self) {
        if let Some(obs) = self.obs {
            obs.shutdown().await;
        }
        if let Some(vtube) = self.vtube {
            vtube.shutdown().await;
        }
        if let Some(twitch) = self.twitch {
            twitch.shutdown().await;
        }
        if let Some(kick) = self.kick {
            kick.shutdown().await;
        }
        if let Some(youtube) = self.youtube {
            youtube.shutdown().await;
        }
        if let Some(discord) = self.discord {
            discord.shutdown().await;
        }
        if let Some(donatello) = self.donatello {
            donatello.shutdown().await;
        }
        if let Some(monobank) = self.monobank {
            monobank.shutdown().await;
        }
    }
}

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
mod tests {
    use std::path::Path;

    use super::*;
    use crate::harness::scenario::parse_scenario;
    use forge_events::{ActionStarted, TypedEvent};
    use forge_runtime::TRIGGER_BLOCKED_KIND;

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
                (None, Some(ActionStarted::KIND.to_owned())),
                (None, Some(TRIGGER_BLOCKED_KIND.to_owned())),
            ]
        );
    }
}
