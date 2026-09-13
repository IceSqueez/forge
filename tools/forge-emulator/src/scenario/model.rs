use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::step::Step;
use crate::fixture::{Fixture, TwitchAccount};
use crate::twitch::FakeTwitchConfig;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    pub purpose: String,
    pub fixture: Fixture,
    #[serde(default)]
    pub fakes: Fakes,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fakes {
    #[serde(default)]
    pub twitch: Option<FakeTwitchSetup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FakeTwitchSetup {
    #[serde(default = "default_keepalive_ms")]
    pub keepalive_interval_ms: u64,
}

impl Default for FakeTwitchSetup {
    fn default() -> Self {
        Self {
            keepalive_interval_ms: default_keepalive_ms(),
        }
    }
}

fn default_keepalive_ms() -> u64 {
    10_000
}

impl FakeTwitchSetup {
    pub fn config_for(&self, account: &TwitchAccount) -> FakeTwitchConfig {
        FakeTwitchConfig {
            keepalive_interval: Duration::from_millis(self.keepalive_interval_ms),
            ..FakeTwitchConfig::for_account(account)
        }
    }
}
