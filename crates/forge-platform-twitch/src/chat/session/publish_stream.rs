use super::*;

impl ChatSession {
    pub(in crate::chat) fn publish_stream_online_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let broadcaster = wire_broadcaster(event_data);

        debug!(broadcaster_id = %broadcaster.id, "stream online event received");

        self.publish_typed(StreamWentOnline {
            stream: LiveStream {
                id: wire_text(event_data, "id"),
                stream_type: wire_text(event_data, "type"),
                started_at: wire_text(event_data, "started_at"),
            },
            broadcaster,
        });
    }

    pub(in crate::chat) fn publish_stream_offline_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let broadcaster = wire_broadcaster(event_data);

        debug!(broadcaster_id = %broadcaster.id, "stream offline event received");

        self.publish_typed(StreamWentOffline { broadcaster });
    }

    pub(in crate::chat) fn publish_hype_train_begin_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let hype = HypeTrainOpening {
            id: wire_text(event_data, "id"),
            level: wire_count(event_data, "level"),
            total: wire_count(event_data, "total"),
            goal: wire_count(event_data, "goal"),
            progress: wire_count(event_data, "progress"),
            started_at: wire_text(event_data, "started_at"),
            expires_at: wire_text(event_data, "expires_at"),
        };

        debug!(hype_id = %hype.id, level = hype.level, total = hype.total, "hype train began");

        self.publish_typed(HypeTrainBegan { hype });
    }

    pub(in crate::chat) fn publish_hype_train_progress_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let hype = HypeTrainProgress {
            id: wire_text(event_data, "id"),
            level: wire_count(event_data, "level"),
            total: wire_count(event_data, "total"),
            goal: wire_count(event_data, "goal"),
            progress: wire_count(event_data, "progress"),
        };

        debug!(hype_id = %hype.id, level = hype.level, progress = hype.progress, "hype train progressed");

        self.publish_typed(HypeTrainProgressed { hype });
    }

    pub(in crate::chat) fn publish_hype_train_end_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let hype = HypeTrainClosing {
            id: wire_text(event_data, "id"),
            level: wire_count(event_data, "level"),
            total: wire_count(event_data, "total"),
            ended_at: wire_text(event_data, "ended_at"),
            cooldown_ends_at: wire_text(event_data, "cooldown_ends_at"),
        };

        debug!(hype_id = %hype.id, level = hype.level, total = hype.total, "hype train ended");

        self.publish_typed(HypeTrainEnded { hype });
    }

    pub(in crate::chat) fn publish_charity_donation_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let amount = wire_child(event_data, "amount");
        let charity = CharityDonation {
            id: wire_text(event_data, "campaign_id"),
            name: wire_text(event_data, "charity_name"),
            description: wire_text(event_data, "charity_description"),
            website: wire_text(event_data, "charity_website"),
            amount_cents: wire_count(amount, "value"),
            currency_code: wire_text(amount, "currency"),
            decimal_places: wire_count(amount, "decimal_places"),
        };
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, amount_cents = charity.amount_cents, "charity donation received");

        self.publish_typed(CharityDonated { charity, user });
    }

    pub(in crate::chat) fn publish_charity_start_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        self.publish_typed(CharityCampaignStarted(wire_charity_campaign(event_data)));
    }

    pub(in crate::chat) fn publish_charity_progress_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        self.publish_typed(CharityCampaignProgressed(wire_charity_campaign(event_data)));
    }

    pub(in crate::chat) fn publish_charity_stop_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        self.publish_typed(CharityCampaignStopped(wire_charity_campaign(event_data)));
    }
}
