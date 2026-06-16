mod add_moderator;
mod ban_user;
mod delete_message;
mod remove_moderator;
mod send_message;
mod timeout_user;
mod unban_user;
mod update_category;
mod update_description;
mod update_privacy;
mod update_title;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};

pub use add_moderator::AddModeratorRunner;
pub use ban_user::BanUserRunner;
pub use delete_message::DeleteMessageRunner;
pub use remove_moderator::RemoveModeratorRunner;
pub use send_message::SendMessageRunner;
pub use timeout_user::TimeoutUserRunner;
pub use unban_user::UnbanUserRunner;
pub use update_category::UpdateCategoryRunner;
pub use update_description::UpdateDescriptionRunner;
pub use update_privacy::UpdatePrivacyRunner;
pub use update_title::UpdateTitleRunner;

use crate::moderation::YoutubeModeration;
use crate::send_chat::YoutubeSendChat;
use crate::stream_metadata::YoutubeStreamMetadata;

pub fn register_youtube_sub_actions(
    reg: &mut SubActionRegistry,
    sender: Arc<YoutubeSendChat>,
    moderation: Arc<YoutubeModeration>,
    metadata: Arc<YoutubeStreamMetadata>,
) -> Result<(), RegistryError> {
    reg.register(Box::new(SendMessageRunner::new(Arc::clone(&sender))))?;
    reg.register(Box::new(DeleteMessageRunner::new(sender)))?;
    reg.register(Box::new(BanUserRunner::new(Arc::clone(&moderation))))?;
    reg.register(Box::new(TimeoutUserRunner::new(Arc::clone(&moderation))))?;
    reg.register(Box::new(UnbanUserRunner::new(Arc::clone(&moderation))))?;
    reg.register(Box::new(AddModeratorRunner::new(Arc::clone(&moderation))))?;
    reg.register(Box::new(RemoveModeratorRunner::new(moderation)))?;
    reg.register(Box::new(UpdateTitleRunner::new(Arc::clone(&metadata))))?;
    reg.register(Box::new(UpdateDescriptionRunner::new(Arc::clone(
        &metadata,
    ))))?;
    reg.register(Box::new(UpdateCategoryRunner::new(Arc::clone(&metadata))))?;
    reg.register(Box::new(UpdatePrivacyRunner::new(metadata)))?;
    Ok(())
}
