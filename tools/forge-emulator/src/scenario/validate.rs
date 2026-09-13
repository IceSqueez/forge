use std::collections::HashMap;
use std::fmt;

use forge_types::Variant;

use super::crowd::{Crowd, CrowdLine, template_problem};
use super::expectation::{
    AbsentEvent, Causation, Expectation, LogLine, ObservedEvent, RequestCount, TwitchSubscription,
};
use super::matcher::{PayloadMatchers, ValueMatcher};
use super::model::Scenario;
use super::step::{ChatMessage, StepAction};
use crate::EmulatorError;

pub const MAX_READY_MS: u64 = 300_000;
pub const MAX_WAIT_MS: u64 = 120_000;
pub const MAX_PAUSE_MS: u64 = 5_000;
/// forge drops an EventSub session that stays silent for 15 s.
pub const MAX_KEEPALIVE_MS: u64 = 14_000;
pub const MAX_CROWD_VIEWERS: u32 = 1_000;
pub const MAX_CHATTER_PER_VIEWER: u32 = 20;
pub const MAX_CROWD_MESSAGES: u64 = 10_000;
pub const MAX_CROWD_SPACING_MS: u64 = 1_000;

const CHAT_SUBSCRIPTION: &str = "channel.chat.message";
const COMMAND_MATCHED: &str = "command.matched";
const COMMAND_POINTER: &str = "/command";
const ACTION_START: &str = "action.start";
const ACTION_NAME_POINTER: &str = "/action_name";
const PROSE_FIELD: &str = "message";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioProblem {
    /// Path into the scenario document, e.g. `steps[2].expect[0].event.kind`.
    pub location: String,
    pub message: String,
}

impl fmt::Display for ScenarioProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.location, self.message)
    }
}

impl Scenario {
    /// Every statically detectable reason this scenario cannot run or cannot pass.
    pub fn problems(&self) -> Vec<ScenarioProblem> {
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
    problems: Vec<ScenarioProblem>,
    fake_twitch: bool,
    chat_subscribed: bool,
    session_subscribed: bool,
    command_matches_sent: u64,
    named_events: HashMap<&'a str, NamedEvent>,
}

impl<'a> Validator<'a> {
    fn new(scenario: &'a Scenario) -> Self {
        Self {
            scenario,
            problems: Vec::new(),
            fake_twitch: scenario.fakes.twitch.is_some() && scenario.fixture.twitch.is_some(),
            chat_subscribed: false,
            session_subscribed: false,
            command_matches_sent: 0,
            named_events: HashMap::new(),
        }
    }

    fn report(&mut self, location: impl Into<String>, message: impl Into<String>) {
        self.problems.push(ScenarioProblem {
            location: location.into(),
            message: message.into(),
        });
    }

    fn not_blank(&mut self, location: impl Into<String>, value: &str) {
        if value.trim().is_empty() {
            self.report(location, "must not be blank");
        }
    }

