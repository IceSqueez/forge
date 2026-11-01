use super::*;

impl ChatSession {
    pub(in crate::chat) fn publish_chat_message(&self, event_data: &serde_json::Value) {
        let user_login = wire_text(event_data, "chatter_user_login");
        let user_id = wire_text(event_data, "chatter_user_id");
        let user_display = event_data
            .get("chatter_user_name")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(user_login.as_str())
            .to_owned();
        let roles = extract_roles_from_badges(event_data.get("badges"));
        let broadcaster_id = event_data
            .get("broadcaster_user_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();

        debug!(
            broadcaster_id = %broadcaster_id,
            user_id = %user_id,
            "chat message received"
        );

        let chat_payload = payload::build_chat_message_chat_payload(event_data);
        let cheer = event_data
            .get("cheer")
            .and_then(|c| c.get("bits"))
            .and_then(serde_json::Value::as_i64)
            .map(|bits| ChatCheer { bits });
        let source_id = event_data
            .get("source_broadcaster_user_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let from_channel = if !source_id.is_empty() && source_id != broadcaster_id {
            wire_non_empty(event_data, "source_broadcaster_user_login")
                .zip(wire_non_empty(event_data, "source_broadcaster_user_name"))
                .map(|(login, display_name)| ChatOrigin {
                    login: login.to_owned(),
                    display_name: display_name.to_owned(),
                })
        } else {
            None
        };
        let is_connected_bot = self
            .config
            .bot
            .as_ref()
            .and_then(|bot| bot.connected_user_id())
            .is_some_and(|bot_user_id| bot_user_id == user_id);
        let viewer = ChatViewer::new(user_id.clone(), user_login.clone());
        let reply = event_data
            .get("reply")
            .filter(|v| !v.is_null())
            .and_then(|reply| {
                let parent_author = reply.get("parent_user_name")?.as_str()?;
                let parent_text = reply.get("parent_message_body")?.as_str()?;
                Some(ChatReply {
                    parent_author: parent_author.to_owned(),
                    parent_text: parent_text.to_owned(),
                })
            });

        let message = ChatMessageReceived {
            channel: wire_text(event_data, "broadcaster_user_login"),
            user: ChatAuthor {
                login: user_login,
                id: user_id,
                display_name: user_display,
                roles: roles.clone(),
            },
            message: wire_text(wire_child(event_data, "message"), "text"),
            badges: roles,
            color: wire_text(event_data, "color"),
            cheer,
            from_channel,
        };
        let mut event = twitch_event(message);
        if is_connected_bot && let Some(fields) = event.payload.as_object_mut() {
            fields.insert(
                AUTHOR_IS_CONNECTED_BOT_FIELD.to_owned(),
                serde_json::Value::Bool(true),
            );
        }
        viewer.attach(&mut event.payload);
        attach_chat_payload(&mut event.payload, chat_payload);
        if let Some(reply) = reply {
            attach_chat_reply_payload(&mut event.payload, reply);
        }
        self.config.bus.publish(event);
    }

    pub(in crate::chat) fn publish_subscribe_event(
        &self,
        event_data: &serde_json::Value,
        frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, "subscriber event received");

        let chat_payload = payload::build_subscribe_chat_payload(event_data, frame_msg_id);
        self.publish_with_chat(
            ChannelSubscribed {
                user,
                tier: wire_text(event_data, "tier"),
                is_gift: wire_flag(event_data, "is_gift"),
            },
            chat_payload,
        );
    }

    pub(in crate::chat) fn publish_resubscribe_event(
        &self,
        event_data: &serde_json::Value,
        frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);
        let cumulative_months = wire_count(event_data, "cumulative_months");

        debug!(user_id = %user.id, cumulative_months, "resub event received");

