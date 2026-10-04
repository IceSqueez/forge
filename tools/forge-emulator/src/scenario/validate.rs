use std::collections::{HashMap, HashSet};
use std::fmt;

use forge_types::Variant;

use super::crowd::{Crowd, CrowdLine, template_problem};
use super::donation::{DonatelloGift, MonobankGift, OfflineGift};
use super::expectation::{
    AbsentEvent, Causation, DiscordPost, Expectation, KickRequestSeen, LogLine, ObservedEvent,
    OverlayContent, RequestCount, TwitchSubscription,
};
use super::matcher::{PayloadMatchers, ValueMatcher};
use super::model::Scenario;
use super::step::{ChatMessage, StepAction};
use crate::EmulatorError;

pub const MAX_READY_MS: u64 = 300_000;
pub const MAX_WAIT_MS: u64 = 120_000;
pub const MAX_PAUSE_MS: u64 = 5_000;
pub const MAX_KEEPALIVE_MS: u64 = 14_000;
pub const MAX_CROWD_VIEWERS: u32 = 100_000;
pub const MAX_CHATTER_PER_VIEWER: u32 = 20;
pub const MAX_CROWD_MESSAGES: u64 = 1_000_000;
pub const MAX_CROWD_SPACING_MS: u64 = 1_000;

const CHAT_SUBSCRIPTION: &str = "channel.chat.message";
const COMMAND_MATCHED: &str = "command.matched";
const COMMAND_POINTER: &str = "/command";
const ACTION_START: &str = "action.start";
const ACTION_NAME_POINTER: &str = "/action_name";
const PROSE_FIELD: &str = "message";
const MENTION_KINDS: [&str; 3] = ["users", "roles", "everyone"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioProblem {
    pub location: String,
    pub message: String,
}

impl fmt::Display for ScenarioProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.location, self.message)
    }
}

impl Scenario {
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
    fake_discord: bool,
    subscriptions_awaited: HashSet<&'a str>,
    session_subscribed: bool,
    command_matches_sent: u64,
    named_events: HashMap<&'a str, NamedEvent>,
    pages_opened: HashSet<&'a str>,
    obs_online: bool,
    vtube_online: bool,
    kick_live: bool,
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
            if step.action.needs_fake_obs() {
                self.require_fake_obs(&location);
            }
            if step.action.needs_fake_vtube() {
                self.require_fake_vtube(&location);
            }
            if step.action.needs_fake_kick() {
                self.require_fake_kick(&location);
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
        self.check_fake_kick();
        self.check_fake_obs();
        self.check_fake_vtube();
        self.check_fake_donations();
        let seeds_webhooks = !scenario.fixture.discord_webhooks.is_empty();
        match (seeds_webhooks, &scenario.fakes.discord) {
            (true, None) => self.report(
                "fakes.discord",
                "is required because the fixture seeds Discord webhooks; without it they have no address to post to",
            ),
            (false, Some(_)) => self.report(
                "fakes.discord",
                "needs fixture.discord_webhooks: the fake answers only the webhooks the fixture seeds",
            ),
            _ => {}
        }
    }

    fn check_fake_donations(&mut self) {
        let scenario = self.scenario;
        match (&scenario.fixture.donatello, &scenario.fakes.donatello) {
            (Some(_), None) => self.report(
                "fakes.donatello",
                "is required because the fixture seeds a Donatello token; without it forge would poll the real Donatello",
            ),
            (None, Some(_)) => self.report(
                "fakes.donatello",
                "needs fixture.donatello: forge polls Donatello only with a seeded token",
            ),
            (Some(_), Some(setup)) => {
                for (index, gift) in setup.history.iter().enumerate() {
                    self.check_donatello_gift(&format!("fakes.donatello.history[{index}]"), gift);
                }
            }
            (None, None) => {}
        }
        match (&scenario.fixture.monobank, &scenario.fakes.monobank) {
            (Some(_), None) => self.report(
                "fakes.monobank",
                "is required because the fixture seeds a monobank token; without it forge would poll the real monobank",
            ),
            (None, Some(_)) => self.report(
                "fakes.monobank",
                "needs fixture.monobank: the fake jar is the one the fixture seeds",
            ),
            (Some(_), Some(setup)) => {
                for (index, gift) in setup.history.iter().enumerate() {
                    self.check_monobank_gift(&format!("fakes.monobank.history[{index}]"), gift);
                }
            }
            (None, None) => {}
        }
    }

