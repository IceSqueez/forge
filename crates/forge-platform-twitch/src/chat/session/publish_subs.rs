use super::*;
use crate::chat::sub_notice_pairing::{ReleasedSub, SubNoticeKey, SubNoticeKind};
use crate::vocabulary::topics;

const NOTICE_TYPE_SUB: &str = "sub";
const NOTICE_TYPE_RESUB: &str = "resub";

pub(super) enum HeldSub {
    Subscribed {
        event: ChannelSubscribed,
        chat: ChatPayload,
    },
    Resubscribed {
        event: ChannelResubscribed,
        chat: ChatPayload,
    },
}

impl ChatSession {
    pub(in crate::chat) fn publish_subscribe_event(
        &self,
        event_data: &serde_json::Value,
        frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);
        let is_gift = wire_flag(event_data, "is_gift");

        debug!(user_id = %user.id, is_gift, "subscriber event received");

        let key = SubNoticeKey::new(SubNoticeKind::Sub, user.id.clone());
        let held = HeldSub::Subscribed {
            event: ChannelSubscribed {
                user,
                tier: wire_text(event_data, "tier"),
                is_gift,
                is_prime: false,
            },
            chat: payload::build_subscribe_chat_payload(event_data, frame_msg_id),
        };
        if is_gift {
            self.release_sub(ReleasedSub::without_notice(held));
        } else {
            self.hold_for_notice(key, held);
        }
    }

    pub(in crate::chat) fn publish_resubscribe_event(
        &self,
        event_data: &serde_json::Value,
        frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);
        let cumulative_months = wire_count(event_data, "cumulative_months");

        debug!(user_id = %user.id, cumulative_months, "resub event received");

        let key = SubNoticeKey::new(SubNoticeKind::Resub, user.id.clone());
        let held = HeldSub::Resubscribed {
            event: ChannelResubscribed {
                user,
                tier: wire_text(event_data, "tier"),
                cumulative_months,
                streak_months: wire_count(event_data, "streak_months"),
                message: wire_text(wire_child(event_data, "message"), "text"),
                share_streak: wire_flag(event_data, "share_streak"),
                is_prime: false,
            },
            chat: payload::build_resubscribe_chat_payload(event_data, frame_msg_id),
        };
        self.hold_for_notice(key, held);
    }

    pub(in crate::chat) fn pair_chat_notification(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        if wire_non_empty(event_data, "source_broadcaster_user_id").is_some() {
            return;
        }
        let notice_type = wire_text(event_data, "notice_type");
        let kind = match notice_type.as_str() {
            NOTICE_TYPE_SUB => SubNoticeKind::Sub,
            NOTICE_TYPE_RESUB => SubNoticeKind::Resub,
            _ => return,
        };
        let Some(chatter_id) = wire_non_empty(event_data, "chatter_user_id") else {
            return;
        };
        let is_prime = wire_flag(wire_child(event_data, &notice_type), "is_prime");

        debug!(user_id = %chatter_id, notice_type = %notice_type, is_prime, "sub chat notification received");

        let released = self.sub_notices.lock_recover().offer_notice(
            SubNoticeKey::new(kind, chatter_id),
            is_prime,
            Instant::now(),
        );
        if let Some(released) = released {
            self.release_sub(released);
        }
    }

    pub(super) fn next_sub_release(&self) -> Option<Instant> {
        self.sub_notices.lock_recover().next_deadline()
    }

    pub(super) fn release_expired_subs(&self) {
        let due = self
            .sub_notices
            .lock_recover()
            .release_expired(Instant::now());
        for released in due {
            self.release_sub(released);
        }
    }

    pub(super) fn release_waiting_subs(&self) {
        let waiting = self.sub_notices.lock_recover().release_all();
        for released in waiting {
            self.release_sub(released);
        }
    }

    fn hold_for_notice(&self, key: SubNoticeKey, held: HeldSub) {
        let now = Instant::now();
        let notice_topic_live = self
            .config
            .tracker
            .is_active(topics::CHANNEL_CHAT_NOTIFICATION);
        let released = {
            let mut pairing = self.sub_notices.lock_recover();
            match pairing.take_notice(&key, now) {
                Some(is_prime) => Some(ReleasedSub { held, is_prime }),
                None if notice_topic_live => pairing.wait_evicting_oldest(key, held, now),
                None => Some(ReleasedSub::without_notice(held)),
            }
        };
        if let Some(released) = released {
            self.release_sub(released);
        }
    }

    fn release_sub(&self, released: ReleasedSub<HeldSub>) {
        match released.held {
            HeldSub::Subscribed { mut event, chat } => {
                event.is_prime = released.is_prime;
                self.publish_with_chat(event, chat);
            }
            HeldSub::Resubscribed { mut event, chat } => {
                event.is_prime = released.is_prime;
                self.publish_with_chat(event, chat);
            }
        }
    }
}

pub(super) async fn sleep_until_due(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}
