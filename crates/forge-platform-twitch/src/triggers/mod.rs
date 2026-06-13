mod channel_follow;
mod channel_points_redemption;
mod channel_raid_received;
mod chat_arg_stack;
mod chat_cheer_message;
mod chat_command;
mod chat_message;
mod shared_chat_message;
mod stream_offline;
mod stream_online;
mod support_cheer;
mod support_gift_sub;
mod support_resubscriber;
mod support_subscriber;

use forge_registry::{RegistryError, TriggerRegistry};

use channel_follow::ChannelFollowDescriptor;
use channel_points_redemption::ChannelPointsRedemptionDescriptor;
use channel_raid_received::ChannelRaidReceivedDescriptor;
use chat_cheer_message::ChatCheerMessageDescriptor;
use chat_command::ChatCommandDescriptor;
use chat_message::ChatMessageDescriptor;
use shared_chat_message::SharedChatMessageDescriptor;
use stream_offline::StreamOfflineDescriptor;
use stream_online::StreamOnlineDescriptor;
use support_cheer::SupportCheerDescriptor;
use support_gift_sub::SupportGiftSubDescriptor;
use support_resubscriber::SupportResubscriberDescriptor;
use support_subscriber::SupportSubscriberDescriptor;

pub fn register_twitch_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    reg.register(Box::new(ChatCommandDescriptor))?;
    reg.register(Box::new(ChatMessageDescriptor))?;
    reg.register(Box::new(ChatCheerMessageDescriptor))?;
    reg.register(Box::new(SharedChatMessageDescriptor))?;
    reg.register(Box::new(SupportSubscriberDescriptor))?;
    reg.register(Box::new(SupportResubscriberDescriptor))?;
    reg.register(Box::new(SupportGiftSubDescriptor))?;
    reg.register(Box::new(SupportCheerDescriptor))?;
    reg.register(Box::new(ChannelRaidReceivedDescriptor))?;
    reg.register(Box::new(ChannelFollowDescriptor))?;
    reg.register(Box::new(ChannelPointsRedemptionDescriptor))?;
    reg.register(Box::new(StreamOnlineDescriptor))?;
    reg.register(Box::new(StreamOfflineDescriptor))?;
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
            "twitch.chat.command",
            "twitch.chat.message",
            "twitch.chat.cheer_message",
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
        ];

        for id in ids {
            assert!(reg.get(id).is_some(), "missing kind id: {id}");
        }
    }
}
