use std::fmt;
use std::path::PathBuf;

use forge_types::{ActionId, TriggerInstanceId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeedReport {
    pub data_dir: PathBuf,
    pub server: SeededServer,
    pub twitch: Option<SeededTwitch>,
    pub chat_commands: Vec<SeededCommand>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeededServer {
    pub port: u16,
    pub bearer_token: String,
}

impl fmt::Debug for SeededServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SeededServer")
            .field("port", &self.port)
            .field("bearer_token", &"<redacted>")
            .finish()
    }
}

/// forge reads the client id from its environment, never from storage: the launcher must pass it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeededTwitch {
    pub client_id: String,
    pub user_id: String,
    pub login: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeededCommand {
    pub phrase: String,
    pub action_id: ActionId,
    pub trigger_instance_id: TriggerInstanceId,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_never_carries_the_bearer_token() {
        let server = SeededServer {
            port: 40000,
            bearer_token: "secret-bearer-value".to_owned(),
        };
        let rendered = format!("{server:?}");
        assert!(!rendered.contains("secret-bearer-value"), "{rendered}");
    }
}