    fn check_donatello_gift(&mut self, location: &str, gift: &DonatelloGift) {
        if self.scenario.fakes.donatello.is_none() {
            self.report(
                location,
                "needs a fake Donatello: add fixture.donatello and fakes.donatello",
            );
        }
        self.not_blank(format!("{location}.id"), &gift.id);
        self.not_blank(format!("{location}.amount"), &gift.amount);
    }

    fn check_monobank_gift(&mut self, location: &str, gift: &MonobankGift) {
        if self.scenario.fakes.monobank.is_none() {
            self.report(
                location,
                "needs a fake monobank: add fixture.monobank and fakes.monobank",
            );
        }
        self.not_blank(format!("{location}.id"), &gift.id);
        if gift.amount_minor <= 0 {
            self.report(
                format!("{location}.amount_minor"),
                "must be positive: a jar top-up is money coming in",
            );
        }
    }

    fn check_donation_action(&mut self, location: &str, action: &StepAction) {
        let scenario = self.scenario;
        match action {
            StepAction::DonatelloDonation(gift) => self.check_donatello_gift(location, gift),
            StepAction::MonobankTopUp(gift) => self.check_monobank_gift(location, gift),
            StepAction::DonationsPolled { within_ms } => {
                if scenario.fakes.donatello.is_none() && scenario.fakes.monobank.is_none() {
                    self.report(
                        location,
                        "needs a fake donation service: add fakes.donatello or fakes.monobank",
                    );
                }
                self.in_range(format!("{location}.within_ms"), *within_ms, 1, MAX_WAIT_MS);
            }
            StepAction::ForgeRestart { within_ms, offline } => {
                self.in_range(format!("{location}.within_ms"), *within_ms, 1, MAX_READY_MS);
                for (index, gift) in offline.iter().enumerate() {
                    let at = format!("{location}.offline[{index}]");
                    match gift {
                        OfflineGift::Donatello(gift) => self.check_donatello_gift(&at, gift),
                        OfflineGift::Monobank(gift) => self.check_monobank_gift(&at, gift),
                    }
                }
            }
            _ => {}
        }
    }

    fn require_fake_kick(&mut self, location: &str) {
        let scenario = self.scenario;
        if scenario.fixture.kick.is_none() || scenario.fakes.kick.is_none() {
            self.report(
                location,
                "needs a fake Kick: add fixture.kick and fakes.kick",
            );
        }
    }

    fn check_fake_kick(&mut self) {
        let scenario = self.scenario;
        match (&scenario.fixture.kick, &scenario.fakes.kick) {
            (Some(_), None) => self.report(
                "fakes.kick",
                "is required because the fixture seeds a Kick account; without it forge would reach the real Kick",
            ),
            (None, Some(_)) => self.report(
                "fakes.kick",
                "needs fixture.kick: the fake accepts only the credentials the fixture seeds",
            ),
            (Some(_), Some(setup)) => {
                if setup.chatroom_id == 0 {
                    self.report("fakes.kick.chatroom_id", "must be non-zero");
                }
            }
            (None, None) => {}
        }
    }

    fn check_kick_action(&mut self, location: &str, action: &StepAction) {
        match action {
            StepAction::KickChatJoined { within_ms }
            | StepAction::KickChannelPolled { within_ms } => {
                self.in_range(format!("{location}.within_ms"), *within_ms, 1, MAX_WAIT_MS);
            }
            StepAction::KickChat(message) => {
                self.not_blank(format!("{location}.text"), &message.text);
                self.not_blank(
                    format!("{location}.sender.username"),
                    &message.sender.username,
                );
                if message.sender.user_id == 0 {
                    self.report(format!("{location}.sender.user_id"), "must be non-zero");
                }
            }
            StepAction::KickPusherEvent { event, data } => {
                self.not_blank(format!("{location}.event"), event);
                if !data.is_object() {
                    self.report(
                        format!("{location}.data"),
                        "must be a JSON object: Kick events carry an object encoded as a string",
                    );
                }
            }
            StepAction::KickStream { live } => {
                if *live == self.kick_live {
                    self.report(
                        format!("{location}.live"),
                        format!(
                            "the fake Kick stream is already {}, so this step changes nothing",
                            if *live { "live" } else { "offline" }
                        ),
                    );
                }
                self.kick_live = *live;
            }
            _ => {}
        }
    }

