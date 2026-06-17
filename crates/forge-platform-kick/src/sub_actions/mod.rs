mod ban_user;
mod delete_message;
mod send_message;
mod timeout_user;
mod unban_user;

use std::sync::Arc;

use forge_platform_core::PlatformError;
use forge_registry::{RegistryError, SubActionRegistry};
use futures::future::BoxFuture;

pub use ban_user::BanUserRunner;
pub use delete_message::DeleteMessageRunner;
pub use send_message::SendMessageRunner;
pub use timeout_user::TimeoutUserRunner;
pub use unban_user::UnbanUserRunner;

use crate::moderation::KickModeration;
use crate::send::KickSendChat;

pub fn register_kick_sub_actions(
    reg: &mut SubActionRegistry,
    client: Arc<KickSendChat>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
    broadcaster_user_id: u64,
    moderation: Arc<KickModeration>,
) -> Result<(), RegistryError> {
    reg.register(Box::new(SendMessageRunner::new(
        Arc::clone(&client),
        Arc::clone(&token_source),
        broadcaster_user_id,
    )))?;
    reg.register(Box::new(DeleteMessageRunner::new(
        Arc::clone(&client),
        Arc::clone(&token_source),
    )))?;
    reg.register(Box::new(BanUserRunner::new(
        Arc::clone(&moderation),
        Arc::clone(&token_source),
        broadcaster_user_id,
    )))?;
    reg.register(Box::new(TimeoutUserRunner::new(
        Arc::clone(&moderation),
        Arc::clone(&token_source),
        broadcaster_user_id,
    )))?;
    reg.register(Box::new(UnbanUserRunner::new(
        moderation,
        token_source,
        broadcaster_user_id,
    )))?;
    Ok(())
}
