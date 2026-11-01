mod ad_break_started;
mod automatic_reward_redeemed;
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
mod channel_updated;
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
mod families;
mod goal_ended;
mod goal_progress;
mod goal_started;
mod guest_star_guest_updated;
mod guest_star_session_began;
mod guest_star_session_ended;
mod guest_star_settings_updated;
mod hype_train_ended;
mod hype_train_progress;
mod hype_train_started;
mod moderator_added;
mod moderator_removed;
mod payload_read;
mod poll_ended;
mod poll_progress;
mod poll_started;
mod prediction_ended;
mod prediction_locked;
mod prediction_progress;
mod prediction_started;
mod raid_sent;
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
mod spec;
mod stream_offline;
mod stream_online;
mod support_cheer;
mod support_gift_sub;
mod support_resubscriber;
mod support_subscriber;
mod suspicious_user_message;
mod unban_request_created;
mod unban_request_resolved;
mod user_update;
mod vip_added;
mod vip_removed;
mod warning_acknowledged;
mod warning_sent;
mod whisper_received;

use forge_registry::{RegistryError, TriggerRegistry};

const HYPE_TRAIN_LEVEL_MAX: i64 = 20;
const MAX_SYNTHESIZED_COUNT: i64 = 1_000_000;

use spec::TwitchTriggerSpec;

const TRIGGERS: &[TwitchTriggerSpec] = &[
    channel_ban::SPEC,
    channel_timeout::SPEC,
    channel_unban::SPEC,
    chat_command::SPEC,
    chat_message::SPEC,
    chat_cheer_message::SPEC,
    shared_chat_message::SPEC,
    chat_message_deleted::SPEC,
    chat_cleared::SPEC,
    support_subscriber::SPEC,
    support_resubscriber::SPEC,
    support_gift_sub::SPEC,
    support_cheer::SPEC,
    channel_raid_received::SPEC,
    raid_sent::SPEC,
    channel_follow::SPEC,
    channel_points_redemption::SPEC,
    stream_online::SPEC,
    stream_offline::SPEC,
    hype_train_started::SPEC,
    hype_train_progress::SPEC,
    hype_train_ended::SPEC,
    charity_donation::SPEC,
    charity_started::SPEC,
    charity_progress::SPEC,
    charity_stopped::SPEC,
    moderator_added::SPEC,
    moderator_removed::SPEC,
    unban_request_created::SPEC,
    unban_request_resolved::SPEC,
    user_update::SPEC,
    vip_added::SPEC,
    vip_removed::SPEC,
    shield_mode_started::SPEC,
    shield_mode_ended::SPEC,
    shoutout_sent::SPEC,
    shoutout_received::SPEC,
    suspicious_user_message::SPEC,
    warning_acknowledged::SPEC,
    warning_sent::SPEC,
    poll_started::SPEC,
    poll_progress::SPEC,
    poll_ended::SPEC,
    prediction_started::SPEC,
    prediction_progress::SPEC,
    prediction_locked::SPEC,
    prediction_ended::SPEC,
    goal_started::SPEC,
    goal_progress::SPEC,
    goal_ended::SPEC,
    reward_added::SPEC,
    reward_updated::SPEC,
    reward_removed::SPEC,
    redemption_updated::SPEC,
    automod_message_held::SPEC,
    automod_settings_updated::SPEC,
    automod_terms_updated::SPEC,
    automod_message_updated::SPEC,
    chat_settings_updated::SPEC,
    guest_star_session_began::SPEC,
    guest_star_session_ended::SPEC,
    guest_star_settings_updated::SPEC,
    guest_star_guest_updated::SPEC,
    shared_chat_session_began::SPEC,
    shared_chat_session_updated::SPEC,
    shared_chat_session_ended::SPEC,
    channel_updated::SPEC,
    ad_break_started::SPEC,
    automatic_reward_redeemed::SPEC,
    whisper_received::SPEC,
];

