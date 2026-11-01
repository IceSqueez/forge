use super::*;

impl ChatSession {
    pub(in crate::chat) fn publish_reward_add_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let state = wire_custom_reward(event_data);

        debug!(reward_id = %state.reward.id, "channel point reward added");

        self.publish_typed(CustomRewardAdded(state));
    }

    pub(in crate::chat) fn publish_reward_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let state = wire_custom_reward(event_data);

        debug!(reward_id = %state.reward.id, "channel point reward updated");

        self.publish_typed(CustomRewardUpdated(state));
    }

    pub(in crate::chat) fn publish_reward_remove_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let state = wire_custom_reward(event_data);

        debug!(reward_id = %state.reward.id, "channel point reward removed");

        self.publish_typed(CustomRewardRemoved(state));
    }

    pub(in crate::chat) fn publish_redemption_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let redemption = wire_redemption(event_data);
        let user = wire_user(event_data, &USER_KEYS);
        let reward = wire_child(event_data, "reward");
        let reward = UpdatedRedemptionReward {
            id: wire_text(reward, "id"),
            title: wire_text(reward, "title"),
            cost: wire_count(reward, "cost"),
        };

        debug!(user_id = %user.id, reward_id = %reward.id, status = %redemption.status, "channel point redemption updated");

        self.publish_typed(RedemptionUpdated {
            redemption,
            user,
            reward,
        });
    }

    pub(in crate::chat) fn publish_automod_hold_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);
        let reason = wire_text(event_data, WIRE_AUTOMOD_REASON);
        let classification = wire_automod_classification(event_data, &reason);

        debug!(
            user_id = %user.id,
            reason = %reason,
            category = %classification.category,
            level = classification.level,
            "automod hold received"
        );

        self.publish_typed(AutomodMessageHeld {
            automod: HeldMessage {
                message_id: wire_text(event_data, "message_id"),
                category: classification.category_unless_blocked_term(),
                level: classification.level_unless_blocked_term(),
                held_at: wire_text(event_data, "held_at"),
            },
            reason,
            blocked_term: classification.blocked_term_match(),
            user,
            message_text: wire_text(wire_child(event_data, "message"), "text"),
        });
    }

    pub(in crate::chat) fn publish_chat_settings_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let settings = ChatSettings {
            emote_mode: wire_flag(event_data, "emote_mode"),
            follower_mode: wire_flag(event_data, "follower_mode"),
            follower_mode_duration_minutes: wire_optional_count(
                event_data,
                "follower_mode_duration_minutes",
            ),
            slow_mode: wire_flag(event_data, "slow_mode"),
            slow_mode_wait_time_seconds: wire_optional_count(
                event_data,
                "slow_mode_wait_time_seconds",
            ),
            subscriber_mode: wire_flag(event_data, "subscriber_mode"),
            unique_chat_mode: wire_flag(event_data, "unique_chat_mode"),
        };

        debug!(
            emote_mode = settings.emote_mode,
            follower_mode = settings.follower_mode,
            slow_mode = settings.slow_mode,
            subscriber_mode = settings.subscriber_mode,
            unique_chat_mode = settings.unique_chat_mode,
            "chat settings updated"
        );

        self.publish_typed(ChatSettingsUpdated { settings });
    }

    pub(in crate::chat) fn publish_guest_star_session_begin_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let session = GuestStarSessionOpening {
            id: wire_text(event_data, "session_id"),
            started_at: wire_text(event_data, "started_at"),
        };

        debug!(session_id = %session.id, "guest star session began");

        self.publish_typed(GuestStarSessionBegan { session });
    }

    pub(in crate::chat) fn publish_guest_star_session_end_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let session = GuestStarSessionSpan {
            id: wire_text(event_data, "session_id"),
            started_at: wire_text(event_data, "started_at"),
            ended_at: wire_text(event_data, "ended_at"),
        };

        debug!(session_id = %session.id, "guest star session ended");

        self.publish_typed(GuestStarSessionEnded { session });
    }

    pub(in crate::chat) fn publish_guest_star_settings_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let settings = GuestStarSettings {
            slot_count: wire_count(event_data, "slot_count"),
            group_layout: wire_text(event_data, "group_layout"),
            is_moderator_send_live_enabled: wire_flag(event_data, "is_moderator_send_live_enabled"),
            is_browser_source_audio_enabled: wire_flag(
                event_data,
                "is_browser_source_audio_enabled",
            ),
        };

        debug!(slot_count = settings.slot_count, group_layout = %settings.group_layout, "guest star settings updated");

        self.publish_typed(GuestStarSettingsUpdated { settings });
    }

    pub(in crate::chat) fn publish_guest_star_guest_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let session_id = wire_text(event_data, "session_id");
        let state = wire_text(event_data, "state");
        let guest = GuestStarGuest {
            id: wire_non_empty(event_data, "guest_user_id").map(str::to_owned),
            login: wire_non_empty(event_data, "guest_user_login").map(str::to_owned),
            display_name: wire_non_empty(event_data, "guest_user_name").map(str::to_owned),
        };

        debug!(
            session_id = %session_id,
            guest_user_id = %guest.id.as_deref().unwrap_or_default(),
            state = %state,
            "guest star guest update"
        );

        self.publish_typed(GuestStarGuestUpdated {
            session_id,
            slot_id: wire_text(event_data, "slot_id"),
            state,
            guest,
            host: GuestStarHost {
                video_enabled: wire_flag(event_data, "host_video_enabled"),
                audio_enabled: wire_flag(event_data, "host_audio_enabled"),
                volume: wire_count(event_data, "host_volume"),
            },
        });
    }

    pub(in crate::chat) fn publish_automod_settings_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let moderator = wire_user(event_data, &MODERATOR_KEYS);

        debug!(moderator_id = %moderator.id, "automod settings updated");

        self.publish_typed(AutomodSettingsUpdated {
            moderator,
            overall_level: wire_optional_count(event_data, "overall_level"),
        });
    }

    pub(in crate::chat) fn publish_automod_terms_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let moderator = wire_user(event_data, &MODERATOR_KEYS);
        let action = wire_text(event_data, "action");

        debug!(moderator_id = %moderator.id, action = %action, "automod terms updated");

        self.publish_typed(AutomodTermsUpdated {
            moderator,
            action,
            terms: wire_list(event_data, "terms"),
        });
    }

    pub(in crate::chat) fn publish_automod_message_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);
        let status = wire_text(event_data, "status");
        let reason = wire_text(event_data, WIRE_AUTOMOD_REASON);
        let classification = wire_automod_classification(event_data, &reason);

        debug!(
            user_id = %user.id,
            status = %status,
            reason = %reason,
            category = %classification.category,
            "automod message updated"
        );

        self.publish_typed(AutomodMessageUpdated {
            automod: ReviewedMessage {
                message_id: wire_text(event_data, "message_id"),
                status,
                category: classification.category_unless_blocked_term(),
                level: classification.level_unless_blocked_term(),
            },
            reason,
            blocked_term: classification.blocked_term_match(),
            user,
            moderator: wire_user(event_data, &MODERATOR_KEYS),
            message_text: wire_text(wire_child(event_data, "message"), "text"),
        });
    }

    pub(in crate::chat) fn publish_shared_chat_begin_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let state = wire_shared_chat(event_data);

        debug!(session_id = %state.shared_chat.session_id, host_id = %state.host.id, "shared chat session began");

        self.publish_typed(SharedChatBegan(state));
    }

    pub(in crate::chat) fn publish_shared_chat_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let state = wire_shared_chat(event_data);

        debug!(session_id = %state.shared_chat.session_id, host_id = %state.host.id, "shared chat session updated");

        self.publish_typed(SharedChatUpdated(state));
    }

    pub(in crate::chat) fn publish_shared_chat_end_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let state = wire_shared_chat(event_data);

        debug!(session_id = %state.shared_chat.session_id, host_id = %state.host.id, "shared chat session ended");

        self.publish_typed(SharedChatEnded(state));
    }

    pub(in crate::chat) fn publish_channel_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let channel = ChannelInfo {
            title: wire_text(event_data, "title"),
            language: wire_text(event_data, "language"),
            category_id: wire_text(event_data, "category_id"),
            category_name: wire_text(event_data, "category_name"),
        };

        debug!(category_id = %channel.category_id, "channel update event received");

        self.publish_typed(ChannelUpdated { channel });
    }

    pub(in crate::chat) fn publish_ad_break_begin_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let ad_break = AdBreak {
            duration_seconds: wire_count(event_data, "duration_seconds"),
            is_automatic: wire_flag(event_data, "is_automatic"),
            started_at: wire_text(event_data, "started_at"),
        };

        debug!(
            duration_seconds = ad_break.duration_seconds,
            is_automatic = ad_break.is_automatic,
            "ad break begin event received"
        );

        self.publish_typed(AdBreakBegan {
            ad_break,
            requester: wire_user(event_data, &REQUESTER_KEYS),
        });
    }

    pub(in crate::chat) fn publish_automatic_reward_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);
        let reward = wire_child(event_data, "reward");
        let reward = AutomaticReward {
            reward_type: wire_text(reward, "type"),
            cost: wire_count(reward, "cost"),
        };

        debug!(user_id = %user.id, reward_type = %reward.reward_type, "automatic channel point reward redeemed");

        self.publish_typed(AutomaticRewardRedeemed {
            redemption: AutomaticRedemption {
                id: wire_text(event_data, "id"),
                redeemed_at: wire_text(event_data, "redeemed_at"),
            },
            user,
            reward,
        });
    }

    pub(in crate::chat) fn publish_whisper_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let sender = wire_user(event_data, &WHISPER_SENDER_KEYS);

        debug!(from_user_id = %sender.id, "whisper received");

        self.publish_typed(WhisperReceived {
            user: WhisperSender {
                id: sender.id,
                login: sender.login,
                display_name: sender.display_name,
                color: None,
            },
            whisper: WhisperBody {
                text: wire_text(wire_child(event_data, "whisper"), "text"),
            },
            whisper_thread_id: wire_text(event_data, "whisper_id"),
        });
    }

    pub(in crate::chat) fn publish_user_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, "user profile updated");

        self.publish_typed(UserUpdated {
            user: UpdatedUser {
                id: user.id,
                login: user.login,
                display_name: user.display_name,
                description: wire_text(event_data, "description"),
            },
        });
    }
}
