mod actions;
mod crowd;
mod donations;
mod expectations;
mod fakes;
mod kick;
mod obs;
mod twitch;
mod vtube;
mod youtube;

use std::collections::{HashMap, HashSet};

use super::model::Scenario;
use super::step::StepAction;
use crate::{EmulatorError, Problem};

pub const MAX_READY_MS: u64 = 300_000;
pub const MAX_WAIT_MS: u64 = 120_000;
pub const MAX_PAUSE_MS: u64 = 5_000;
pub const MAX_KEEPALIVE_MS: u64 = 14_000;
pub const MAX_CROWD_VIEWERS: u32 = 100_000;
pub const MAX_CHATTER_PER_VIEWER: u32 = 20;
pub const MAX_CROWD_MESSAGES: u64 = 1_000_000;
pub const MAX_CROWD_SPACING_MS: u64 = 1_000;

impl Scenario {
    pub fn problems(&self) -> Vec<Problem> {
        let mut validator = Validator::new(self);
        validator.run();
        validator.problems
    }
}

struct NamedEvent {
    location: String,
    single: bool,
}

struct Validator<'a> {
    scenario: &'a Scenario,
    problems: Vec<Problem>,
    fake_twitch: bool,
    fake_discord: bool,
    subscriptions_awaited: HashSet<&'a str>,
    session_subscribed: bool,
    command_matches_sent: u64,
    named_events: HashMap<&'a str, NamedEvent>,
    pages_opened: HashSet<&'a str>,
    obs_online: bool,
    vtube_online: bool,
    kick_live: bool,
    youtube_live: bool,
}

impl<'a> Validator<'a> {
    fn new(scenario: &'a Scenario) -> Self {
        Self {
            scenario,
            problems: Vec::new(),
            fake_twitch: scenario.fakes.twitch.is_some() && scenario.fixture.twitch.is_some(),
            fake_discord: scenario.fakes.discord.is_some(),
            subscriptions_awaited: HashSet::new(),
            session_subscribed: false,
            command_matches_sent: 0,
            named_events: HashMap::new(),
            pages_opened: HashSet::new(),
            obs_online: scenario
                .fakes
                .obs
                .as_ref()
                .is_some_and(|obs| obs.online_at_boot),
            vtube_online: scenario
                .fakes
                .vtube
                .as_ref()
                .is_some_and(|vtube| vtube.online_at_boot),
            kick_live: scenario
                .fakes
                .kick
                .as_ref()
                .is_some_and(|kick| kick.live_at_boot),
            youtube_live: scenario
                .fakes
                .youtube
                .as_ref()
                .is_some_and(|youtube| youtube.live_at_boot),
        }
    }

    fn report(&mut self, location: impl Into<String>, message: impl Into<String>) {
        self.problems.push(Problem::new(location, message));
    }

    fn report_all(&mut self, prefix: &str, problems: Vec<Problem>) {
        self.problems
            .extend(problems.into_iter().map(|problem| problem.under(prefix)));
    }

    fn not_blank(&mut self, location: impl Into<String>, value: &str) {
        if value.trim().is_empty() {
            self.report(location, "must not be blank");
        }
    }

    fn in_range(&mut self, location: impl Into<String>, value: u64, min: u64, max: u64) -> bool {
        let outside = Problem::outside(location, value, min, max);
        let inside = outside.is_none();
        self.problems.extend(outside);
        inside
    }

    fn check_within(&mut self, location: &str, within_ms: u64) {
        self.in_range(format!("{location}.within_ms"), within_ms, 1, MAX_WAIT_MS);
    }

    fn run(&mut self) {
        let scenario = self.scenario;
        self.not_blank("name", &scenario.name);
        self.not_blank("purpose", &scenario.purpose);
        self.check_fixture();
        self.check_fakes();
        if scenario.steps.is_empty() {
            self.report("steps", "needs at least one step");
        }
        for (index, step) in scenario.steps.iter().enumerate() {
            let location = format!("steps[{index}].do");
            if index == 0 && !matches!(step.action, StepAction::ForgeReady { .. }) {
                self.report(
                    &location,
                    "the first step must be forge_ready: nothing is observable before forge is up",
                );
            }
            let location = format!("{location}.{}", step.action.keyword());
            if step.action.needs_fake_twitch() {
                self.require_fake_twitch(&location);
            }
            if step.action.needs_fake_obs() {
                self.require_fake_obs(&location);
            }
            if step.action.needs_fake_vtube() {
                self.require_fake_vtube(&location);
            }
            if step.action.needs_fake_kick() {
                self.require_fake_kick(&location);
            }
            if step.action.needs_fake_youtube() {
                self.require_fake_youtube(&location);
            }
            self.check_action(index, &location, &step.action);
            for (position, expectation) in step.expect.iter().enumerate() {
                let location = format!(
                    "steps[{index}].expect[{position}].{}",
                    expectation.keyword()
                );
                if expectation.needs_fake_twitch() {
                    self.require_fake_twitch(&location);
                }
                if expectation.needs_fake_obs() {
                    self.require_fake_obs(&location);
                }
                if expectation.needs_fake_vtube() {
                    self.require_fake_vtube(&location);
                }
                if expectation.needs_fake_kick() {
                    self.require_fake_kick(&location);
                }
                if expectation.needs_fake_youtube() {
                    self.require_fake_youtube(&location);
                }
                if expectation.needs_fake_discord() && !self.fake_discord {
                    self.report(
                        &location,
                        "needs a fake Discord: add fixture.discord_webhooks and fakes.discord",
                    );
                }
                self.check_expectation(&location, expectation);
            }
        }
    }

    fn check_fixture(&mut self) {
        let fixture = &self.scenario.fixture;
        if let Err(EmulatorError::InvalidFixture { reason }) = fixture.validate() {
            self.report("fixture", reason);
        }
        let mut first_use: HashMap<&str, String> = HashMap::new();
        for action in fixture.actions() {
            if let Some(first) = first_use.get(action.name) {
                let message = format!(
                    "`{}` is already the action of {first}, so run_action could not tell them apart",
                    action.name
                );
                self.report(format!("fixture.{}.action_name", action.location), message);
            } else {
                first_use.insert(action.name, action.location);
            }
        }
    }
}
