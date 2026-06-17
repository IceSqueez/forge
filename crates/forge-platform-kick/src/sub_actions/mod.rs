mod ban_user;
mod create_reward;
mod delete_message;
mod delete_reward;
mod send_message;
mod timeout_user;
mod unban_user;
mod update_info;
mod update_reward;

use std::sync::Arc;

use forge_platform_core::PlatformError;
use forge_registry::{RegistryError, SubActionRegistry};
use futures::future::BoxFuture;

pub use ban_user::BanUserRunner;
pub use create_reward::CreateRewardRunner;
pub use delete_message::DeleteMessageRunner;
pub use delete_reward::DeleteRewardRunner;
pub use send_message::SendMessageRunner;
pub use timeout_user::TimeoutUserRunner;
pub use unban_user::UnbanUserRunner;
pub use update_info::UpdateInfoRunner;
pub use update_reward::UpdateRewardRunner;

use crate::channel::KickChannel;
use crate::moderation::KickModeration;
use crate::rewards::KickRewards;
use crate::send::KickSendChat;

pub struct KickSubActionDeps {
    pub client: Arc<KickSendChat>,
    pub token_source:
        Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
    pub broadcaster_user_id: u64,
    pub moderation: Arc<KickModeration>,
    pub channel: Arc<KickChannel>,
    pub rewards: Arc<KickRewards>,
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
        rewards,
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
    reg.register(Box::new(UpdateInfoRunner::new(
        Arc::clone(&channel),
        Arc::clone(&token_source),
    )))?;
    reg.register(Box::new(CreateRewardRunner::new(
        Arc::clone(&rewards),
        Arc::clone(&token_source),
    )))?;
    reg.register(Box::new(UpdateRewardRunner::new(
        Arc::clone(&rewards),
        Arc::clone(&token_source),
    )))?;
    reg.register(Box::new(DeleteRewardRunner::new(rewards, token_source)))?;
    Ok(())
}
