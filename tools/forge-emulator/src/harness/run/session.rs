use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

use tokio::time::Instant;

use super::discord_checks::observe_discord_post;
use super::event_checks::{
    Assessment, NamedEvents, Window, assess_causation, observe_absence, observe_event,
};
use super::forge_host::ForgeHost;
use super::http_checks::LedgerMark;
use super::journal::Journal;
use super::kick_checks::{
    assess_kick_request_count, assess_no_unexpected_kick_requests, observe_kick_request,
};
use super::ledger_checks::{
    assess_no_unexpected_requests, assess_request_count, observe_subscription,
};
use super::log_checks::observe_log_line;
use super::log_tail::LogTail;
use super::obs_checks::{observe_obs_auth, observe_obs_request};
use super::outcome::{
    ActionDetail, ActionReport, Evidence, ExpectationOutcome, FailureCause, RunClock, StepOutcome,
    StepStatus, Verdict,
};
use super::overlay_checks::observe_overlay_content;
use super::steps::{ActionFailure, ActionIndex, DonationFakes, Stimuli, restart_forge};
use super::vtube_checks::{observe_vtube_auth, observe_vtube_request};
use super::youtube_checks::{
    assess_no_unexpected_youtube_requests, assess_youtube_request_count, observe_youtube_request,
};
use crate::fakes::discord::FakeDiscord;
use crate::fakes::kick::FakeKick;
use crate::fakes::obs::FakeObs;
use crate::fakes::twitch::FakeTwitch;
use crate::fakes::vtube::FakeVTube;
use crate::fakes::youtube::FakeYouTube;
use crate::harness::overlay::{OverlayMarks, OverlayPages};
use crate::harness::scenario::{
    DiscordPost, Expectation, HttpRequestSeen, LogLine, ObsAuthOutcome, ObsRequestSeen,
    OverlayContent, Scenario, Step, StepAction, TwitchSubscription, VTubeAuthOutcome,
    VTubeRequestSeen,
};

pub struct Session<'a> {
    pub forge: &'a ForgeHost,
    pub journal: &'a Journal,
    pub twitch: Option<&'a FakeTwitch>,
    pub discord: Option<&'a FakeDiscord>,
    pub kick: Option<&'a FakeKick>,
    pub youtube: Option<&'a FakeYouTube>,
    pub obs: Option<&'a FakeObs>,
    pub vtube: Option<&'a FakeVTube>,
    pub donations: DonationFakes<'a>,
    pub actions: &'a ActionIndex,
    pub pages: &'a OverlayPages,
    pub log_dir: PathBuf,
    pub clock: RunClock,
    pub ready_at: Instant,
    pub ready: ActionDetail,
}

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
    let marks = session.pages.marks();
    let ledgers = if is_ready_step {
        LedgerMarks::default()
    } else {
        LedgerMarks::of(session)
    };
    let performed = perform(session, step, is_ready_step).await;
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
                keyword: step.action.keyword().to_owned(),
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

    let mut marks = marks;
    if let StepAction::OverlayPage { overlay, .. } = &step.action {
        marks.reopened(overlay);
    }
    let context = ExpectationContext {
        session,
        from,
        marks,
        acted,
        log_tail,
        discord_from: ledgers.discord,
        obs_from: ledgers.obs,
        vtube_from: ledgers.vtube,
        kick_from: ledgers.kick,
        youtube_from: ledgers.youtube,
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
        keyword: step.action.keyword().to_owned(),
        status,
        started_ms: Some(clock.millis(started)),
        acted_ms: Some(clock.millis(acted)),
        action: Some(ActionReport::Done(detail)),
        expectations,
    }
}

#[derive(Default)]
struct LedgerMarks {
    discord: usize,
    obs: LedgerMark,
    vtube: LedgerMark,
    kick: LedgerMark,
    youtube: usize,
}

impl LedgerMarks {
    fn of(session: &Session<'_>) -> Self {
        let mark = |sessions, requests| LedgerMark { sessions, requests };
        Self {
            discord: session.discord.map_or(0, |discord| discord.posts().len()),
            obs: session.obs.map_or_else(LedgerMark::default, |obs| {
                let ledger = obs.ledger();
                mark(ledger.sessions.len(), ledger.requests.len())
            }),
            vtube: session.vtube.map_or_else(LedgerMark::default, |vtube| {
                let ledger = vtube.ledger();
                mark(ledger.sessions.len(), ledger.requests.len())
            }),
            kick: session.kick.map_or_else(LedgerMark::default, |kick| {
                let ledger = kick.ledger();
                mark(ledger.sessions.len(), ledger.requests.len())
            }),
            youtube: session
                .youtube
                .map_or(0, |youtube| youtube.ledger().requests.len()),
        }
    }
}

