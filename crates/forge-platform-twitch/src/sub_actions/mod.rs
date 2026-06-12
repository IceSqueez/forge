mod identity;
mod send_announcement;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};
use forge_storage::CredentialsRepo;

pub use identity::SelfIdentity;
pub use send_announcement::SendAnnouncementRunner;

use crate::helix::HelixTransport;

pub fn register_twitch_sub_actions(
    reg: &mut SubActionRegistry,
    transport: Arc<dyn HelixTransport>,
    creds: Arc<dyn CredentialsRepo>,
) -> Result<(), RegistryError> {
    let identity = Arc::new(SelfIdentity::new(creds));
    reg.register(Box::new(SendAnnouncementRunner::new(transport, identity)))?;
    Ok(())
}
