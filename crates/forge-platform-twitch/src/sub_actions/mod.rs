mod add_moderator;
mod add_vip;
mod ban_user;
mod cancel_raid;
mod cancel_redemption;
mod clear_chat;
mod create_marker;
mod create_reward;
mod delete_message;
mod delete_reward;
mod disable_reward;
mod enable_reward;
mod fulfill_redemption;
mod identity;
mod pause_reward;
mod remove_moderator;
mod remove_vip;
mod reply_chat;
mod resume_reward;
mod run_ad;
mod send_announcement;
mod send_shoutout;
mod send_whisper;
mod set_mode;
mod shield_mode;
mod snooze_ad;
mod start_raid;
mod timeout_user;
mod unban_user;
mod update_category;
mod update_reward;
mod update_tags;
mod update_title;
mod warn_user;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};
use forge_storage::CredentialsRepo;

pub use add_moderator::AddModeratorRunner;
pub use add_vip::AddVipRunner;
pub use ban_user::BanUserRunner;
pub use cancel_raid::CancelRaidRunner;
pub use cancel_redemption::CancelRedemptionRunner;
pub use clear_chat::ClearChatRunner;
pub use create_marker::CreateMarkerRunner;
pub use create_reward::CreateRewardRunner;
pub use delete_message::DeleteMessageRunner;
pub use delete_reward::DeleteRewardRunner;
pub use disable_reward::DisableRewardRunner;
pub use enable_reward::EnableRewardRunner;
pub use fulfill_redemption::FulfillRedemptionRunner;
pub use identity::SelfIdentity;
pub use pause_reward::PauseRewardRunner;
pub use remove_moderator::RemoveModeratorRunner;
pub use remove_vip::RemoveVipRunner;
pub use reply_chat::ReplyChatRunner;
pub use resume_reward::ResumeRewardRunner;
pub use run_ad::RunAdRunner;
pub use send_announcement::SendAnnouncementRunner;
pub use send_shoutout::SendShoutoutRunner;
pub use send_whisper::SendWhisperRunner;
pub use set_mode::SetModeRunner;
pub use shield_mode::{ShieldModeOffRunner, ShieldModeOnRunner};
pub use snooze_ad::SnoozeAdRunner;
pub use start_raid::StartRaidRunner;
pub use timeout_user::TimeoutUserRunner;
pub use unban_user::UnbanUserRunner;
pub use update_category::UpdateCategoryRunner;
pub use update_reward::UpdateRewardRunner;
pub use update_tags::UpdateTagsRunner;
pub use update_title::UpdateTitleRunner;
pub use warn_user::WarnUserRunner;

use crate::helix::HelixTransport;

