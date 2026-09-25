use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::EmulatorError;
use crate::fixture::{DEFAULT_QUEUE_NAME, Fixture};
use crate::scenario::FakeTwitchSetup;

pub const MAX_SENDERS: u32 = 1_000_000;
pub const MAX_RATE: u32 = 100_000;
pub const MAX_HOLD_SECS: u64 = 3_600;
const BYTE_ORDER_MARK: char = '\u{feff}';

/// A throughput run: a seeded forge, a set of connected overlay pages, and a load that steps
/// through `ramp.rates` until forge degrades, then recovers, bursts, and recovers again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StressProfile {
    pub name: String,
    pub purpose: String,
    pub fixture: Fixture,
    #[serde(default)]
    pub fakes: FakeTwitchSetup,
    /// Persisted globals set over the control socket before any load, as a user would create
    /// the counters their actions increment.
    #[serde(default)]
    pub globals: serde_json::Map<String, serde_json::Value>,
    /// Fixture overlays that get a connected browser source for the whole run.
    #[serde(default)]
    pub pages: Vec<String>,
    /// Distinct chatters the load cycles through.
    pub senders: u32,
    pub stimuli: Vec<Stimulus>,
    pub ramp: Ramp,
    pub burst: Burst,
    pub recovery_secs: u64,
    #[serde(default = "default_sample_ms")]
    pub sample_ms: u64,
    pub knee: KneeRule,
}

/// One kind of inbound event. A `weight` makes it a share of the stepped flood rate; a
/// `per_minute` makes it a steady trickle that ignores the ramp, as alerts do on a real stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stimulus {
    pub kind: StimulusKind,
    #[serde(default)]
    pub weight: Option<u32>,
    #[serde(default)]
    pub per_minute: Option<u32>,
    /// The command phrase for `command`; unused otherwise.
    #[serde(default)]
    pub text: Option<String>,
    /// The fixture actions each such event is expected to start.
    pub runs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StimulusKind {
    Chat,
    Command,
    Follow,
    Subscribe,
    Cheer,
    Reward,
}

impl StimulusKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Command => "command",
            Self::Follow => "follow",
            Self::Subscribe => "subscribe",
            Self::Cheer => "cheer",
            Self::Reward => "reward",
        }
    }

    /// The EventSub type that carries it.
    pub fn subscription_type(self) -> &'static str {
        match self {
            Self::Chat | Self::Command => "channel.chat.message",
            Self::Follow => "channel.follow",
            Self::Subscribe => "channel.subscribe",
            Self::Cheer => "channel.cheer",
            Self::Reward => "channel.channel_points_custom_reward_redemption.add",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ramp {
    /// Flood rates in events per second, stepped in order.
    pub rates: Vec<u32>,
    pub hold_secs: u64,
}

/// Fired after the ramp at `multiplier` times the last step forge sustained.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Burst {
    pub multiplier: f64,
    pub secs: u64,
}

/// A step is degraded when the generator fell short of the target rate, forge logged one of
/// `fatal_warnings`, or - while the observer lost nothing, so its counts hold - the watched
/// actions' unfinished work exceeds `max_backlog_secs` of the step's rate or any was skipped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KneeRule {
    /// The actions judged: the flood's own work, not a queue meant to hold a backlog.
    pub watch: Vec<String>,
    pub max_backlog_secs: f64,
    pub min_achieved_share: f64,
    /// WARN/ERROR messages that mean work was lost; any of them logged during a step degrades it.
    #[serde(default)]
    pub fatal_warnings: Vec<String>,
}

fn default_sample_ms() -> u64 {
    1_000
}

pub fn load_profile(path: &Path) -> Result<StressProfile, EmulatorError> {
    let unreadable = |reason: String| EmulatorError::StressProfileUnreadable {
        path: path.to_owned(),
        reason,
    };
    let text = std::fs::read_to_string(path).map_err(|e| unreadable(e.to_string()))?;
    let text = text.strip_prefix(BYTE_ORDER_MARK).unwrap_or(&text);
    let profile: StressProfile =
        serde_json::from_str(text).map_err(|e| unreadable(e.to_string()))?;
    let problems = profile.problems();
    if problems.is_empty() {
        Ok(profile)
    } else {
        Err(EmulatorError::StressProfileInvalid {
            path: path.to_owned(),
            problems,
        })
    }
}

