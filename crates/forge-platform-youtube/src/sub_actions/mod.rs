mod add_moderator;
mod ban_user;
mod create_poll;
mod delete_message;
mod insert_ad_break;
mod lookup_stream_stats;
mod lookup_viewer;
mod remove_moderator;
mod send_message;
mod set_thumbnail;
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
pub use create_poll::CreatePollRunner;
pub use delete_message::DeleteMessageRunner;
pub use insert_ad_break::InsertAdBreakRunner;
pub use lookup_stream_stats::LookupStreamStatsRunner;
pub use lookup_viewer::LookupViewerRunner;
pub use remove_moderator::RemoveModeratorRunner;
pub use send_message::SendMessageRunner;
pub use set_thumbnail::SetThumbnailRunner;
pub use timeout_user::TimeoutUserRunner;
pub use unban_user::UnbanUserRunner;
pub use update_category::UpdateCategoryRunner;
pub use update_description::UpdateDescriptionRunner;
pub use update_privacy::UpdatePrivacyRunner;
pub use update_title::UpdateTitleRunner;

use crate::ad_break::YoutubeAdBreak;
use crate::channel_lookup::YoutubeChannelLookup;
use crate::moderation::YoutubeModeration;
use crate::send_chat::YoutubeSendChat;
use crate::stream_metadata::YoutubeStreamMetadata;
use crate::stream_stats::YoutubeStreamStats;
use crate::thumbnail::YoutubeThumbnail;

#[allow(clippy::too_many_arguments)]
pub fn register_youtube_sub_actions(
    reg: &mut SubActionRegistry,
    sender: Arc<YoutubeSendChat>,
    moderation: Arc<YoutubeModeration>,
    metadata: Arc<YoutubeStreamMetadata>,
    stream_stats: Arc<YoutubeStreamStats>,
    ad_break: Arc<YoutubeAdBreak>,
    thumbnail: Arc<YoutubeThumbnail>,
    channel_lookup: Arc<YoutubeChannelLookup>,
) -> Result<(), RegistryError> {
    reg.register(Box::new(SendMessageRunner::new(Arc::clone(&sender))))?;
    reg.register(Box::new(CreatePollRunner::new(Arc::clone(&sender))))?;
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
    reg.register(Box::new(LookupStreamStatsRunner::new(stream_stats)))?;
    reg.register(Box::new(InsertAdBreakRunner::new(ad_break)))?;
    reg.register(Box::new(SetThumbnailRunner::new(thumbnail)))?;
    reg.register(Box::new(LookupViewerRunner::new(channel_lookup)))?;
    Ok(())
}
