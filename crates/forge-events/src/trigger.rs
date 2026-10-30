use serde::{Deserialize, Serialize};

use crate::TypedEvent;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandMatched {
    pub command: String,
    pub kind_id: String,
}

impl TypedEvent for CommandMatched {
    const KIND: &'static str = "command.matched";
}