async fn perform(
    session: &Session<'_>,
    step: &Step,
    is_ready_step: bool,
) -> Result<ActionDetail, ActionFailure> {
    if is_ready_step {
        return Ok(session.ready.clone());
    }
    if let StepAction::ForgeRestart { within_ms, offline } = &step.action {
        return restart_forge(session.forge, session.donations, *within_ms, offline).await;
    }
    let live = session.forge.live().await;
    let live = live.as_ref().ok_or_else(forge_down)?;
    let stimuli = Stimuli {
        client: &live.client,
        twitch: session.twitch,
        kick: session.kick,
        youtube: session.youtube,
        obs: session.obs,
        vtube: session.vtube,
        donations: session.donations,
        actions: session.actions,
        pages: session.pages,
    };
    stimuli.perform(&step.action).await
}

struct ExpectationContext<'s, 'a> {
    session: &'s Session<'a>,
    from: usize,
    marks: OverlayMarks,
    acted: Instant,
    log_tail: LogTail,
    discord_from: usize,
    obs_from: LedgerMark,
    vtube_from: LedgerMark,
    kick_from: LedgerMark,
    youtube_from: usize,
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
        let (verdict, evidence, deadline) = match expectation {
            Expectation::Event(expected) => {
                let window = self.window(self.until(expected.within_ms));
                let assessment = observe_event(journal, window, expected, clock).await;
                if let (Some(name), Some(matched)) = (&expected.name, &assessment.matched) {
                    named.insert(name.clone(), matched.clone());
                }
                split(assessment, Some(window.deadline))
            }
            Expectation::EventAbsent(absent) => {
                let window = self.window(self.until(absent.window_ms));
                split(
                    observe_absence(journal, window, absent, clock).await,
                    Some(window.deadline),
                )
            }
            Expectation::CausedBy(causation) => split(
                journal.read(|view| assess_causation(&view, named, causation, clock)),
                None,
            ),
            Expectation::TwitchNoUnexpectedRequests {} => {
                unbounded(self.twitch(|twitch| assess_no_unexpected_requests(&twitch.ledger())))
            }
            Expectation::TwitchRequestCount(count) => {
                unbounded(self.twitch(|twitch| assess_request_count(&twitch.ledger(), count)))
            }
            Expectation::KickRequestCount(count) => {
                unbounded(self.kick(|kick| assess_kick_request_count(&kick.ledger(), count)))
            }
            Expectation::KickNoUnexpectedRequests {} => {
                unbounded(self.kick(|kick| assess_no_unexpected_kick_requests(&kick.ledger())))
            }
            Expectation::YoutubeRequestCount(count) => unbounded(
                self.youtube(|youtube| assess_youtube_request_count(&youtube.ledger(), count)),
            ),
            Expectation::YoutubeNoUnexpectedRequests {} => unbounded(
                self.youtube(|youtube| assess_no_unexpected_youtube_requests(&youtube.ledger())),
            ),
            Expectation::TwitchSubscription(subscription) => {
                self.twitch_subscription(subscription).await
            }
            Expectation::KickRequest(request) => self.kick_request(request).await,
            Expectation::YoutubeRequest(request) => self.youtube_request(request).await,
            Expectation::OverlayContent(content) => self.overlay_content(content).await,
            Expectation::LogLine(line) => self.log_line(line).await,
            Expectation::DiscordPost(post) => self.discord_post(post).await,
            Expectation::ObsRequest(request) => self.obs_request(request).await,
            Expectation::ObsAuth(auth) => self.obs_auth(auth).await,
            Expectation::VtubeRequest(request) => self.vtube_request(request).await,
            Expectation::VtubeAuth(auth) => self.vtube_auth(auth).await,
        };
        ExpectationOutcome {
            index,
            keyword: expectation.keyword().to_owned(),
            verdict,
            deadline_ms: deadline.map(|at| clock.millis(at)),
            evaluated_ms: Some(clock.millis(Instant::now())),
            evidence,
        }
    }

    async fn twitch_subscription(&self, subscription: &TwitchSubscription) -> Observation {
        let until = self.until(subscription.within_ms);
        let assessed = match self.session.twitch {
            Some(twitch) => observe_subscription(twitch, until, subscription).await,
            None => no_fake_twitch(),
        };
        bounded(until, assessed)
    }

    async fn kick_request(&self, request: &HttpRequestSeen) -> Observation {
        let until = self.until(request.within_ms);
        let assessed = match self.session.kick {
            Some(kick) => observe_kick_request(kick, self.kick_from, until, request).await,
            None => no_fake_kick(),
        };
        bounded(until, assessed)
    }

    async fn youtube_request(&self, request: &HttpRequestSeen) -> Observation {
        let until = self.until(request.within_ms);
        let assessed = match self.session.youtube {
            Some(youtube) => {
                observe_youtube_request(youtube, self.youtube_from, until, request).await
            }
            None => no_fake_youtube(),
        };
        bounded(until, assessed)
    }

    async fn log_line(&self, line: &LogLine) -> Observation {
        let until = self.until(line.within_ms);
        bounded(
            until,
            observe_log_line(self.log_tail.clone(), until, line).await,
        )
    }

    async fn discord_post(&self, post: &DiscordPost) -> Observation {
        let until = self.until(post.within_ms);
        let assessed = match self.session.discord {
            Some(discord) => observe_discord_post(discord, self.discord_from, until, post).await,
            None => (Verdict::Failed(FailureCause::NoFakeDiscord), Evidence::None),
        };
        bounded(until, assessed)
    }

    async fn obs_request(&self, request: &ObsRequestSeen) -> Observation {
        let until = self.until(request.within_ms);
        let assessed = match self.session.obs {
            Some(obs) => observe_obs_request(obs, self.obs_from, until, request).await,
            None => no_fake_obs(),
        };
        bounded(until, assessed)
    }

    async fn obs_auth(&self, auth: &ObsAuthOutcome) -> Observation {
        let until = self.until(auth.within_ms);
        let assessed = match self.session.obs {
            Some(obs) => observe_obs_auth(obs, self.obs_from, until, auth).await,
            None => no_fake_obs(),
        };
        bounded(until, assessed)
    }

    async fn vtube_request(&self, request: &VTubeRequestSeen) -> Observation {
        let until = self.until(request.within_ms);
        let assessed = match self.session.vtube {
            Some(vtube) => observe_vtube_request(vtube, self.vtube_from, until, request).await,
            None => no_fake_vtube(),
        };
        bounded(until, assessed)
    }

    async fn vtube_auth(&self, auth: &VTubeAuthOutcome) -> Observation {
        let until = self.until(auth.within_ms);
        let assessed = match self.session.vtube {
            Some(vtube) => observe_vtube_auth(vtube, self.vtube_from, until, auth).await,
            None => no_fake_vtube(),
        };
        bounded(until, assessed)
    }

    async fn overlay_content(&self, content: &OverlayContent) -> Observation {
        let until = self.until(content.within_ms);
        let assessed = match self.session.pages.page(&content.overlay) {
            Some(page) => {
                let mark = self.marks.of(&content.overlay);
                observe_overlay_content(&page, mark, until, content, self.session.clock).await
            }
            None => (
                Verdict::Failed(FailureCause::NoOverlayPage {
                    overlay: content.overlay.clone(),
                }),
                Evidence::None,
            ),
        };
        bounded(until, assessed)
    }

    fn until(&self, ms: u64) -> Instant {
        self.acted + Duration::from_millis(ms)
    }

    fn twitch(
        &self,
        assess: impl FnOnce(&FakeTwitch) -> (Verdict, Evidence),
    ) -> (Verdict, Evidence) {
        self.session.twitch.map_or_else(no_fake_twitch, assess)
    }

    fn kick(&self, assess: impl FnOnce(&FakeKick) -> (Verdict, Evidence)) -> (Verdict, Evidence) {
        self.session.kick.map_or_else(no_fake_kick, assess)
    }

    fn youtube(
        &self,
        assess: impl FnOnce(&FakeYouTube) -> (Verdict, Evidence),
    ) -> (Verdict, Evidence) {
        self.session.youtube.map_or_else(no_fake_youtube, assess)
    }

    fn window(&self, deadline: Instant) -> Window {
        Window {
            from: self.from,
            deadline,
        }
    }
}

