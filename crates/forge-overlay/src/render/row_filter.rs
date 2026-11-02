use std::collections::BTreeSet;

use forge_types::Variant;

use crate::catalog::descriptor::OverlayConfig;
use crate::config::{COMMAND_PREFIX, HIDDEN_ACCOUNTS, HIDE_COMMANDS};

const MENTION_SIGIL: char = '@';
const ACCOUNT_SEPARATOR: char = ',';

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatRowFilter {
    command_prefix: Option<String>,
    hidden_logins: BTreeSet<String>,
}

impl ChatRowFilter {
    pub fn from_config(config: &OverlayConfig) -> Self {
        let text_at = |key: &str| config.get(key).and_then(Variant::as_str).unwrap_or("");
        let command_prefix = Some(text_at(COMMAND_PREFIX).trim())
            .filter(|prefix| !prefix.is_empty())
            .filter(|_| matches!(config.get(HIDE_COMMANDS), Some(Variant::Bool(true))))
            .map(str::to_owned);
        Self {
            command_prefix,
            hidden_logins: hidden_logins(text_at(HIDDEN_ACCOUNTS)),
        }
    }

    pub fn hides(&self, login: &str, message: &str) -> bool {
        self.is_command(message) || self.hidden_logins.contains(&normalized_login(login))
    }

    fn is_command(&self, message: &str) -> bool {
        self.command_prefix
            .as_deref()
            .is_some_and(|prefix| message.trim_start().starts_with(prefix))
    }
}

fn hidden_logins(listed: &str) -> BTreeSet<String> {
    listed
        .split(|c: char| c == ACCOUNT_SEPARATOR || c.is_whitespace())
        .map(normalized_login)
        .filter(|login| !login.is_empty())
        .collect()
}

fn normalized_login(login: &str) -> String {
    login
        .trim()
        .trim_start_matches(MENTION_SIGIL)
        .to_lowercase()
}
