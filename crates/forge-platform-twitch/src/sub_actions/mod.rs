mod add_blocked_term;
mod approve_automod_message;
mod ban_user;
mod cancel_prediction;
mod cancel_raid;
mod cancel_redemption;
mod clear_chat;
mod create_marker;
mod create_reward;
mod delete_message;
mod delete_reward;
mod deny_automod_message;
mod end_poll;
mod fields;
mod fulfill_redemption;
mod get_current_goal;
mod guest_star;
mod guest_star_assign_slot;
mod guest_star_end_session;
mod guest_star_invite;
mod guest_star_remove_guest;
mod guest_star_update_slot;
mod lock_prediction;
mod remove_blocked_term;
mod reply_chat;
mod resolve_prediction;
mod reward_toggle;
mod role_grant;
mod run_ad;
mod send_announcement;
mod send_shoutout;
mod send_whisper;
mod set_mode;
mod shield_mode;
mod snooze_ad;
mod start_poll;
mod start_prediction;
mod start_raid;
mod timeout_user;
pub(crate) use timeout_user::MAX_DURATION_SECONDS;
mod unban_user;
mod update_automod_settings;
mod update_category;
mod update_reward;
mod update_tags;
mod update_title;
mod warn_user;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry, SubActionRunner};

use crate::account::broadcaster::BroadcasterSlot;
use crate::account::identity::SelfIdentity;

pub(crate) use end_poll::PollEndStatus;
pub(crate) use fields::ModeToggle;
pub(crate) use run_ad::{AD_LENGTH, AdLength};
pub(crate) use send_announcement::{AnnouncementColor, COLOR as ANNOUNCEMENT_COLOR};
pub(crate) use set_mode::{EMOTE_ONLY, FOLLOWER_MODE_MIN_AGE, SLOW_MODE_WAIT};
pub(crate) use start_poll::POLL_DURATION;
pub(crate) use start_prediction::PREDICTION_WINDOW;
pub(crate) use timeout_user::TIMEOUT_DURATION;

pub(crate) use cancel_raid::CancelRaidRunner;
pub(crate) use reply_chat::ReplyChatRunner;
pub(crate) use send_announcement::SendAnnouncementRunner;
pub(crate) use send_shoutout::SendShoutoutRunner;
pub(crate) use send_whisper::SendWhisperRunner;
pub(crate) use shield_mode::ShieldModeRunner;
pub(crate) use start_raid::StartRaidRunner;

pub(crate) use add_blocked_term::AddBlockedTermRunner;
pub(crate) use approve_automod_message::ApproveAutomodMessageRunner;
pub(crate) use ban_user::BanUserRunner;
pub(crate) use cancel_prediction::CancelPredictionRunner;
pub(crate) use cancel_redemption::CancelRedemptionRunner;
pub(crate) use clear_chat::ClearChatRunner;
pub(crate) use create_marker::CreateMarkerRunner;
pub(crate) use create_reward::CreateRewardRunner;
pub(crate) use delete_message::DeleteMessageRunner;
pub(crate) use delete_reward::DeleteRewardRunner;
pub(crate) use deny_automod_message::DenyAutomodMessageRunner;
pub(crate) use end_poll::EndPollRunner;
pub(crate) use fulfill_redemption::FulfillRedemptionRunner;
pub(crate) use get_current_goal::GetCurrentGoalRunner;
pub(crate) use guest_star_assign_slot::GuestStarAssignSlotRunner;
pub(crate) use guest_star_end_session::GuestStarEndSessionRunner;
pub(crate) use guest_star_invite::GuestStarInviteRunner;
pub(crate) use guest_star_remove_guest::GuestStarRemoveGuestRunner;
pub(crate) use guest_star_update_slot::GuestStarUpdateSlotRunner;
pub(crate) use lock_prediction::LockPredictionRunner;
pub(crate) use remove_blocked_term::RemoveBlockedTermRunner;
pub(crate) use resolve_prediction::ResolvePredictionRunner;
pub(crate) use reward_toggle::RewardToggleRunner;
pub(crate) use role_grant::RoleGrantRunner;
pub(crate) use run_ad::RunAdRunner;
pub(crate) use set_mode::SetModeRunner;
pub(crate) use snooze_ad::SnoozeAdRunner;
pub(crate) use start_poll::StartPollRunner;
pub(crate) use start_prediction::StartPredictionRunner;
pub(crate) use timeout_user::TimeoutUserRunner;
pub(crate) use unban_user::UnbanUserRunner;
pub(crate) use update_automod_settings::UpdateAutomodSettingsRunner;
pub(crate) use update_category::UpdateCategoryRunner;
pub(crate) use update_reward::UpdateRewardRunner;
pub(crate) use update_tags::UpdateTagsRunner;
pub(crate) use update_title::UpdateTitleRunner;
pub(crate) use warn_user::WarnUserRunner;

