use forge_events::TypedEvent;
use serde::{Deserialize, Serialize};

use super::event_kinds;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VTubeConnectionChanged {
    pub is_connected: bool,
    pub endpoint: String,
    pub reason: Option<String>,
    pub detail: Option<String>,
}

impl TypedEvent for VTubeConnectionChanged {
    const KIND: &'static str = event_kinds::CONNECTION_CHANGED;
}
