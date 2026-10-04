use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::fixture::TwitchAccount;

const DEFAULT_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);
const DEFAULT_BROADCASTER_TYPE: &str = "affiliate";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FakeGoal {
    pub id: String,
    #[serde(rename = "type")]
    pub goal_type: String,
    #[serde(default)]
    pub description: String,
    pub current_amount: i64,
    pub target_amount: i64,
}

#[derive(Clone, PartialEq, Eq)]
pub struct FakeTwitchConfig {
    pub client_id: String,
    pub access_token: String,
    pub broadcaster_user_id: String,
    pub broadcaster_login: String,
    pub broadcaster_type: String,
    pub keepalive_interval: Duration,
    pub goals: Vec<FakeGoal>,
}

impl FakeTwitchConfig {
    pub fn for_account(account: &TwitchAccount) -> Self {
        Self {
            client_id: account.client_id.clone(),
            access_token: account.access_token.clone(),
            broadcaster_user_id: account.user_id.clone(),
            broadcaster_login: account.login.clone(),
            broadcaster_type: DEFAULT_BROADCASTER_TYPE.to_owned(),
            keepalive_interval: DEFAULT_KEEPALIVE_INTERVAL,
            goals: Vec::new(),
        }
    }
}

impl fmt::Debug for FakeTwitchConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeTwitchConfig")
            .field("client_id", &self.client_id)
            .field("access_token", &"<redacted>")
            .field("broadcaster_user_id", &self.broadcaster_user_id)
            .field("broadcaster_login", &self.broadcaster_login)
            .field("broadcaster_type", &self.broadcaster_type)
            .field("keepalive_interval", &self.keepalive_interval)
            .field("goals", &self.goals)
            .finish()
    }
}