type Observation = (Verdict, Evidence, Option<Instant>);

fn split(assessment: Assessment, deadline: Option<Instant>) -> Observation {
    (assessment.verdict, assessment.evidence, deadline)
}

fn bounded(until: Instant, (verdict, evidence): (Verdict, Evidence)) -> Observation {
    (verdict, evidence, Some(until))
}

fn unbounded((verdict, evidence): (Verdict, Evidence)) -> Observation {
    (verdict, evidence, None)
}

fn forge_down() -> ActionFailure {
    ActionFailure {
        reason: "forge is not running: an earlier restart did not bring it back".to_owned(),
        ledger: None,
    }
}

fn no_fake_twitch() -> (Verdict, Evidence) {
    (Verdict::Failed(FailureCause::NoFakeTwitch), Evidence::None)
}

fn no_fake_obs() -> (Verdict, Evidence) {
    (Verdict::Failed(FailureCause::NoFakeObs), Evidence::None)
}

fn no_fake_kick() -> (Verdict, Evidence) {
    (Verdict::Failed(FailureCause::NoFakeKick), Evidence::None)
}

fn no_fake_youtube() -> (Verdict, Evidence) {
    (Verdict::Failed(FailureCause::NoFakeYouTube), Evidence::None)
}

fn no_fake_vtube() -> (Verdict, Evidence) {
    (Verdict::Failed(FailureCause::NoFakeVTube), Evidence::None)
}

fn unevaluated(step: &Step) -> Vec<ExpectationOutcome> {
    step.expect
        .iter()
        .enumerate()
        .map(|(index, expectation)| ExpectationOutcome {
            index,
            keyword: expectation.keyword().to_owned(),
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
        keyword: step.action.keyword().to_owned(),
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
