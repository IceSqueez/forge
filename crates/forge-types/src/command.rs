use crate::ids::{ActionId, CommandId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandPermission {
    Everyone,
    Subscriber,
    Vip,
    Moderator,
    Broadcaster,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub id: CommandId,
    pub action_id: ActionId,
    /// Includes the `!` prefix, e.g. `"!quote"`. Stored and matched lowercase.
    pub name: String,
    pub cooldown_secs: u64,
    pub permission: CommandPermission,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn quote_command() -> Command {
        Command {
            id: CommandId::new(),
            action_id: ActionId::new(),
            name: "!quote".to_string(),
            cooldown_secs: 30,
            permission: CommandPermission::Everyone,
        }
    }

    #[test]
    fn command_serde_roundtrip() {
        let c = quote_command();
        let json = serde_json::to_string(&c).unwrap();
        let back: Command = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn all_permissions_serde_roundtrip() {
        let perms = [
            CommandPermission::Everyone,
            CommandPermission::Subscriber,
            CommandPermission::Vip,
            CommandPermission::Moderator,
            CommandPermission::Broadcaster,
        ];
        for perm in perms {
            let json = serde_json::to_string(&perm).unwrap();
            let back: CommandPermission = serde_json::from_str(&json).unwrap();
            assert_eq!(perm, back);
        }
    }

    #[test]
    fn command_zero_cooldown_serde_roundtrip() {
        let mut c = quote_command();
        c.cooldown_secs = 0;
        let json = serde_json::to_string(&c).unwrap();
        let back: Command = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }
}
