use std::fmt;
use std::time::Duration;

use crate::fixture::TwitchAccount;

const DEFAULT_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Clone, PartialEq, Eq)]
pub struct FakeTwitchConfig {
    pub client_id: String,
    pub access_token: String,
    pub broadcaster_user_id: String,
    pub broadcaster_login: String,
    /// Idle time before the socket sends `session_keepalive`; forge drops a session silent for 15 s.
    pub keepalive_interval: Duration,
}

impl FakeTwitchConfig {
    /// Expects exactly the credentials the fixture seeder stored for `account`.
    pub fn for_account(account: &TwitchAccount) -> Self {
        Self {
            client_id: account.client_id.clone(),
            access_token: account.access_token.clone(),
            broadcaster_user_id: account.user_id.clone(),
            broadcaster_login: account.login.clone(),
            keepalive_interval: DEFAULT_KEEPALIVE_INTERVAL,
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
            .field("keepalive_interval", &self.keepalive_interval)
            .finish()
    }
}