use crate::bot_account::TwitchBotAccount;
use crate::cadence::CadenceLedger;
use crate::helix::HelixTransport;
use crate::lifecycle::TwitchLifecycle;

type RunnerBuild = fn(Arc<dyn HelixTransport>, Arc<SelfIdentity>) -> Box<dyn SubActionRunner>;

const PLAIN_RUNNERS: &[RunnerBuild] = &[
    |transport, identity| Box::new(DeleteMessageRunner::new(transport, identity)),
    |transport, identity| Box::new(ClearChatRunner::new(transport, identity)),
    |transport, identity| Box::new(SetModeRunner::new(transport, identity)),
    |transport, identity| Box::new(BanUserRunner::new(transport, identity)),
    |transport, identity| Box::new(TimeoutUserRunner::new(transport, identity)),
    |transport, identity| Box::new(UnbanUserRunner::new(transport, identity)),
    |transport, identity| Box::new(WarnUserRunner::new(transport, identity)),
    |transport, identity| Box::new(RoleGrantRunner::add_moderator(transport, identity)),
    |transport, identity| Box::new(RoleGrantRunner::remove_moderator(transport, identity)),
    |transport, identity| Box::new(RoleGrantRunner::add_vip(transport, identity)),
    |transport, identity| Box::new(RoleGrantRunner::remove_vip(transport, identity)),
    |transport, identity| Box::new(UpdateTitleRunner::new(transport, identity)),
    |transport, identity| Box::new(UpdateCategoryRunner::new(transport, identity)),
    |transport, identity| Box::new(UpdateTagsRunner::new(transport, identity)),
    |transport, identity| Box::new(CreateMarkerRunner::new(transport, identity)),
    |transport, identity| Box::new(RunAdRunner::new(transport, identity)),
    |transport, identity| Box::new(SnoozeAdRunner::new(transport, identity)),
    |transport, identity| Box::new(CreateRewardRunner::new(transport, identity)),
    |transport, identity| Box::new(UpdateRewardRunner::new(transport, identity)),
    |transport, identity| Box::new(RewardToggleRunner::enable(transport, identity)),
    |transport, identity| Box::new(RewardToggleRunner::disable(transport, identity)),
    |transport, identity| Box::new(RewardToggleRunner::pause(transport, identity)),
    |transport, identity| Box::new(RewardToggleRunner::resume(transport, identity)),
    |transport, identity| Box::new(DeleteRewardRunner::new(transport, identity)),
    |transport, identity| Box::new(FulfillRedemptionRunner::new(transport, identity)),
    |transport, identity| Box::new(CancelRedemptionRunner::new(transport, identity)),
    |transport, identity| Box::new(StartPollRunner::new(transport, identity)),
    |transport, identity| Box::new(EndPollRunner::new(transport, identity)),
    |transport, identity| Box::new(StartPredictionRunner::new(transport, identity)),
    |transport, identity| Box::new(LockPredictionRunner::new(transport, identity)),
    |transport, identity| Box::new(CancelPredictionRunner::new(transport, identity)),
    |transport, identity| Box::new(ResolvePredictionRunner::new(transport, identity)),
    |transport, identity| Box::new(GetCurrentGoalRunner::new(transport, identity)),
    |transport, identity| Box::new(ApproveAutomodMessageRunner::new(transport, identity)),
    |transport, identity| Box::new(DenyAutomodMessageRunner::new(transport, identity)),
    |transport, identity| Box::new(UpdateAutomodSettingsRunner::new(transport, identity)),
    |transport, identity| Box::new(AddBlockedTermRunner::new(transport, identity)),
    |transport, identity| Box::new(RemoveBlockedTermRunner::new(transport, identity)),
    |transport, identity| Box::new(GuestStarInviteRunner::new(transport, identity)),
    |transport, identity| Box::new(GuestStarAssignSlotRunner::new(transport, identity)),
    |transport, identity| Box::new(GuestStarRemoveGuestRunner::new(transport, identity)),
    |transport, identity| Box::new(GuestStarUpdateSlotRunner::new(transport, identity)),
    |transport, identity| Box::new(GuestStarEndSessionRunner::new(transport, identity)),
];