pub fn register_twitch_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    let mut reg = reg.owned_by(crate::TWITCH_INTEGRATION.id);
    for spec in TRIGGERS {
        reg.register(Box::new(*spec))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use forge_registry::{
        ActorRole, ActorSlot, CanonicalCount, CanonicalVariable, TriggerKindDescriptor,
    };

    #[test]
    fn the_hype_level_form_accepts_every_level_the_variable_hints_can_produce() {
        let form_max = match hype_train_progress::SPEC.config_fields().as_slice() {
            [forge_registry::FormField::Integer { max, .. }] => *max,
            other => panic!("expected one integer field, got {} fields", other.len()),
        };
        let descriptors: [&dyn TriggerKindDescriptor; 3] = [
            &hype_train_started::SPEC,
            &hype_train_progress::SPEC,
            &hype_train_ended::SPEC,
        ];

        for descriptor in descriptors {
            let schema = descriptor.output_schema().unwrap();
            let level = schema.variables.iter().find(|v| v.name == "hype.level");
            assert_eq!(
                level.and_then(|v| v.synthesis.clone()),
                Some(forge_types::SynthesisHint::BoundedInt {
                    min: 1,
                    max: form_max
                }),
                "{}",
                descriptor.id()
            );
        }
    }

    fn twitch_registry() -> TriggerRegistry {
        let mut registry = TriggerRegistry::new();
        register_twitch_triggers(&mut registry).unwrap();
        registry
    }

    fn legacy_twins(
        descriptor: &dyn TriggerKindDescriptor,
    ) -> Vec<(forge_registry::TriggerVariable, CanonicalVariable)> {
        forge_registry::declared_variables(descriptor)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|variable| {
                let canonical = variable.standing.superseded_by()?;
                Some((variable, canonical))
            })
            .collect()
    }

    #[test]
    fn every_legacy_name_is_declared_with_the_kind_and_hint_of_a_canonical_twin_on_its_trigger() {
        for descriptor in twitch_registry().all() {
            let declarations = forge_registry::declared_variables(descriptor).unwrap_or_default();
            for (legacy, canonical) in legacy_twins(descriptor) {
                let twin = declarations.iter().find(|variable| {
                    variable.standing == forge_registry::VariableStanding::Canonical(canonical)
                });
                assert_eq!(
                    twin.map(|twin| (twin.declared.kind, twin.declared.synthesis.clone())),
                    Some((legacy.declared.kind, legacy.declared.synthesis.clone())),
                    "'{}' legacy '{}' defers to '{}'",
                    descriptor.id(),
                    legacy.declared.name,
                    canonical.name()
                );
            }
        }
    }

    fn twin_its_name_describes(legacy: &str) -> Option<CanonicalVariable> {
        let role = [
            ("moderator", ActorRole::Moderator),
            ("gifter", ActorRole::Gifter),
            ("recipient", ActorRole::Recipient),
        ]
        .into_iter()
        .find(|(word, _)| legacy.contains(word))
        .map_or(ActorRole::Principal, |(_, role)| role);
        let actor = |slot| Some(CanonicalVariable::actor(role, slot));
        let ends = |suffixes: &[&str]| suffixes.iter().any(|suffix| legacy.ends_with(suffix));
        if ends(&["tier", "level_name"]) {
            Some(CanonicalVariable::SubTier)
        } else if ends(&["display_name"]) {
            actor(ActorSlot::Name)
        } else if ends(&["login", "username"]) {
            actor(ActorSlot::Login)
        } else if ends(&["message_id"]) {
            Some(CanonicalVariable::MessageId)
        } else if ends(&["id"]) {
            actor(ActorSlot::Id)
        } else if ends(&["bits"]) {
            Some(CanonicalVariable::Count(CanonicalCount::BitsAmount))
        } else if ends(&["viewer_count"]) {
            Some(CanonicalVariable::Count(CanonicalCount::ViewerCount))
        } else if ends(&["count"]) {
            Some(CanonicalVariable::Count(CanonicalCount::GiftCount))
        } else if ends(&["month", "months"]) {
            Some(CanonicalVariable::Count(
                CanonicalCount::SubCumulativeMonths,
            ))
        } else if ends(&["content", "input", "text", "message"]) {
            Some(CanonicalVariable::MessageText)
        } else {
            None
        }
    }

    #[test]
    fn every_legacy_name_defers_to_the_canonical_its_name_describes() {
        for descriptor in twitch_registry().all() {
            for (legacy, canonical) in legacy_twins(descriptor) {
                assert_eq!(
                    twin_its_name_describes(&legacy.declared.name),
                    Some(canonical),
                    "'{}' legacy '{}'",
                    descriptor.id(),
                    legacy.declared.name
                );
            }
        }
    }

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
            "twitch.channel.raid_sent",
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
            "twitch.channel.unban_request_created",
            "twitch.channel.unban_request_resolved",
            "twitch.channel.shield_mode_started",
            "twitch.channel.shield_mode_ended",
            "twitch.channel.shoutout_sent",
            "twitch.channel.shoutout_received",
            "twitch.channel.suspicious_user_message",
            "twitch.channel.warning_acknowledged",
            "twitch.channel.warning_sent",
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
            "twitch.guest_star.guest_updated",
            "twitch.shared_chat.session_began",
            "twitch.shared_chat.session_updated",
            "twitch.shared_chat.session_ended",
            "twitch.channel.update",
            "twitch.stream.ad_break_started",
            "twitch.channel_points.automatic_reward_redeemed",
            "twitch.chat.whisper",
            "twitch.user.update",
        ];

        for id in ids {
            assert!(reg.get(id).is_some(), "missing kind id: {id}");
        }
    }
}