        let chat_payload = payload::build_resubscribe_chat_payload(event_data, frame_msg_id);
        self.publish_with_chat(
            ChannelResubscribed {
                user,
                tier: wire_text(event_data, "tier"),
                cumulative_months,
                streak_months: wire_count(event_data, "streak_months"),
                message: wire_text(wire_child(event_data, "message"), "text"),
                share_streak: wire_flag(event_data, "share_streak"),
            },
            chat_payload,
        );
    }

    pub(in crate::chat) fn publish_gift_sub_event(
        &self,
        event_data: &serde_json::Value,
        frame_msg_id: &str,
    ) {
        let gifter = wire_user(event_data, &USER_KEYS);

        debug!(gifter_id = %gifter.id, "gift sub event received");

        let chat_payload = payload::build_gift_sub_chat_payload(event_data, frame_msg_id);
        self.publish_with_chat(
            SubscriptionsGifted {
                tier: wire_text(event_data, "tier"),
                is_anonymous: wire_flag(event_data, "is_anonymous"),
                gift_total: wire_count(event_data, "total"),
                gifter,
            },
            chat_payload,
        );
    }

    pub(in crate::chat) fn publish_cheer_event(
        &self,
        event_data: &serde_json::Value,
        frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);
        let bits = wire_count(event_data, "bits");

        debug!(user_id = %user.id, bits, "cheer event received");

        let chat_payload = payload::build_cheer_chat_payload(event_data, frame_msg_id);
        self.publish_with_chat(
            BitsCheered {
                bits,
                message: wire_text(event_data, "message"),
                is_anonymous: wire_flag(event_data, "is_anonymous"),
                user,
            },
            chat_payload,
        );
    }

    pub(in crate::chat) fn publish_raid_event(
        &self,
        event_data: &serde_json::Value,
        frame_msg_id: &str,
    ) {
        let from_broadcaster = wire_user(event_data, &FROM_BROADCASTER_KEYS);
        let to_broadcaster = wire_user(event_data, &TO_BROADCASTER_KEYS);
        let viewer_count = wire_count(event_data, "viewers");
        let direction = if to_broadcaster.id == self.config.broadcaster_id {
            RaidDirection::Received
        } else {
            RaidDirection::Sent
        };

        debug!(
            from_broadcaster_id = %from_broadcaster.id,
            viewer_count,
            direction = direction.as_str(),
            "raid event received"
        );

        let chat_payload = payload::build_raid_chat_payload(event_data, frame_msg_id);
        self.publish_with_chat(
            ChannelRaided {
                direction,
                viewer_count,
                from_broadcaster,
                to_broadcaster,
            },
            chat_payload,
        );
    }

    pub(in crate::chat) fn publish_reward_redemption_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);
        let reward = wire_child(event_data, "reward");
        let reward = RedeemedReward {
            id: wire_text(reward, "id"),
            title: wire_text(reward, "title"),
            cost: wire_count(reward, "cost"),
            prompt: wire_text(reward, "prompt"),
        };

        debug!(user_id = %user.id, reward_id = %reward.id, "channel point reward redemption received");

        self.publish_typed(RewardRedeemed {
            redemption: wire_redemption(event_data),
            user,
            reward,
        });
    }

    pub(in crate::chat) fn publish_message_delete_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let message_id = wire_text(event_data, "message_id");
        let target_user = wire_user(event_data, &TARGET_USER_KEYS);

        debug!(target_user_id = %target_user.id, message_id = %message_id, "chat message deleted");

        self.publish_with_moderation(
            ChatMessageDeleted {
                message_id: message_id.clone(),
                target_user,
            },
            ChatModerationAction::DeleteMessage { message_id },
        );
    }

    pub(in crate::chat) fn publish_chat_clear_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let broadcaster = wire_broadcaster(event_data);

        debug!(broadcaster_id = %broadcaster.id, "chat cleared");

        self.publish_with_moderation(ChatCleared { broadcaster }, ChatModerationAction::ClearChat);
    }

    pub(in crate::chat) fn publish_follow_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, "follow event received");

        self.publish_typed(ChannelFollowed {
            followed_at: wire_text(event_data, "followed_at"),
            user,
        });
    }
}