pub fn register_twitch_sub_actions(
    reg: &mut SubActionRegistry,
    transport: Arc<dyn HelixTransport>,
    broadcaster: Arc<BroadcasterSlot>,
    lifecycle: TwitchLifecycle,
    bot: Option<Arc<TwitchBotAccount>>,
) -> Result<(), RegistryError> {
    let mut reg = reg.owned_by(crate::TWITCH_INTEGRATION.id);
    let identity = Arc::new(SelfIdentity::new(broadcaster));
    let cadence = Arc::new(CadenceLedger::default());
    reg.register(Box::new(
        SendAnnouncementRunner::new(Arc::clone(&transport), Arc::clone(&identity))
            .with_bot(bot.clone())
            .with_cadence(Arc::clone(&cadence)),
    ))?;
    reg.register(Box::new(ShieldModeRunner::on(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(ShieldModeRunner::off(
        Arc::clone(&transport),
        Arc::clone(&identity),
    )))?;
    reg.register(Box::new(
        ReplyChatRunner::new(Arc::clone(&transport), Arc::clone(&identity)).with_bot(bot.clone()),
    ))?;
    reg.register(Box::new(
        SendWhisperRunner::new(Arc::clone(&transport), Arc::clone(&identity)).with_bot(bot.clone()),
    ))?;
    reg.register(Box::new(
        SendShoutoutRunner::new(Arc::clone(&transport), Arc::clone(&identity))
            .with_bot(bot.clone())
            .with_cadence(Arc::clone(&cadence)),
    ))?;
    reg.register(Box::new(StartRaidRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
        lifecycle.clone(),
    )))?;
    reg.register(Box::new(CancelRaidRunner::new(
        Arc::clone(&transport),
        Arc::clone(&identity),
        lifecycle,
    )))?;
    for build in PLAIN_RUNNERS {
        reg.register(build(Arc::clone(&transport), Arc::clone(&identity)))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockTransport;

    #[test]
    fn register_twitch_sub_actions_resolves_all_runner_ids() {
        let mut reg = SubActionRegistry::new();

        register_twitch_sub_actions(
            &mut reg,
            Arc::new(MockTransport::returning(Ok(serde_json::Value::Null))),
            Arc::new(crate::account::broadcaster::BroadcasterSlot::default()),
            TwitchLifecycle::new(),
            None,
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
            "twitch.poll.start",
            "twitch.poll.end",
            "twitch.prediction.start",
            "twitch.prediction.lock",
            "twitch.prediction.cancel",
            "twitch.prediction.resolve",
            "twitch.goal.get_current",
            "twitch.automod.approve_message",
            "twitch.automod.deny_message",
            "twitch.automod.update_settings",
            "twitch.automod.add_blocked_term",
            "twitch.automod.remove_blocked_term",
            "twitch.guest_star.invite",
            "twitch.guest_star.assign_slot",
            "twitch.guest_star.remove_guest",
            "twitch.guest_star.update_slot",
            "twitch.guest_star.end_session",
        ] {
            assert!(reg.get(id).is_some(), "missing sub-action: {id}");
        }
    }
}