impl StressProfile {
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if let Err(e) = self.fixture.validate() {
            problems.push(e.to_string());
        }
        if self.fixture.twitch.is_none() {
            problems
                .push("fixture.twitch is required: every stimulus arrives over Twitch".to_owned());
        }
        if self.senders == 0 || self.senders > MAX_SENDERS {
            problems.push(format!(
                "senders must be between 1 and {MAX_SENDERS}, got {}",
                self.senders
            ));
        }
        if self.ramp.rates.is_empty() {
            problems.push("ramp.rates is empty".to_owned());
        }
        for (index, rate) in self.ramp.rates.iter().enumerate() {
            if *rate == 0 || *rate > MAX_RATE {
                problems.push(format!(
                    "ramp.rates[{index}] must be between 1 and {MAX_RATE}, got {rate}"
                ));
            }
        }
        for (label, secs) in [
            ("ramp.hold_secs", self.ramp.hold_secs),
            ("burst.secs", self.burst.secs),
            ("recovery_secs", self.recovery_secs),
        ] {
            if secs == 0 || secs > MAX_HOLD_SECS {
                problems.push(format!(
                    "{label} must be between 1 and {MAX_HOLD_SECS}, got {secs}"
                ));
            }
        }
        if !(self.burst.multiplier.is_finite() && self.burst.multiplier >= 1.0) {
            problems.push(format!(
                "burst.multiplier must be at least 1, got {}",
                self.burst.multiplier
            ));
        }
        if self.sample_ms == 0 {
            problems.push("sample_ms must be non-zero".to_owned());
        }
        if !(self.knee.max_backlog_secs.is_finite() && self.knee.max_backlog_secs > 0.0) {
            problems.push("knee.max_backlog_secs must be positive".to_owned());
        }
        if !(0.0..=1.0).contains(&self.knee.min_achieved_share) {
            problems.push("knee.min_achieved_share must be between 0 and 1".to_owned());
        }
        if self.knee.watch.is_empty() {
            problems.push("knee.watch names no action, so no step could be judged".to_owned());
        }
        let seeded: BTreeSet<&str> = self.fixture.actions().iter().map(|a| a.name).collect();
        for action in &self.knee.watch {
            if !seeded.contains(action.as_str()) {
                problems.push(format!(
                    "knee.watch names `{action}`, which the fixture does not seed"
                ));
            }
        }
        for page in &self.pages {
            if !self.fixture.declares_overlay(page) {
                problems.push(format!(
                    "pages names `{page}`, which the fixture does not declare"
                ));
            }
        }
        self.stimulus_problems(&mut problems);
        problems
    }

    fn stimulus_problems(&self, problems: &mut Vec<String>) {
        let actions: BTreeSet<&str> = self.fixture.actions().iter().map(|a| a.name).collect();
        let mut kinds = BTreeSet::new();
        let mut flood_weight = 0u64;
        for (index, stimulus) in self.stimuli.iter().enumerate() {
            let at = format!("stimuli[{index}] ({})", stimulus.kind.label());
            if !kinds.insert(stimulus.kind) {
                problems.push(format!("{at} repeats a kind listed earlier"));
            }
            match (stimulus.weight, stimulus.per_minute) {
                (Some(0), None) | (None, Some(0)) => {
                    problems.push(format!("{at} is zero, so it would never be sent"));
                }
                (Some(weight), None) => flood_weight += u64::from(weight),
                (None, Some(_)) => {}
                _ => problems.push(format!("{at} needs exactly one of weight or per_minute")),
            }
            if stimulus.kind == StimulusKind::Command
                && stimulus
                    .text
                    .as_deref()
                    .is_none_or(|text| text.trim().is_empty())
            {
                problems.push(format!("{at} needs the command phrase in text"));
            }
            if stimulus.runs.is_empty() {
                problems.push(format!(
                    "{at} runs no action, so nothing it causes is measured"
                ));
            }
            for action in &stimulus.runs {
                if !actions.contains(action.as_str()) {
                    problems.push(format!(
                        "{at} runs `{action}`, which the fixture does not seed"
                    ));
                }
            }
        }
        if flood_weight == 0 {
            problems.push("no stimulus has a weight, so the ramp would send nothing".to_owned());
        }
    }

    /// The queue each fixture action runs on.
    pub fn queue_of(&self, action: &str) -> String {
        self.fixture
            .actions()
            .iter()
            .find(|candidate| candidate.name == action)
            .and_then(|candidate| candidate.queue)
            .unwrap_or(DEFAULT_QUEUE_NAME)
            .to_owned()
    }

    pub fn subscription_types(&self) -> Vec<&'static str> {
        let types: BTreeSet<&'static str> = self
            .stimuli
            .iter()
            .map(|stimulus| stimulus.kind.subscription_type())
            .collect();
        types.into_iter().collect()
    }
}
