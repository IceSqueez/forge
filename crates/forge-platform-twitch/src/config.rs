use std::time::Duration;

use forge_platform_core::PlatformEndpoints;

pub const HELIX_BUDGET_CAPACITY: u32 = 800;
pub const HELIX_BUDGET_WINDOW: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct ChatSessionConfig {
    pub client_id: String,
    pub broadcaster_id: String,
    pub user_id: String,
    pub endpoints: PlatformEndpoints,
}
