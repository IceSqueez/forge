use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

use tokio::time::Instant;

use super::discord_checks::observe_discord_post;
use super::event_checks::{
    Assessment, NamedEvents, Window, assess_causation, observe_absence, observe_event,
};
use super::forge_host::ForgeHost;
use super::journal::Journal;
use super::kick_checks::{
    KickMark, assess_kick_request_count, assess_no_unexpected_kick_requests, observe_kick_request,
};
use super::ledger_checks::{
    assess_no_unexpected_requests, assess_request_count, observe_subscription,
};
use super::log_checks::observe_log_line;
use super::log_tail::LogTail;
use super::obs_checks::{ObsMark, observe_obs_auth, observe_obs_request};
use super::outcome::{
    ActionDetail, ActionReport, Evidence, ExpectationOutcome, FailureCause, RunClock, StepOutcome,
    StepStatus, Verdict,
};
use super::overlay_checks::observe_overlay_content;
use super::steps::{ActionFailure, ActionIndex, DonationFakes, Stimuli, restart_forge};
use super::vtube_checks::{VTubeMark, observe_vtube_auth, observe_vtube_request};
use super::youtube_checks::{
    assess_no_unexpected_youtube_requests, assess_youtube_request_count, observe_youtube_request,
};
use crate::discord::FakeDiscord;
use crate::kick::FakeKick;
use crate::obs::FakeObs;
use crate::overlay::{OverlayMarks, OverlayPages};
use crate::scenario::{Expectation, Scenario, Step, StepAction};
use crate::twitch::FakeTwitch;
use crate::vtube::FakeVTube;
use crate::youtube::FakeYouTube;

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
    let discord_from = match (is_ready_step, session.discord) {
        (false, Some(discord)) => discord.posts().len(),
        _ => 0,
    };
    let obs_from = match (is_ready_step, session.obs) {
        (false, Some(obs)) => ObsMark::of(&obs.ledger()),
        _ => ObsMark::default(),
    };
    let kick_from = match (is_ready_step, session.kick) {
        (false, Some(kick)) => KickMark::of(&kick.ledger()),
        _ => KickMark::default(),
    };
    let youtube_from = match (is_ready_step, session.youtube) {
        (false, Some(youtube)) => youtube.ledger().requests.len(),
        _ => 0,
    };
    let vtube_from = match (is_ready_step, session.vtube) {
        (false, Some(vtube)) => VTubeMark::of(&vtube.ledger()),
        _ => VTubeMark::default(),
    };
    let performed = if is_ready_step {
        Ok(session.ready.clone())
    } else if let StepAction::ForgeRestart { within_ms, offline } = &step.action {
        restart_forge(session.forge, session.donations, *within_ms, offline).await
    } else {
        let live = session.forge.live().await;
        match live.as_ref() {
            Some(live) => {
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
            None => Err(forge_down()),
        }
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
        discord_from,
        obs_from,
        vtube_from,
        kick_from,
        youtube_from,
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

struct ExpectationContext<'s, 'a> {
    session: &'s Session<'a>,
    from: usize,
    marks: OverlayMarks,
    acted: Instant,
    log_tail: LogTail,
    discord_from: usize,
    obs_from: ObsMark,
    vtube_from: VTubeMark,
    kick_from: KickMark,
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
            Expectation::KickRequest(request) => {
                let until = deadline(request.within_ms);
                let (verdict, evidence) = match session.kick {
                    Some(kick) => observe_kick_request(kick, self.kick_from, until, request).await,
                    None => no_fake_kick(),
                };
                (verdict, evidence, Some(until))
            }
            Expectation::KickRequestCount(count) => {
                let (verdict, evidence) = match session.kick {
                    Some(kick) => assess_kick_request_count(&kick.ledger(), count),
                    None => no_fake_kick(),
                };
                (verdict, evidence, None)
            }
            Expectation::KickNoUnexpectedRequests {} => {
                let (verdict, evidence) = match session.kick {
                    Some(kick) => assess_no_unexpected_kick_requests(&kick.ledger()),
                    None => no_fake_kick(),
                };
                (verdict, evidence, None)
            }
            Expectation::YoutubeRequest(request) => {
                let until = deadline(request.within_ms);
                let (verdict, evidence) = match session.youtube {
                    Some(youtube) => {
                        observe_youtube_request(youtube, self.youtube_from, until, request).await
                    }
                    None => no_fake_youtube(),
                };
                (verdict, evidence, Some(until))
            }
            Expectation::YoutubeRequestCount(count) => {
                let (verdict, evidence) = match session.youtube {
                    Some(youtube) => assess_youtube_request_count(&youtube.ledger(), count),
                    None => no_fake_youtube(),
                };
                (verdict, evidence, None)
            }
            Expectation::YoutubeNoUnexpectedRequests {} => {
                let (verdict, evidence) = match session.youtube {
                    Some(youtube) => assess_no_unexpected_youtube_requests(&youtube.ledger()),
                    None => no_fake_youtube(),
                };
                (verdict, evidence, None)
            }
            Expectation::OverlayContent(content) => {
                let until = deadline(content.within_ms);
                let (verdict, evidence) = match session.pages.page(&content.overlay) {
                    Some(page) => {
                        observe_overlay_content(
                            &page,
                            self.marks.of(&content.overlay),
                            until,
                            content,
                            clock,
                        )
                        .await
                    }
                    None => (
                        Verdict::Failed(FailureCause::NoOverlayPage {
                            overlay: content.overlay.clone(),
                        }),
                        Evidence::None,
                    ),
                };
                (verdict, evidence, Some(until))
            }
            Expectation::LogLine(line) => {
                let until = deadline(line.within_ms);
                let (verdict, evidence) =
                    observe_log_line(self.log_tail.clone(), until, line).await;
                (verdict, evidence, Some(until))
            }
            Expectation::DiscordPost(post) => {
                let until = deadline(post.within_ms);
                let (verdict, evidence) = match session.discord {
                    Some(discord) => {
                        observe_discord_post(discord, self.discord_from, until, post).await
                    }
                    None => (Verdict::Failed(FailureCause::NoFakeDiscord), Evidence::None),
                };
                (verdict, evidence, Some(until))
            }
            Expectation::ObsRequest(request) => {
                let until = deadline(request.within_ms);
                let (verdict, evidence) = match session.obs {
                    Some(obs) => observe_obs_request(obs, self.obs_from, until, request).await,
                    None => no_fake_obs(),
                };
                (verdict, evidence, Some(until))
            }
            Expectation::ObsAuth(auth) => {
                let until = deadline(auth.within_ms);
                let (verdict, evidence) = match session.obs {
                    Some(obs) => observe_obs_auth(obs, self.obs_from, until, auth).await,
                    None => no_fake_obs(),
                };
                (verdict, evidence, Some(until))
            }
            Expectation::VtubeRequest(request) => {
                let until = deadline(request.within_ms);
                let (verdict, evidence) = match session.vtube {
                    Some(vtube) => {
                        observe_vtube_request(vtube, self.vtube_from, until, request).await
                    }
                    None => no_fake_vtube(),
                };
                (verdict, evidence, Some(until))
            }
            Expectation::VtubeAuth(auth) => {
                let until = deadline(auth.within_ms);
                let (verdict, evidence) = match session.vtube {
                    Some(vtube) => observe_vtube_auth(vtube, self.vtube_from, until, auth).await,
                    None => no_fake_vtube(),
                };
                (verdict, evidence, Some(until))
            }
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
