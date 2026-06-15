mod add_moderator;
mod ban_user;
mod remove_moderator;
mod send_message;
mod timeout_user;
mod unban_user;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};

pub use add_moderator::AddModeratorRunner;
pub use ban_user::BanUserRunner;
pub use remove_moderator::RemoveModeratorRunner;
pub use send_message::SendMessageRunner;
pub use timeout_user::TimeoutUserRunner;
pub use unban_user::UnbanUserRunner;

use crate::moderation::YoutubeModeration;
use crate::send_chat::YoutubeSendChat;

pub fn register_youtube_sub_actions(
    reg: &mut SubActionRegistry,
    sender: Arc<YoutubeSendChat>,
    moderation: Arc<YoutubeModeration>,
) -> Result<(), RegistryError> {
    reg.register(Box::new(SendMessageRunner::new(sender)))?;
    reg.register(Box::new(BanUserRunner::new(Arc::clone(&moderation))))?;
    reg.register(Box::new(TimeoutUserRunner::new(Arc::clone(&moderation))))?;
    reg.register(Box::new(UnbanUserRunner::new(Arc::clone(&moderation))))?;
    reg.register(Box::new(AddModeratorRunner::new(Arc::clone(&moderation))))?;
    reg.register(Box::new(RemoveModeratorRunner::new(moderation)))?;
    Ok(())
}
