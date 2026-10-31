use forge_platform_twitch::vocabulary::topics;
use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::fixture::{DEFAULT_QUEUE_NAME, Fixture};
use crate::harness::scenario::{FakeTwitchSetup, without_position};
use crate::{EmulatorError, Problem};

pub const MAX_SENDERS: u32 = 1_000_000;
pub const MAX_RATE: u32 = 100_000;
pub const MAX_HOLD_SECS: u64 = 3_600;
const BYTE_ORDER_MARK: char = '\u{feff}';

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StressProfile {
    pub name: String,
    pub purpose: String,
    pub fixture: Fixture,
    #[serde(default)]
    pub fakes: FakeTwitchSetup,
    #[serde(default)]
    pub globals: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub pages: Vec<String>,
    pub senders: u32,
    pub stimuli: Vec<Stimulus>,
    pub ramp: Ramp,
    pub burst: Burst,
    pub recovery_secs: u64,
    #[serde(default = "default_sample_ms")]
    pub sample_ms: u64,
    pub knee: KneeRule,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stimulus {
    pub kind: StimulusKind,
    #[serde(default)]
    pub weight: Option<u32>,
    #[serde(default)]
    pub per_minute: Option<u32>,
    #[serde(default)]
    pub text: Option<String>,
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

    pub fn subscription_type(self) -> &'static str {
        match self {
            Self::Chat | Self::Command => topics::CHANNEL_CHAT_MESSAGE,
            Self::Follow => topics::CHANNEL_FOLLOW,
            Self::Subscribe => topics::CHANNEL_SUBSCRIBE,
            Self::Cheer => topics::CHANNEL_CHEER,
            Self::Reward => topics::CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_REDEMPTION_ADD,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ramp {
    pub rates: Vec<u32>,
    pub hold_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Burst {
    pub multiplier: f64,
    pub secs: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KneeRule {
    pub watch: Vec<String>,
    pub max_backlog_secs: f64,
    pub min_achieved_share: f64,
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
        serde_json::from_str(text).map_err(|e| EmulatorError::StressProfileSyntax {
            path: path.to_owned(),
            line: e.line(),
            column: e.column(),
            reason: without_position(&e),
        })?;
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
    pub fn problems(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        if let Err(e) = self.fixture.validate() {
            problems.push(Problem::new("fixture", e.to_string()));
        }
        if self.fixture.twitch.is_none() {
            problems.push(Problem::new(
                "fixture.twitch",
                "is required: every stimulus arrives over Twitch",
            ));
        }
        problems.extend(
            self.fakes
                .problems()
                .into_iter()
                .map(|problem| problem.under("fakes")),
        );
        problems.extend(self.load_problems());
        self.knee_problems(&mut problems);
        for page in &self.pages {
            if !self.fixture.declares_overlay(page) {
                problems.push(Problem::new(
                    "pages",
                    format!("names `{page}`, which the fixture does not declare"),
                ));
            }
        }
        self.stimulus_problems(&mut problems);
        problems
    }

    fn load_problems(&self) -> Vec<Problem> {
        let rates = self.ramp.rates.iter().enumerate();
        let mut problems: Vec<Problem> = Problem::outside(
            "senders",
            u64::from(self.senders),
            1,
            u64::from(MAX_SENDERS),
        )
        .into_iter()
        .chain(rates.filter_map(|(index, rate)| {
            let location = format!("ramp.rates[{index}]");
            Problem::outside(location, u64::from(*rate), 1, u64::from(MAX_RATE))
        }))
        .collect();
        if self.ramp.rates.is_empty() {
            problems.push(Problem::new("ramp.rates", "is empty"));
        }
        for (label, secs) in [
            ("ramp.hold_secs", self.ramp.hold_secs),
            ("burst.secs", self.burst.secs),
            ("recovery_secs", self.recovery_secs),
        ] {
            problems.extend(Problem::outside(label, secs, 1, MAX_HOLD_SECS));
        }
        if !(self.burst.multiplier.is_finite() && self.burst.multiplier >= 1.0) {
            problems.push(Problem::new(
                "burst.multiplier",
                format!("must be at least 1, got {}", self.burst.multiplier),
            ));
        }
        if self.sample_ms == 0 {
            problems.push(Problem::new("sample_ms", "must be non-zero"));
        }
        problems
    }

    fn knee_problems(&self, problems: &mut Vec<Problem>) {
        if !(self.knee.max_backlog_secs.is_finite() && self.knee.max_backlog_secs > 0.0) {
            problems.push(Problem::new("knee.max_backlog_secs", "must be positive"));
        }
        if !(0.0..=1.0).contains(&self.knee.min_achieved_share) {
            problems.push(Problem::new(
                "knee.min_achieved_share",
                "must be between 0 and 1",
            ));
        }
        if self.knee.watch.is_empty() {
            problems.push(Problem::new(
                "knee.watch",
                "names no action, so no step could be judged",
            ));
        }
        let seeded: BTreeSet<&str> = self.fixture.actions().iter().map(|a| a.name).collect();
        for action in &self.knee.watch {
            if !seeded.contains(action.as_str()) {
                problems.push(Problem::new(
                    "knee.watch",
                    format!("names `{action}`, which the fixture does not seed"),
                ));
            }
        }
    }

    fn stimulus_problems(&self, problems: &mut Vec<Problem>) {
        let actions: BTreeSet<&str> = self.fixture.actions().iter().map(|a| a.name).collect();
        let mut kinds = BTreeSet::new();
        let mut flood_weight = 0u64;
        for (index, stimulus) in self.stimuli.iter().enumerate() {
            let at = format!("stimuli[{index}] ({})", stimulus.kind.label());
            if !kinds.insert(stimulus.kind) {
                problems.push(Problem::new(&at, "repeats a kind listed earlier"));
            }
            match (stimulus.weight, stimulus.per_minute) {
                (Some(0), None) | (None, Some(0)) => {
                    problems.push(Problem::new(&at, "is zero, so it would never be sent"));
                }
                (Some(weight), None) => flood_weight += u64::from(weight),
                (None, Some(_)) => {}
                _ => problems.push(Problem::new(
                    &at,
                    "needs exactly one of weight or per_minute",
                )),
            }
            if stimulus.kind == StimulusKind::Command
                && stimulus
                    .text
                    .as_deref()
                    .is_none_or(|text| text.trim().is_empty())
            {
                problems.push(Problem::new(&at, "needs the command phrase in text"));
            }
            if stimulus.runs.is_empty() {
                problems.push(Problem::new(
                    &at,
                    "runs no action, so nothing it causes is measured",
                ));
            }
            for action in &stimulus.runs {
                if !actions.contains(action.as_str()) {
                    problems.push(Problem::new(
                        &at,
                        format!("runs `{action}`, which the fixture does not seed"),
                    ));
                }
            }
        }
        if flood_weight == 0 {
            problems.push(Problem::new(
                "stimuli",
                "no stimulus has a weight, so the ramp would send nothing",
            ));
        }
    }

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
