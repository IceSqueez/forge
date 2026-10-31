mod add_moderator;
mod ban_user;
mod create_poll;
mod delete_message;
mod display_name;
mod fields;
mod insert_ad_break;
mod lookup_stream_stats;
mod lookup_viewer;
mod remove_moderator;
mod send_message;
mod set_thumbnail;
mod step_failure;
mod timeout_user;
mod unban_user;
mod update_category;
mod update_description;
mod update_privacy;
mod update_title;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};

pub(crate) use add_moderator::AddModeratorRunner;
pub(crate) use ban_user::BanUserRunner;
pub(crate) use create_poll::CreatePollRunner;
pub(crate) use delete_message::DeleteMessageRunner;
pub(crate) use insert_ad_break::InsertAdBreakRunner;
pub(crate) use lookup_stream_stats::LookupStreamStatsRunner;
pub(crate) use lookup_viewer::LookupViewerRunner;
pub(crate) use remove_moderator::RemoveModeratorRunner;
pub(crate) use send_message::SendMessageRunner;
pub(crate) use set_thumbnail::SetThumbnailRunner;
pub(crate) use timeout_user::TimeoutUserRunner;
pub(crate) use unban_user::UnbanUserRunner;
pub(crate) use update_category::UpdateCategoryRunner;
pub(crate) use update_description::UpdateDescriptionRunner;
pub(crate) use update_privacy::PrivacyStatus;
pub(crate) use update_privacy::UpdatePrivacyRunner;
pub(crate) use update_title::UpdateTitleRunner;

use crate::ad_break::YoutubeAdBreak;
use crate::channel_lookup::YoutubeChannelLookup;
use crate::moderation::YoutubeModeration;
use crate::send_chat::YoutubeSendChat;
use crate::stream_metadata::YoutubeStreamMetadata;
use crate::stream_stats::YoutubeStreamStats;
use crate::thumbnail::YoutubeThumbnail;

#[expect(
    clippy::too_many_arguments,
    reason = "each argument is a shared service a runner needs"
)]
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
    let mut reg = reg.owned_by(crate::YOUTUBE_INTEGRATION.id);
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

#[cfg(test)]
pub(crate) fn rejected_config(
    kind: &str,
    problem: forge_registry::ConfigProblem,
) -> forge_types::SubActionOutcome {
    let message = problem.message(|keys| forge_types::MessageArg::StepFields {
        kind: kind.to_owned(),
        keys: keys.to_vec(),
    });
    forge_types::SubActionOutcome::Failed(message.into())
}
