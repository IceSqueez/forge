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
mod hype_train_ended;
mod hype_train_progress;
mod hype_train_started;
mod shared_chat_message;
mod stream_offline;
mod stream_online;
mod support_cheer;
mod support_gift_sub;
mod support_resubscriber;
mod support_subscriber;

use forge_registry::{RegistryError, TriggerRegistry};

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
use hype_train_ended::HypeTrainEndedDescriptor;
use hype_train_progress::HypeTrainProgressDescriptor;
use hype_train_started::HypeTrainStartedDescriptor;
use shared_chat_message::SharedChatMessageDescriptor;
use stream_offline::StreamOfflineDescriptor;
use stream_online::StreamOnlineDescriptor;
use support_cheer::SupportCheerDescriptor;
use support_gift_sub::SupportGiftSubDescriptor;
use support_resubscriber::SupportResubscriberDescriptor;
use support_subscriber::SupportSubscriberDescriptor;

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
        ];

        for id in ids {
            assert!(reg.get(id).is_some(), "missing kind id: {id}");
        }
    }
}
