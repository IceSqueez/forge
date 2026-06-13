mod automod_message_held;
mod automod_message_updated;
mod automod_settings_updated;
mod automod_terms_updated;
mod channel_ban;
mod channel_follow;
mod channel_points_redemption;
mod channel_raid_received;
mod channel_timeout;
mod channel_unban;
mod charity_donation;
mod charity_progress;
mod charity_started;
mod charity_stopped;
mod chat_arg_stack;
mod chat_cheer_message;
mod chat_cleared;
mod chat_command;
mod chat_message;
mod chat_message_deleted;
mod chat_settings_updated;
mod goal_ended;
mod goal_progress;
mod goal_started;
mod guest_star_session_began;
mod guest_star_session_ended;
mod guest_star_settings_updated;
mod hype_train_ended;
mod hype_train_progress;
mod hype_train_started;
mod moderator_added;
mod moderator_removed;
mod poll_ended;
mod poll_progress;
mod poll_started;
mod prediction_ended;
mod prediction_locked;
mod prediction_progress;
mod prediction_started;
mod redemption_updated;
mod reward_added;
mod reward_removed;
mod reward_updated;
mod shared_chat_message;
mod shared_chat_session_began;
mod shared_chat_session_ended;
mod shared_chat_session_updated;
mod shield_mode_ended;
mod shield_mode_started;
mod shoutout_received;
mod shoutout_sent;
mod stream_offline;
mod stream_online;
mod support_cheer;
mod support_gift_sub;
mod support_resubscriber;
mod support_subscriber;
mod suspicious_user_message;
mod warning_acknowledged;

use forge_registry::{RegistryError, TriggerRegistry};

use automod_message_held::AutomodMessageHeldDescriptor;
use automod_message_updated::AutomodMessageUpdatedDescriptor;
use automod_settings_updated::AutomodSettingsUpdatedDescriptor;
use automod_terms_updated::AutomodTermsUpdatedDescriptor;
use channel_ban::ChannelBanDescriptor;
use channel_follow::ChannelFollowDescriptor;
use channel_points_redemption::ChannelPointsRedemptionDescriptor;
use channel_raid_received::ChannelRaidReceivedDescriptor;
use channel_timeout::ChannelTimeoutDescriptor;
use channel_unban::ChannelUnbanDescriptor;
use charity_donation::CharityDonationDescriptor;
use charity_progress::CharityProgressDescriptor;
use charity_started::CharityStartedDescriptor;
use charity_stopped::CharityStoppedDescriptor;
use chat_cheer_message::ChatCheerMessageDescriptor;
use chat_cleared::ChatClearedDescriptor;
use chat_command::ChatCommandDescriptor;
use chat_message::ChatMessageDescriptor;
use chat_message_deleted::ChatMessageDeletedDescriptor;
use chat_settings_updated::ChatSettingsUpdatedDescriptor;
use goal_ended::GoalEndedDescriptor;
use goal_progress::GoalProgressDescriptor;
use goal_started::GoalStartedDescriptor;
use guest_star_session_began::GuestStarSessionBeganDescriptor;
use guest_star_session_ended::GuestStarSessionEndedDescriptor;
use guest_star_settings_updated::GuestStarSettingsUpdatedDescriptor;
use hype_train_ended::HypeTrainEndedDescriptor;
use hype_train_progress::HypeTrainProgressDescriptor;
use hype_train_started::HypeTrainStartedDescriptor;
use moderator_added::ModeratorAddedDescriptor;
use moderator_removed::ModeratorRemovedDescriptor;
use poll_ended::PollEndedDescriptor;
use poll_progress::PollProgressDescriptor;
use poll_started::PollStartedDescriptor;
use prediction_ended::PredictionEndedDescriptor;
use prediction_locked::PredictionLockedDescriptor;
use prediction_progress::PredictionProgressDescriptor;
use prediction_started::PredictionStartedDescriptor;
use redemption_updated::RedemptionUpdatedDescriptor;
use reward_added::RewardAddedDescriptor;
use reward_removed::RewardRemovedDescriptor;
use reward_updated::RewardUpdatedDescriptor;
use shared_chat_message::SharedChatMessageDescriptor;
use shared_chat_session_began::SharedChatSessionBeganDescriptor;
use shared_chat_session_ended::SharedChatSessionEndedDescriptor;
use shared_chat_session_updated::SharedChatSessionUpdatedDescriptor;
use shield_mode_ended::ShieldModeEndedDescriptor;
use shield_mode_started::ShieldModeStartedDescriptor;
use shoutout_received::ShoutoutReceivedDescriptor;
use shoutout_sent::ShoutoutSentDescriptor;
use stream_offline::StreamOfflineDescriptor;
use stream_online::StreamOnlineDescriptor;
use support_cheer::SupportCheerDescriptor;
use support_gift_sub::SupportGiftSubDescriptor;
use support_resubscriber::SupportResubscriberDescriptor;
use support_subscriber::SupportSubscriberDescriptor;
use suspicious_user_message::SuspiciousUserMessageDescriptor;
use warning_acknowledged::WarningAcknowledgedDescriptor;

