use forge_registry::{RegistryError, TriggerRegistry};

use crate::triggers::channel_broadcast_ended::ChannelBroadcastEndedDescriptor;
use crate::triggers::channel_broadcast_started::ChannelBroadcastStartedDescriptor;
use crate::triggers::chat_command::ChatCommandDescriptor;
use crate::triggers::chat_message::ChatMessageDescriptor;
use crate::triggers::moderation_ban::ModerationBanDescriptor;
use crate::triggers::moderation_timeout::ModerationTimeoutDescriptor;
use crate::triggers::support_member_milestone::SupportMemberMilestoneDescriptor;
use crate::triggers::support_new_member::SupportNewMemberDescriptor;
use crate::triggers::support_super_chat::SupportSuperChatDescriptor;
use crate::triggers::support_super_sticker::SupportSuperStickerDescriptor;

pub fn register_youtube_triggers(registry: &mut TriggerRegistry) -> Result<(), RegistryError> {
    registry.register(Box::new(ChatMessageDescriptor))?;
    registry.register(Box::new(ChatCommandDescriptor))?;
    registry.register(Box::new(SupportSuperChatDescriptor))?;
    registry.register(Box::new(SupportSuperStickerDescriptor))?;
    registry.register(Box::new(SupportNewMemberDescriptor))?;
    registry.register(Box::new(SupportMemberMilestoneDescriptor))?;
    registry.register(Box::new(ModerationTimeoutDescriptor))?;
    registry.register(Box::new(ModerationBanDescriptor))?;
    registry.register(Box::new(ChannelBroadcastStartedDescriptor))?;
    registry.register(Box::new(ChannelBroadcastEndedDescriptor))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn register_adds_all_ten_descriptors() {
        let mut reg = TriggerRegistry::new();
        register_youtube_triggers(&mut reg).unwrap();
        assert_eq!(reg.all().count(), 10);
    }

    #[test]
    fn duplicate_registration_returns_error() {
        let mut reg = TriggerRegistry::new();
        register_youtube_triggers(&mut reg).unwrap();
        let result = register_youtube_triggers(&mut reg);
        assert!(result.is_err());
    }

    #[test]
    fn all_kind_ids_are_reachable() {
        let mut reg = TriggerRegistry::new();
        register_youtube_triggers(&mut reg).unwrap();

        let ids = [
            "youtube.chat.message",
            "youtube.chat.command",
            "youtube.support.super_chat",
            "youtube.support.super_sticker",
            "youtube.support.new_member",
            "youtube.support.member_milestone",
            "youtube.moderation.timeout",
            "youtube.moderation.ban",
            "youtube.channel.live_broadcast_started",
            "youtube.channel.live_broadcast_ended",
        ];

        for id in ids {
            assert!(reg.get(id).is_some(), "missing kind id: {id}");
        }
    }
}
