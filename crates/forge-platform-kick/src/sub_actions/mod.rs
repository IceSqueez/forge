mod ban_user;
mod delete_message;
mod send_message;
mod timeout_user;
mod unban_user;
mod update_info;

use std::sync::Arc;

use forge_platform_core::PlatformError;
use forge_registry::{RegistryError, SubActionRegistry};
use futures::future::BoxFuture;

pub use ban_user::BanUserRunner;
pub use delete_message::DeleteMessageRunner;
pub use send_message::SendMessageRunner;
pub use timeout_user::TimeoutUserRunner;
pub use unban_user::UnbanUserRunner;
pub use update_info::UpdateInfoRunner;

use crate::channel::KickChannel;
use crate::moderation::KickModeration;
use crate::send::KickSendChat;

pub struct KickSubActionDeps {
    pub client: Arc<KickSendChat>,
    pub token_source:
        Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
    pub broadcaster_user_id: u64,
    pub moderation: Arc<KickModeration>,
    pub channel: Arc<KickChannel>,
}

pub fn register_kick_sub_actions(
    reg: &mut SubActionRegistry,
    deps: KickSubActionDeps,
) -> Result<(), RegistryError> {
    let KickSubActionDeps {
        client,
        token_source,
        broadcaster_user_id,
        moderation,
        channel,
    } = deps;
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
        Arc::clone(&moderation),
        Arc::clone(&token_source),
        broadcaster_user_id,
    )))?;
    reg.register(Box::new(UpdateInfoRunner::new(channel, token_source)))?;
    Ok(())
}