    fn check_kick_request(&mut self, location: &str, request: &KickRequestSeen) {
        self.check_request_count(
            location,
            &RequestCount {
                method: request.method.clone(),
                path: request.path.clone(),
                min: Some(1),
                max: None,
            },
        );
        self.in_range(
            format!("{location}.within_ms"),
            request.within_ms,
            1,
            MAX_WAIT_MS,
        );
    }

    fn require_fake_obs(&mut self, location: &str) {
        if self.scenario.fakes.obs.is_none() {
            self.report(location, "needs a fake OBS: add fixture.obs and fakes.obs");
        }
    }

    fn check_fake_obs(&mut self) {
        let scenario = self.scenario;
        match (&scenario.fixture.obs, &scenario.fakes.obs) {
            (Some(_), None) => self.report(
                "fakes.obs",
                "is required because the fixture seeds an OBS connection; without it forge has no OBS to reach",
            ),
            (None, Some(_)) => self.report(
                "fakes.obs",
                "needs fixture.obs: forge connects only to an OBS the fixture seeds",
            ),
            (Some(connection), Some(config)) => {
                if connection.port != 0 {
                    self.report(
                        "fixture.obs.port",
                        "must be left out: the run fills it with the fake OBS port",
                    );
                }
                for (field, problem) in config.problems() {
                    self.report(format!("fakes.obs.{field}"), problem);
                }
            }
            (None, None) => {}
        }
    }

    fn check_obs_action(&mut self, location: &str, action: &StepAction) {
        let obs = self.scenario.fakes.obs.as_ref();
        match action {
            StepAction::ObsOnline {} => {
                if self.obs_online {
                    self.report(
                        location,
                        "the fake OBS is already running; set fakes.obs.online_at_boot to false to start it from a step",
                    );
                }
                self.obs_online = true;
            }
            StepAction::ObsIdentified { within_ms } => {
                self.in_range(format!("{location}.within_ms"), *within_ms, 1, MAX_WAIT_MS);
            }
            StepAction::ObsRestart { down_ms } => {
                if !self.obs_online {
                    self.report(
                        location,
                        "the fake OBS is not running yet, so there is nothing to restart",
                    );
                }
                self.in_range(format!("{location}.down_ms"), *down_ms, 1, MAX_WAIT_MS);
            }
            StepAction::ObsSceneSwitch { scene }
                if obs.is_some_and(|obs| !obs.declares_scene(scene)) =>
            {
                self.report(
                    format!("{location}.scene"),
                    format!("names scene `{scene}`, which fakes.obs.scenes does not list"),
                );
            }
            StepAction::ObsInputMute { input, .. }
                if obs.is_some_and(|obs| !obs.declares_input(input)) =>
            {
                self.report(
                    format!("{location}.input"),
                    format!("names input `{input}`, which fakes.obs.inputs does not list"),
                );
            }
            _ => {}
        }
    }

    fn require_fake_vtube(&mut self, location: &str) {
        if self.scenario.fakes.vtube.is_none() {
            self.report(
                location,
                "needs a fake VTube Studio: add fixture.vtube and fakes.vtube",
            );
        }
    }

    fn check_fake_vtube(&mut self) {
        let scenario = self.scenario;
        match (&scenario.fixture.vtube, &scenario.fakes.vtube) {
            (Some(_), None) => self.report(
                "fakes.vtube",
                "is required because the fixture seeds a VTube Studio connection; without it forge has no VTube Studio to reach",
            ),
            (None, Some(_)) => self.report(
                "fakes.vtube",
                "needs fixture.vtube: forge connects only to a VTube Studio the fixture seeds",
            ),
            (Some(connection), Some(config)) => {
                if connection.port != 0 {
                    self.report(
                        "fixture.vtube.port",
                        "must be left out: the run fills it with the fake VTube Studio port",
                    );
                }
                self.not_blank("fixture.vtube.token", &connection.token);
                for (field, problem) in config.problems() {
                    self.report(format!("fakes.vtube.{field}"), problem);
                }
            }
            (None, None) => {}
        }
    }