    fn in_range(&mut self, location: impl Into<String>, value: u64, min: u64, max: u64) -> bool {
        let inside = (min..=max).contains(&value);
        if !inside {
            self.report(
                location,
                format!("must be between {min} and {max}, got {value}"),
            );
        }
        inside
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
            self.check_action(index, &location, &step.action);
            for (position, expectation) in step.expect.iter().enumerate() {
                let location = format!(
                    "steps[{index}].expect[{position}].{}",
                    expectation.keyword()
                );
                if expectation.needs_fake_twitch() {
                    self.require_fake_twitch(&location);
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
        let mut first_use: HashMap<&str, usize> = HashMap::new();
        for (index, command) in fixture.chat_commands.iter().enumerate() {
            if let Some(first) = first_use.get(command.action_name.as_str()) {
                let message = format!(
                    "`{}` is already the action of chat_commands[{first}], so run_action could not tell them apart",
                    command.action_name
                );
                self.report(
                    format!("fixture.chat_commands[{index}].action_name"),
                    message,
                );
            } else {
                first_use.insert(&command.action_name, index);
            }
        }
    }

    fn check_fakes(&mut self) {
        let scenario = self.scenario;
        match (&scenario.fixture.twitch, &scenario.fakes.twitch) {
            (Some(_), None) => self.report(
                "fakes.twitch",
                "is required because the fixture seeds a Twitch account; without it forge would reach the real Twitch",
            ),
            (None, Some(_)) => self.report(
                "fakes.twitch",
                "needs fixture.twitch: the fake accepts only the credentials the fixture seeds",
            ),
            (Some(_), Some(setup)) => {
                self.in_range(
                    "fakes.twitch.keepalive_interval_ms",
                    setup.keepalive_interval_ms,
                    1,
                    MAX_KEEPALIVE_MS,
                );
            }
            (None, None) => {}
        }
    }

    fn require_fake_twitch(&mut self, location: &str) {
        if !self.fake_twitch {
            self.report(
                location,
                "needs a fake Twitch: add fixture.twitch and fakes.twitch",
            );
        }
    }

    fn check_action(&mut self, index: usize, location: &str, action: &StepAction) {
        match action {
            StepAction::ForgeReady { within_ms } => {
                if index > 0 {
                    self.report(location, "only the first step may be forge_ready");
                }
                self.in_range(format!("{location}.within_ms"), *within_ms, 1, MAX_READY_MS);
            }
            StepAction::TwitchSubscribed { types, within_ms } => {
                self.check_subscription_types(&format!("{location}.types"), types);
                self.in_range(format!("{location}.within_ms"), *within_ms, 1, MAX_WAIT_MS);
                self.session_subscribed = true;
                self.chat_subscribed |= types.iter().any(|kind| kind == CHAT_SUBSCRIPTION);
            }
            StepAction::Chat(message) => self.check_chat(location, message),
            StepAction::Crowd(crowd) => self.check_crowd(location, crowd),
            StepAction::SessionReconnect { within_ms } => {
                if !self.session_subscribed {
                    self.report(
                        location,
                        "comes before any twitch_subscribed step, so no EventSub session is known to be live",
                    );
                }
                self.in_range(format!("{location}.within_ms"), *within_ms, 1, MAX_WAIT_MS);
            }
            StepAction::Pause { ms, reason } => {
                self.in_range(format!("{location}.ms"), *ms, 1, MAX_PAUSE_MS);
                self.not_blank(format!("{location}.reason"), reason);
            }
            StepAction::RunAction { action, .. } => {
                if !self.defines_action(action) {
                    self.report(
                        format!("{location}.action"),
                        format!("names action `{action}`, which the fixture does not define"),
                    );
                }
            }
            StepAction::SetGlobal { name, value, .. } => {
                self.not_blank(format!("{location}.name"), name);
                if let Err(e) = Variant::from_json(value.clone()) {
                    self.report(
                        format!("{location}.value"),
                        format!("cannot be stored as a global: {e}"),
                    );
                }
            }
        }
    }

    fn defines_action(&self, name: &str) -> bool {
        self.scenario
            .fixture
            .chat_commands
            .iter()
            .any(|command| command.action_name == name)
    }

    fn check_subscription_types(&mut self, location: &str, types: &[String]) {
        if types.is_empty() {
            self.report(location, "must list at least one subscription type");
        }
        for (index, kind) in types.iter().enumerate() {
            let at = format!("{location}[{index}]");
            if kind.trim().is_empty() {
                self.report(at, "must not be blank");
            } else if types[..index].contains(kind) {
                self.report(at, format!("repeats `{kind}`"));
            }
        }
    }

    fn require_chat_subscription(&mut self, location: &str) {
        if !self.chat_subscribed {
            self.report(
                location,
                format!(
                    "sends chat before any twitch_subscribed step waits for {CHAT_SUBSCRIPTION}, so the fake may have nowhere to deliver it"
                ),
            );
        }
    }

    fn check_chat(&mut self, location: &str, message: &ChatMessage) {
        self.require_chat_subscription(location);
        self.not_blank(
            format!("{location}.viewer.user_id"),
            &message.viewer.user_id,
        );
        self.not_blank(format!("{location}.viewer.login"), &message.viewer.login);
        self.not_blank(format!("{location}.text"), &message.text);
        self.command_matches_sent += self.commands_fired_by(&message.text);
    }

    fn check_crowd(&mut self, location: &str, crowd: &Crowd) {
        self.require_chat_subscription(location);
        let mut plannable = self.in_range(
            format!("{location}.viewers"),
            u64::from(crowd.viewers),
            1,
            u64::from(MAX_CROWD_VIEWERS),
        );
        if !crowd.chatter.is_empty() {
            plannable &= self.in_range(
                format!("{location}.chatter_per_viewer"),
                u64::from(crowd.chatter_per_viewer),
                1,
                u64::from(MAX_CHATTER_PER_VIEWER),
            );
        }
        self.in_range(
            format!("{location}.spacing_ms"),
            crowd.spacing_ms,
            0,
            MAX_CROWD_SPACING_MS,
        );
        self.check_crowd_commands(location, crowd);
        if crowd.chatter.is_empty() && crowd.command_senders == 0 {
            self.report(
                location,
                "sends no messages: give it chatter or command_senders",
            );
        }
        if crowd.message_count() > MAX_CROWD_MESSAGES {
            plannable = false;
            self.report(
                location,
                format!(
                    "sends {} messages, more than the limit of {MAX_CROWD_MESSAGES}",
                    crowd.message_count()
                ),
            );
        }
        for (index, template) in crowd.chatter.iter().enumerate() {
            let at = format!("{location}.chatter[{index}]");
            let problem = if template.trim().is_empty() {
                Some("must not be blank".to_owned())
            } else {
                template_problem(template)
            };
            if let Some(problem) = problem {
                plannable = false;
                self.report(at, problem);
            }
        }
        if plannable {
            self.check_crowd_plan(location, crowd);
        }
    }

    fn check_crowd_commands(&mut self, location: &str, crowd: &Crowd) {
        if crowd.command_senders > crowd.viewers {
            self.report(
                format!("{location}.command_senders"),
                format!("exceeds the crowd's {} viewers", crowd.viewers),
            );
        }
        if crowd.command_senders > 0 && crowd.commands.is_empty() {
            self.report(
                format!("{location}.commands"),
                "must list at least one command when command_senders is set",
            );
        }
        if crowd.command_senders == 0 && !crowd.commands.is_empty() {
            self.report(
                format!("{location}.command_senders"),
                "must be at least 1 when commands are listed",
            );
        }
        for (index, command) in crowd.commands.iter().enumerate() {
            if self.commands_fired_by(command) == 0 {
                self.report(
                    format!("{location}.commands[{index}]"),
                    format!("`{command}` runs no chat command in the fixture"),
                );
            }
        }
    }

    fn check_crowd_plan(&mut self, location: &str, crowd: &Crowd) {
        let mut reported = vec![false; crowd.chatter.len()];
        for message in crowd.plan() {
            self.command_matches_sent += self.commands_fired_by(&message.text);
            let CrowdLine::Chatter(index) = message.line else {
                continue;
            };
            if reported[index] {
                continue;
            }
            if let Some(phrase) = self.first_command_fired_by(&message.text) {
                reported[index] = true;
                self.report(
                    format!("{location}.chatter[{index}]"),
                    format!(
                        "renders `{}`, which starts with the fixture command `{phrase}`, so chatter would run it",
                        message.text
                    ),
                );
            }
        }
    }

    fn command_phrases(&self) -> impl Iterator<Item = &'a str> + use<'a> {
        let fixture = &self.scenario.fixture;
        fixture
            .twitch
            .iter()
            .flat_map(|_| fixture.chat_commands.iter())
            .map(|command| command.phrase.as_str())
            .filter(|phrase| !phrase.is_empty())
    }

    fn first_command_fired_by(&self, text: &str) -> Option<&'a str> {
        let text = text.to_lowercase();
        self.command_phrases()
            .find(|phrase| text.starts_with(&phrase.to_lowercase()))
    }

    fn commands_fired_by(&self, text: &str) -> u64 {
        let text = text.to_lowercase();
        let fired = self
            .command_phrases()
            .filter(|phrase| text.starts_with(&phrase.to_lowercase()))
            .count();
        u64::try_from(fired).unwrap_or(u64::MAX)
    }

    fn check_expectation(&mut self, location: &str, expectation: &'a Expectation) {
        match expectation {
            Expectation::Event(event) => self.check_observed(location, event),
            Expectation::EventAbsent(absent) => self.check_absent(location, absent),
            Expectation::CausedBy(causation) => self.check_causation(location, causation),
            Expectation::TwitchSubscription(subscription) => {
                self.check_subscription_expectation(location, subscription);
            }
            Expectation::TwitchNoUnexpectedRequests {} => {}
            Expectation::TwitchRequestCount(count) => self.check_request_count(location, count),
            Expectation::LogLine(line) => self.check_log_line(location, line),
        }
    }

    fn check_payload(&mut self, location: &str, payload: &PayloadMatchers) {
        for (pointer, matcher) in &payload.0 {
            if !pointer.is_empty() && !pointer.starts_with('/') {
                self.report(
                    format!("{location}.payload"),
                    format!("key `{pointer}` is not a JSON pointer; write `/{pointer}`"),
                );
            }
            if matches!(matcher, ValueMatcher::Contains(needle) if needle.is_empty()) {
                self.report(
                    format!("{location}.payload[{pointer}].contains"),
                    "must not be empty: it would match every string",
                );
            }
        }
    }

    fn check_observed(&mut self, location: &str, event: &'a ObservedEvent) {
        self.not_blank(format!("{location}.kind"), &event.kind);
        self.in_range(
            format!("{location}.within_ms"),
            event.within_ms,
            1,
            MAX_WAIT_MS,
        );
        self.check_payload(location, &event.payload);
        if event.count.minimum() == 0 {
            self.report(
                format!("{location}.count"),
                "must be at least 1; expect absence with event_absent",
            );
        }
        if event.kind == COMMAND_MATCHED {
            self.check_command_expectation(location, event);
        }
        if event.kind == ACTION_START
            && let Some(action) = event.pattern().pinned_string(ACTION_NAME_POINTER)
            && !self.defines_action(action)
        {
            self.report(
                format!("{location}.payload[{ACTION_NAME_POINTER}]"),
                format!("expects action `{action}` to start, which the fixture does not define"),
            );
        }
        if let Some(name) = &event.name {
            self.declare_name(location, name, event.count.is_single());
        }
    }

    fn check_command_expectation(&mut self, location: &str, event: &ObservedEvent) {
        if self.command_phrases().next().is_none() {
            self.report(
                format!("{location}.kind"),
                format!("expects {COMMAND_MATCHED}, but the fixture defines no chat command"),
            );
            return;
        }
        if let Some(command) = event.pattern().pinned_string(COMMAND_POINTER)
            && !self.command_phrases().any(|phrase| phrase == command)
        {
            self.report(
                format!("{location}.payload[{COMMAND_POINTER}]"),
                format!("expects command `{command}`, which the fixture does not define"),
            );
        }
        let needed = u64::from(event.count.minimum());
        if needed > self.command_matches_sent {
            let message = format!(
                "needs {needed} {COMMAND_MATCHED} event(s), but chat sent up to this step can produce at most {}",
                self.command_matches_sent
            );
            self.report(location, message);
        }
    }

    fn declare_name(&mut self, location: &str, name: &'a str, single: bool) {
        let at = format!("{location}.name");
        if name.trim().is_empty() {
            self.report(at, "must not be blank");
            return;
        }
        if let Some(existing) = self.named_events.get(name) {
            let message = format!("`{name}` is already declared at {}", existing.location);
            self.report(at, message);
            return;
        }
        self.named_events.insert(
            name,
            NamedEvent {
                location: location.to_owned(),
                single,
            },
        );
    }

    fn check_absent(&mut self, location: &str, absent: &AbsentEvent) {
        self.not_blank(format!("{location}.kind"), &absent.kind);
        self.in_range(
            format!("{location}.window_ms"),
            absent.window_ms,
            1,
            MAX_WAIT_MS,
        );
        self.check_payload(location, &absent.payload);
    }

    fn check_causation(&mut self, location: &str, causation: &Causation) {
        if causation.effect == causation.cause {
            self.report(
                location,
                format!(
                    "effect and cause are both `{}`; an event cannot cause itself",
                    causation.effect
                ),
            );
            return;
        }
        for (role, name) in [("effect", &causation.effect), ("cause", &causation.cause)] {
            let problem = match self.named_events.get(name.as_str()) {
                None => Some(format!(
                    "names `{name}`, which no event expectation declares at or before this point"
                )),
                Some(named) if !named.single => Some(format!(
                    "`{name}` may match several events; causation needs a name whose count is 1"
                )),
                Some(_) => None,
            };
            if let Some(problem) = problem {
                self.report(format!("{location}.{role}"), problem);
            }
        }
    }

    fn check_subscription_expectation(
        &mut self,
        location: &str,
        subscription: &TwitchSubscription,
    ) {
        self.not_blank(format!("{location}.type"), &subscription.subscription_type);
        if let Some(version) = &subscription.version {
            self.not_blank(format!("{location}.version"), version);
        }
        self.in_range(
            format!("{location}.within_ms"),
            subscription.within_ms,
            1,
            MAX_WAIT_MS,
        );
    }

    fn check_request_count(&mut self, location: &str, count: &RequestCount) {
        if !count.path.starts_with('/') {
            self.report(format!("{location}.path"), "must start with `/`");
        }
        if let Some(method) = &count.method
            && (method.is_empty() || !method.chars().all(|c| c.is_ascii_uppercase()))
        {
            self.report(
                format!("{location}.method"),
                format!("`{method}` is not an upper-case HTTP method such as GET"),
            );
        }
        match (count.min, count.max) {
            (None, None) => self.report(location, "needs min, max, or both"),
            (Some(min), Some(max)) if min > max => {
                self.report(location, format!("min {min} exceeds max {max}"));
            }
            _ => {}
        }
    }

    fn check_log_line(&mut self, location: &str, line: &LogLine) {
        self.not_blank(format!("{location}.target"), &line.target);
        self.in_range(
            format!("{location}.within_ms"),
            line.within_ms,
            1,
            MAX_WAIT_MS,
        );
        if line.fields.0.is_empty() {
            self.report(
                format!("{location}.fields"),
                "needs at least one structured field; log prose is never matched",
            );
        }
        for key in line.fields.0.keys() {
            if key.trim().is_empty() {
                self.report(format!("{location}.fields"), "has a blank field name");
            } else if key == PROSE_FIELD {
                self.report(
                    format!("{location}.fields.{PROSE_FIELD}"),
                    "is the prose line, which may be reworded; match structured fields instead",
                );
            }
        }
    }
}