pub fn register_twitch_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    reg.register(Box::new(ChannelBanDescriptor))?;
    reg.register(Box::new(ChannelTimeoutDescriptor))?;
    reg.register(Box::new(ChannelUnbanDescriptor))?;
    reg.register(Box::new(ChatCommandDescriptor))?;
    reg.register(Box::new(ChatMessageDescriptor))?;
    reg.register(Box::new(ChatCheerMessageDescriptor))?;
    reg.register(Box::new(SharedChatMessageDescriptor))?;
    reg.register(Box::new(ChatMessageDeletedDescriptor))?;
    reg.register(Box::new(ChatClearedDescriptor))?;
    reg.register(Box::new(SupportSubscriberDescriptor))?;
    reg.register(Box::new(SupportResubscriberDescriptor))?;
    reg.register(Box::new(SupportGiftSubDescriptor))?;
    reg.register(Box::new(SupportCheerDescriptor))?;
    reg.register(Box::new(ChannelRaidReceivedDescriptor))?;
    reg.register(Box::new(ChannelFollowDescriptor))?;
    reg.register(Box::new(ChannelPointsRedemptionDescriptor))?;
    reg.register(Box::new(StreamOnlineDescriptor))?;
    reg.register(Box::new(StreamOfflineDescriptor))?;
    reg.register(Box::new(HypeTrainStartedDescriptor))?;
    reg.register(Box::new(HypeTrainProgressDescriptor))?;
    reg.register(Box::new(HypeTrainEndedDescriptor))?;
    reg.register(Box::new(CharityDonationDescriptor))?;
    reg.register(Box::new(CharityStartedDescriptor))?;
    reg.register(Box::new(CharityProgressDescriptor))?;
    reg.register(Box::new(CharityStoppedDescriptor))?;
    reg.register(Box::new(ModeratorAddedDescriptor))?;
    reg.register(Box::new(ModeratorRemovedDescriptor))?;
    reg.register(Box::new(ShieldModeStartedDescriptor))?;
    reg.register(Box::new(ShieldModeEndedDescriptor))?;
    reg.register(Box::new(ShoutoutSentDescriptor))?;
    reg.register(Box::new(ShoutoutReceivedDescriptor))?;
    reg.register(Box::new(SuspiciousUserMessageDescriptor))?;
    reg.register(Box::new(WarningAcknowledgedDescriptor))?;
    reg.register(Box::new(PollStartedDescriptor))?;
    reg.register(Box::new(PollProgressDescriptor))?;
    reg.register(Box::new(PollEndedDescriptor))?;
    reg.register(Box::new(PredictionStartedDescriptor))?;
    reg.register(Box::new(PredictionProgressDescriptor))?;
    reg.register(Box::new(PredictionLockedDescriptor))?;
    reg.register(Box::new(PredictionEndedDescriptor))?;
    reg.register(Box::new(GoalStartedDescriptor))?;
    reg.register(Box::new(GoalProgressDescriptor))?;
    reg.register(Box::new(GoalEndedDescriptor))?;
    reg.register(Box::new(RewardAddedDescriptor))?;
    reg.register(Box::new(RewardUpdatedDescriptor))?;
    reg.register(Box::new(RewardRemovedDescriptor))?;
    reg.register(Box::new(RedemptionUpdatedDescriptor))?;
    reg.register(Box::new(AutomodMessageHeldDescriptor))?;
    reg.register(Box::new(AutomodSettingsUpdatedDescriptor))?;
    reg.register(Box::new(AutomodTermsUpdatedDescriptor))?;
    reg.register(Box::new(AutomodMessageUpdatedDescriptor))?;
    reg.register(Box::new(ChatSettingsUpdatedDescriptor))?;
    reg.register(Box::new(GuestStarSessionBeganDescriptor))?;
    reg.register(Box::new(GuestStarSessionEndedDescriptor))?;
    reg.register(Box::new(GuestStarSettingsUpdatedDescriptor))?;
    reg.register(Box::new(SharedChatSessionBeganDescriptor))?;
    reg.register(Box::new(SharedChatSessionUpdatedDescriptor))?;
    reg.register(Box::new(SharedChatSessionEndedDescriptor))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_registration_returns_error() {
        let mut reg = TriggerRegistry::new();
        register_twitch_triggers(&mut reg).unwrap();
        let result = register_twitch_triggers(&mut reg);
        assert!(result.is_err());
    }

    #[test]
    fn all_kind_ids_are_reachable() {
        let mut reg = TriggerRegistry::new();
        register_twitch_triggers(&mut reg).unwrap();

        let ids = [
            "twitch.channel.ban",
            "twitch.channel.timeout",
            "twitch.channel.unban",
            "twitch.chat.command",
            "twitch.chat.message",
            "twitch.chat.cheer_message",
            "twitch.chat.message_deleted",
            "twitch.chat.cleared",
            "twitch.shared_chat.message_received",
            "twitch.support.subscriber",
            "twitch.support.resubscriber",
            "twitch.support.gift_sub",
            "twitch.support.cheer",
            "twitch.channel.raid_received",
            "twitch.channel.follow",
            "twitch.channel_points.redemption",
            "twitch.stream.online",
            "twitch.stream.offline",
            "twitch.support.hype_train_started",
            "twitch.support.hype_train_progress",
            "twitch.support.hype_train_ended",
            "twitch.support.charity_donation",
            "twitch.support.charity_started",
            "twitch.support.charity_progress",
            "twitch.support.charity_stopped",
            "twitch.channel.moderator_added",
            "twitch.channel.moderator_removed",
            "twitch.channel.shield_mode_started",
            "twitch.channel.shield_mode_ended",
            "twitch.channel.shoutout_sent",
            "twitch.channel.shoutout_received",
            "twitch.channel.suspicious_user_message",
            "twitch.channel.warning_acknowledged",
            "twitch.poll.started",
            "twitch.poll.progress",
            "twitch.poll.ended",
            "twitch.prediction.started",
            "twitch.prediction.progress",
            "twitch.prediction.locked",
            "twitch.prediction.ended",
            "twitch.goal.started",
            "twitch.goal.progress",
            "twitch.goal.ended",
            "twitch.channel_points.reward_added",
            "twitch.channel_points.reward_updated",
            "twitch.channel_points.reward_removed",
            "twitch.channel_points.redemption_updated",
            "twitch.automod.message_held",
            "twitch.automod.settings_updated",
            "twitch.automod.terms_updated",
            "twitch.automod.message_updated",
            "twitch.channel.chat_settings_updated",
            "twitch.guest_star.session_began",
            "twitch.guest_star.session_ended",
            "twitch.guest_star.settings_updated",
        ];

        for id in ids {
            assert!(reg.get(id).is_some(), "missing kind id: {id}");
        }
    }
}
