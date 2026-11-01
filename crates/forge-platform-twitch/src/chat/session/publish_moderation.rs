use super::*;

impl ChatSession {
    pub(in crate::chat) fn publish_ban_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);
        let is_permanent = wire_flag(event_data, "is_permanent");

        debug!(
            user_id = %user.id,
            is_permanent,
            "ban event received"
        );

        let removal = ChatModerationAction::RemoveUser {
            user_id: (!user.id.is_empty()).then(|| user.id.clone()),
            user_name: user.display_name.clone(),
            removal: if is_permanent {
                UserRemoval::Ban
            } else {
                UserRemoval::Timeout
            },
        };
        self.publish_with_moderation(
            UserBanned {
                user,
                moderator: wire_user(event_data, &MODERATOR_KEYS),
                reason: wire_text(event_data, "reason"),
                banned_at: wire_text(event_data, "banned_at"),
                ends_at: event_data
                    .get("ends_at")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                is_permanent,
            },
            removal,
        );
    }

    pub(in crate::chat) fn publish_unban_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, "unban event received");

        self.publish_typed(UserUnbanned {
            user,
            moderator: wire_user(event_data, &MODERATOR_KEYS),
        });
    }

    pub(in crate::chat) fn publish_moderator_add_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, "moderator added");

        self.publish_typed(ModeratorAdded(UserRef { user }));
    }

    pub(in crate::chat) fn publish_moderator_remove_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, "moderator removed");

        self.publish_typed(ModeratorRemoved(UserRef { user }));
    }

    pub(in crate::chat) fn publish_vip_add_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, "vip added");

        self.publish_typed(VipAdded(UserRef { user }));
    }

    pub(in crate::chat) fn publish_vip_remove_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, "vip removed");

        self.publish_typed(VipRemoved(UserRef { user }));
    }

    pub(in crate::chat) fn publish_unban_request_create_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let id = wire_text(event_data, "id");
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, request_id = %id, "unban request created");

        self.publish_typed(UnbanRequestCreated {
            id,
            user,
            reason_text: wire_text(event_data, "text"),
        });
    }

    pub(in crate::chat) fn publish_unban_request_resolve_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let id = wire_text(event_data, "id");
        let user = wire_user(event_data, &USER_KEYS);
        let status = wire_text(event_data, "status");

        debug!(user_id = %user.id, request_id = %id, status = %status, "unban request resolved");

        self.publish_typed(UnbanRequestResolved {
            id,
            user,
            status,
            moderator: wire_user(event_data, &MODERATOR_KEYS),
            resolution_text: wire_text(event_data, "resolution_text"),
        });
    }

    pub(in crate::chat) fn publish_shield_mode_begin_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let moderator = wire_user(event_data, &MODERATOR_KEYS);

        debug!(moderator_id = %moderator.id, "shield mode started");

        self.publish_typed(ShieldModeBegan {
            moderator,
            started_at: wire_text(event_data, "started_at"),
        });
    }

    pub(in crate::chat) fn publish_shield_mode_end_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let moderator = wire_user(event_data, &MODERATOR_KEYS);

        debug!(moderator_id = %moderator.id, "shield mode ended");

        self.publish_typed(ShieldModeEnded {
            moderator,
            ended_at: wire_text(event_data, "ended_at"),
        });
    }

    pub(in crate::chat) fn publish_shoutout_create_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let to_broadcaster = wire_user(event_data, &TO_BROADCASTER_KEYS);
        let viewer_count = wire_count(event_data, "viewer_count");

        debug!(to_broadcaster_id = %to_broadcaster.id, viewer_count, "shoutout sent");

        self.publish_typed(ShoutoutSent {
            to_broadcaster,
            viewer_count,
            started_at: wire_text(event_data, "started_at"),
        });
    }

    pub(in crate::chat) fn publish_shoutout_receive_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let from_broadcaster = wire_user(event_data, &FROM_BROADCASTER_KEYS);
        let viewer_count = wire_count(event_data, "viewer_count");

        debug!(from_broadcaster_id = %from_broadcaster.id, viewer_count, "shoutout received");

        self.publish_typed(ShoutoutReceived {
            from_broadcaster,
            viewer_count,
            started_at: wire_text(event_data, "started_at"),
        });
    }

    pub(in crate::chat) fn publish_suspicious_user_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);
        let low_trust_status = wire_text(event_data, "low_trust_status");

        debug!(user_id = %user.id, low_trust_status = %low_trust_status, "suspicious user message");

        self.publish_typed(SuspiciousUserMessaged {
            user,
            low_trust_status,
            message_text: wire_text(wire_child(event_data, "message"), "text"),
        });
    }

    pub(in crate::chat) fn publish_warning_acknowledge_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, "warning acknowledged");

        self.publish_typed(WarningAcknowledged(UserRef { user }));
    }

    pub(in crate::chat) fn publish_warning_send_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);
        let moderator = wire_user(event_data, &MODERATOR_KEYS);

        debug!(user_id = %user.id, moderator_id = %moderator.id, "warning sent");

        self.publish_typed(WarningSent {
            user,
            moderator,
            reason: wire_text(event_data, "reason"),
            chat_rules_cited: wire_list(event_data, "chat_rules_cited"),
        });
    }
}