    fn check_vtube_action(&mut self, location: &str, action: &StepAction) {
        let vtube = self.scenario.fakes.vtube.as_ref();
        let undeclared = |field: &str, value: &str, list: &str| {
            (
                format!("{location}.{field}"),
                format!("names `{value}`, which fakes.vtube.{list} does not list"),
            )
        };
        let problem = match action {
            StepAction::VtubeOnline {} => {
                let already = self.vtube_online;
                self.vtube_online = true;
                already.then(|| {
                    (
                        location.to_owned(),
                        "the fake VTube Studio is already running; set fakes.vtube.online_at_boot to false to start it from a step".to_owned(),
                    )
                })
            }
            StepAction::VtubeAuthenticated { within_ms } => {
                self.in_range(format!("{location}.within_ms"), *within_ms, 1, MAX_WAIT_MS);
                None
            }
            StepAction::VtubeHotkey { hotkey }
                if vtube.is_some_and(|v| !v.declares_hotkey(hotkey)) =>
            {
                Some(undeclared("hotkey", hotkey, "models[].hotkeys"))
            }
            StepAction::VtubeModelLoad { model }
                if vtube.is_some_and(|v| !v.declares_model(model)) =>
            {
                Some(undeclared("model", model, "models"))
            }
            StepAction::VtubeItemAdded { file } | StepAction::VtubeItemRemoved { file }
                if vtube.is_some_and(|v| !v.declares_item(file)) =>
            {
                Some(undeclared("file", file, "items"))
            }
            StepAction::VtubeExpression { file, .. }
                if vtube.is_some_and(|v| !v.declares_expression(file)) =>
            {
                Some(undeclared("file", file, "models[].expressions"))
            }
            _ => None,
        };
        if let Some((location, message)) = problem {
            self.report(location, message);
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

    fn check_action(&mut self, index: usize, location: &str, action: &'a StepAction) {
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
                self.subscriptions_awaited
                    .extend(types.iter().map(String::as_str));
            }
            StepAction::Chat(message) => self.check_chat(location, message),
            StepAction::Crowd(crowd) => self.check_crowd(location, crowd),
            StepAction::TwitchEvent {
                subscription_type,
                event,
            } => self.check_twitch_event(location, subscription_type, event),
            StepAction::SessionReconnect { within_ms } => {
                if !self.session_subscribed {
                    self.report(
                        location,
                        "comes before any twitch_subscribed step, so no EventSub session is known to be live",
                    );
                }
                self.in_range(format!("{location}.within_ms"), *within_ms, 1, MAX_WAIT_MS);
            }
            StepAction::OverlayPage { overlay, within_ms } => {
                self.require_seeded_overlay(&format!("{location}.overlay"), overlay);
                self.in_range(format!("{location}.within_ms"), *within_ms, 1, MAX_WAIT_MS);
                self.pages_opened.insert(overlay.as_str());
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
            StepAction::KickChatJoined { .. }
            | StepAction::KickChat(_)
            | StepAction::KickPusherEvent { .. }
            | StepAction::KickStream { .. }
            | StepAction::KickChannelPolled { .. } => self.check_kick_action(location, action),
            StepAction::ObsOnline {}
            | StepAction::ObsIdentified { .. }
            | StepAction::ObsRestart { .. }
            | StepAction::ObsSceneSwitch { .. }
            | StepAction::ObsStream { .. }
            | StepAction::ObsInputMute { .. } => self.check_obs_action(location, action),
            StepAction::VtubeOnline {}
            | StepAction::VtubeAuthenticated { .. }
            | StepAction::VtubeHotkey { .. }
            | StepAction::VtubeModelLoad { .. }
            | StepAction::VtubeModelUnload {}
            | StepAction::VtubeModelConfigChanged {}
            | StepAction::VtubeTracking { .. }
            | StepAction::VtubeItemAdded { .. }
            | StepAction::VtubeItemRemoved { .. }
            | StepAction::VtubeExpression { .. } => self.check_vtube_action(location, action),
            StepAction::DonatelloDonation(_)
            | StepAction::MonobankTopUp(_)
            | StepAction::DonationsPolled { .. }
            | StepAction::ForgeRestart { .. } => self.check_donation_action(location, action),
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

    fn require_seeded_overlay(&mut self, location: &str, overlay: &str) {
        if overlay.trim().is_empty() {
            self.report(location, "must not be blank");
        } else if !self.scenario.fixture.declares_overlay(overlay) {
            let message = format!("names overlay `{overlay}`, which the fixture does not declare");
            self.report(location, message);
        }
    }

    fn defines_action(&self, name: &str) -> bool {
        self.scenario
            .fixture
            .actions()
            .iter()
            .any(|action| action.name == name)
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
        if !self.subscriptions_awaited.contains(CHAT_SUBSCRIPTION) {
            self.report(
                location,
                format!(
                    "sends chat before any twitch_subscribed step waits for {CHAT_SUBSCRIPTION}, so the fake may have nowhere to deliver it"
                ),
            );
        }
    }

    fn check_twitch_event(
        &mut self,
        location: &str,
        subscription_type: &str,
        event: &serde_json::Value,
    ) {
        self.not_blank(format!("{location}.subscription_type"), subscription_type);
        if !event.is_object() {
            self.report(
                format!("{location}.event"),
                "must be a JSON object: EventSub carries every event payload as one",
            );
        }
        if !subscription_type.trim().is_empty()
            && !self.subscriptions_awaited.contains(subscription_type)
        {
            let message = format!(
                "injects `{subscription_type}` before any twitch_subscribed step waits for it, so the fake may have nowhere to deliver it"
            );
            self.report(location, message);
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
            Expectation::TwitchRequestCount(count) | Expectation::KickRequestCount(count) => {
                self.check_request_count(location, count);
            }
            Expectation::KickRequest(request) => self.check_kick_request(location, request),
            Expectation::KickNoUnexpectedRequests {} => {}
            Expectation::OverlayContent(content) => self.check_overlay_content(location, content),
            Expectation::LogLine(line) => self.check_log_line(location, line),
            Expectation::DiscordPost(post) => self.check_discord_post(location, post),
            Expectation::ObsRequest(request) => {
                self.not_blank(format!("{location}.request_type"), &request.request_type);
                self.in_range(
                    format!("{location}.within_ms"),
                    request.within_ms,
                    1,
                    MAX_WAIT_MS,
                );
            }
            Expectation::ObsAuth(auth) => {
                self.in_range(
                    format!("{location}.within_ms"),
                    auth.within_ms,
                    1,
                    MAX_WAIT_MS,
                );
            }
            Expectation::VtubeRequest(request) => {
                self.not_blank(format!("{location}.message_type"), &request.message_type);
                if request.succeeded == Some(true) && request.error_id.is_some() {
                    self.report(
                        format!("{location}.error_id"),
                        "contradicts succeeded: a request answered with an error did not succeed",
                    );
                }
                self.in_range(
                    format!("{location}.within_ms"),
                    request.within_ms,
                    1,
                    MAX_WAIT_MS,
                );
            }
            Expectation::VtubeAuth(auth) => {
                self.in_range(
                    format!("{location}.within_ms"),
                    auth.within_ms,
                    1,
                    MAX_WAIT_MS,
                );
            }
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

    fn check_overlay_content(&mut self, location: &str, content: &OverlayContent) {
        self.require_seeded_overlay(&format!("{location}.overlay"), &content.overlay);
        if !self.pages_opened.contains(content.overlay.as_str()) {
            let message = format!(
                "expects content on `{}` before any overlay_page step opened it, so nothing can be delivered there",
                content.overlay
            );
            self.report(location, message);
        }
        self.in_range(
            format!("{location}.within_ms"),
            content.within_ms,
            1,
            MAX_WAIT_MS,
        );
        if content.values.0.is_empty() {
            self.report(
                format!("{location}.values"),
                "needs at least one content key; an empty frame proves nothing",
            );
        }
        for key in content.values.0.keys() {
            if key.trim().is_empty() {
                self.report(format!("{location}.values"), "has a blank content key");
            }
        }
    }

    fn check_discord_post(&mut self, location: &str, post: &DiscordPost) {
        let declared = self
            .scenario
            .fixture
            .discord_webhooks
            .iter()
            .any(|webhook| webhook.name == post.webhook);
        if !declared {
            self.report(
                format!("{location}.webhook"),
                format!(
                    "expects a post to `{}`, which fixture.discord_webhooks does not declare",
                    post.webhook
                ),
            );
        }
        self.in_range(
            format!("{location}.within_ms"),
            post.within_ms,
            1,
            MAX_WAIT_MS,
        );
        if post.content_contains.as_deref() == Some("") {
            self.report(
                format!("{location}.content_contains"),
                "must not be empty: it would match every post",
            );
        }
        for (index, kind) in post.mention_parse.iter().flatten().enumerate() {
            let repeated = post
                .mention_parse
                .iter()
                .flatten()
                .take(index)
                .any(|earlier| earlier == kind);
            if !MENTION_KINDS.contains(&kind.as_str()) || repeated {
                self.report(
                    format!("{location}.mention_parse[{index}]"),
                    format!(
                        "`{kind}` is repeated or not one of {}",
                        MENTION_KINDS.join(", ")
                    ),
                );
            }
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