pub fn register_twitch_sub_actions(
    reg: &mut SubActionRegistry,
    transport: Arc<dyn HelixTransport>,
    creds: Arc<dyn CredentialsRepo>,
) -> Result<(), RegistryError> {
    let identity = Arc::new(SelfIdentity::new(creds));
    reg.register(Box::new(SendAnnouncementRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(DeleteMessageRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(ClearChatRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(SetModeRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(BanUserRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(TimeoutUserRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(UnbanUserRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(WarnUserRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(AddModeratorRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(RemoveModeratorRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(AddVipRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(RemoveVipRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(ShieldModeOnRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(ShieldModeOffRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(ReplyChatRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(SendWhisperRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(UpdateTitleRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(UpdateCategoryRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(UpdateTagsRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(CreateMarkerRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(SendShoutoutRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(StartRaidRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(CancelRaidRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(RunAdRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(SnoozeAdRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(CreateRewardRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(UpdateRewardRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(EnableRewardRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(DisableRewardRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(PauseRewardRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(ResumeRewardRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(DeleteRewardRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(FulfillRedemptionRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(CancelRedemptionRunner::new(transport, identity)))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) mod test_support {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use forge_events::{Event, EventPublisher};
    use forge_registry::RunContext;
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};
    use forge_types::{ArgStack, EventId};
    use time::OffsetDateTime;

    use crate::helix::{HelixError, HelixRequest, HelixTransport};

    pub(crate) const TOKEN_SENTINEL: &str = "FAKE_TWITCH_TOKEN_SENTINEL_qq123";
    pub(crate) const SELF_USER_ID: &str = "9876";

    pub(crate) struct MockTransport {
        calls: Mutex<Vec<HelixRequest>>,
        responses: Mutex<VecDeque<Result<serde_json::Value, HelixError>>>,
    }

    impl MockTransport {
        pub(crate) fn returning(response: Result<serde_json::Value, HelixError>) -> Self {
            Self::returning_sequence(vec![response])
        }

        /// Queues responses consumed one per `execute` call (FIFO), for
        /// runners that issue several Helix calls (e.g. resolve-then-act).
        /// Exhausted queue yields `Ok(Null)`.
        pub(crate) fn returning_sequence(
            responses: Vec<Result<serde_json::Value, HelixError>>,
        ) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                responses: Mutex::new(responses.into()),
            }
        }

        pub(crate) fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }

        pub(crate) fn request(&self, index: usize) -> HelixRequest {
            self.calls.lock().unwrap()[index].clone()
        }

        pub(crate) fn last_request(&self) -> HelixRequest {
            self.calls.lock().unwrap().last().unwrap().clone()
        }
    }

    #[async_trait]
    impl HelixTransport for MockTransport {
        async fn execute(&self, request: HelixRequest) -> Result<serde_json::Value, HelixError> {
            self.calls.lock().unwrap().push(request);
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(serde_json::Value::Null))
        }
    }

    /// Canned GET /helix/users payload resolving the target login to `id`.
    pub(crate) fn users_fixture(id: &str) -> Result<serde_json::Value, HelixError> {
        Ok(serde_json::json!({ "data": [{ "id": id, "login": "target" }] }))
    }

    pub(crate) struct MockCreds {
        bundle: Option<String>,
    }

    impl MockCreds {
        /// Canned `twitch:broadcaster` bundle with a sentinel token, so leak
        /// assertions can prove the token never surfaces in outcomes.
        pub(crate) fn with_identity() -> Self {
            Self {
                bundle: Some(
                    serde_json::json!({
                        "access_token": TOKEN_SENTINEL,
                        "user_id": SELF_USER_ID,
                        "login": "streamer",
                        "expires_at_unix": null,
                    })
                    .to_string(),
                ),
            }
        }

        pub(crate) fn empty() -> Self {
            Self { bundle: None }
        }
    }

    #[async_trait]
    impl CredentialsRepo for MockCreds {
        async fn store(&self, _: &CredentialId, _: &str) -> Result<(), StorageError> {
            Ok(())
        }

        async fn load(&self, _: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(self.bundle.clone())
        }

        async fn delete(&self, _: &CredentialId) -> Result<bool, StorageError> {
            Ok(false)
        }

        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(Vec::new())
        }

        async fn last_refresh(
            &self,
            _: &CredentialId,
        ) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }

        async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
            Ok(())
        }
    }

    struct NoopPublisher;

    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    pub(crate) fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext {
            arg_stack: stack,
            index: 0,
            parent_event_id: EventId::new(),
            publisher: &NoopPublisher,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::test_support::{MockCreds, MockTransport};
    use super::*;

    #[test]
    fn register_twitch_sub_actions_resolves_all_runner_ids() {
        let mut reg = SubActionRegistry::new();

        register_twitch_sub_actions(
            &mut reg,
            Arc::new(MockTransport::returning(Ok(serde_json::Value::Null))),
            Arc::new(MockCreds::empty()),
        )
        .unwrap();

        for id in [
            "twitch.chat.send_announcement",
            "twitch.chat.delete_message",
            "twitch.chat.clear",
            "twitch.chat.set_mode",
            "twitch.moderation.ban_user",
            "twitch.moderation.timeout_user",
            "twitch.moderation.unban_user",
            "twitch.moderation.warn_user",
            "twitch.moderation.add_moderator",
            "twitch.moderation.remove_moderator",
            "twitch.moderation.add_vip",
            "twitch.moderation.remove_vip",
            "twitch.moderation.shield_mode_on",
            "twitch.moderation.shield_mode_off",
            "twitch.chat.reply",
            "twitch.chat.send_whisper",
            "twitch.channel.update_title",
            "twitch.channel.update_category",
            "twitch.channel.update_tags",
            "twitch.channel.create_marker",
            "twitch.channel.send_shoutout",
            "twitch.channel.start_raid",
            "twitch.channel.cancel_raid",
            "twitch.channel.run_ad",
            "twitch.channel.snooze_ad",
            "twitch.channel_points.create_reward",
            "twitch.channel_points.update_reward",
            "twitch.channel_points.enable_reward",
            "twitch.channel_points.disable_reward",
            "twitch.channel_points.pause_reward",
            "twitch.channel_points.resume_reward",
            "twitch.channel_points.delete_reward",
            "twitch.channel_points.fulfill_redemption",
            "twitch.channel_points.cancel_redemption",
        ] {
            assert!(reg.get(id).is_some(), "missing sub-action: {id}");
        }
    }
}
