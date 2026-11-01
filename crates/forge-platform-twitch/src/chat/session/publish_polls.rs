use super::*;

impl ChatSession {
    pub(in crate::chat) fn publish_poll_begin_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let poll = PollOpening {
            id: wire_text(event_data, "id"),
            title: wire_text(event_data, "title"),
            started_at: wire_text(event_data, "started_at"),
            ends_at: wire_text(event_data, "ends_at"),
        };
        let choices = extract_poll_choices(event_data);

        debug!(poll_id = %poll.id, choices = choices.len(), "poll began");

        self.publish_typed(PollBegan { poll, choices });
    }

    pub(in crate::chat) fn publish_poll_progress_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let poll = PollHeading {
            id: wire_text(event_data, "id"),
            title: wire_text(event_data, "title"),
        };
        let choices = extract_poll_choices(event_data);

        debug!(poll_id = %poll.id, "poll progress");

        self.publish_typed(PollProgressed { poll, choices });
    }

    pub(in crate::chat) fn publish_poll_end_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let poll = PollClosing {
            id: wire_text(event_data, "id"),
            title: wire_text(event_data, "title"),
            status: wire_text(event_data, "status"),
            ended_at: wire_text(event_data, "ended_at"),
        };
        let choices = extract_poll_choices(event_data);

        debug!(poll_id = %poll.id, status = %poll.status, "poll ended");

        self.publish_typed(PollEnded { poll, choices });
    }

    pub(in crate::chat) fn publish_prediction_begin_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let prediction = PredictionOpening {
            id: wire_text(event_data, "id"),
            title: wire_text(event_data, "title"),
            started_at: wire_text(event_data, "started_at"),
            locks_at: wire_text(event_data, "locks_at"),
        };
        let outcomes = extract_prediction_outcomes(event_data);

        debug!(prediction_id = %prediction.id, outcomes = outcomes.len(), "prediction began");

        self.publish_typed(PredictionBegan {
            prediction,
            outcomes,
        });
    }

    pub(in crate::chat) fn publish_prediction_progress_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let prediction = PredictionHeading {
            id: wire_text(event_data, "id"),
            title: wire_text(event_data, "title"),
        };
        let outcomes = extract_prediction_outcomes(event_data);

        debug!(prediction_id = %prediction.id, "prediction progress");

        self.publish_typed(PredictionProgressed {
            prediction,
            outcomes,
        });
    }

    pub(in crate::chat) fn publish_prediction_lock_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let prediction = PredictionLock {
            id: wire_text(event_data, "id"),
            title: wire_text(event_data, "title"),
            locked_at: wire_text(event_data, "locked_at"),
        };

        debug!(prediction_id = %prediction.id, "prediction locked");

        self.publish_typed(PredictionLocked { prediction });
    }

    pub(in crate::chat) fn publish_prediction_end_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let prediction = PredictionResolution {
            id: wire_text(event_data, "id"),
            title: wire_text(event_data, "title"),
            winning_outcome_id: wire_text(event_data, "winning_outcome_id"),
            status: wire_text(event_data, "status"),
            ended_at: wire_text(event_data, "ended_at"),
        };

        debug!(prediction_id = %prediction.id, status = %prediction.status, "prediction ended");

        self.publish_typed(PredictionEnded { prediction });
    }

    pub(in crate::chat) fn publish_goal_begin_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let goal = GoalOpening {
            id: wire_text(event_data, "id"),
            goal_type: wire_text(event_data, "type"),
            description: wire_text(event_data, "description"),
            current_amount: wire_count(event_data, "current_amount"),
            target_amount: wire_count(event_data, "target_amount"),
            started_at: wire_text(event_data, "started_at"),
        };

        debug!(goal_id = %goal.id, goal_type = %goal.goal_type, "goal begun");

        self.publish_typed(GoalBegan { goal });
    }

    pub(in crate::chat) fn publish_goal_progress_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        self.config
            .bus
            .publish(goal_progress_event(event_data, GoalProgressOrigin::Live));
    }

    pub(in crate::chat) fn publish_goal_end_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let goal = GoalClosing {
            id: wire_text(event_data, "id"),
            goal_type: wire_text(event_data, "type"),
            current_amount: wire_count(event_data, "current_amount"),
            target_amount: wire_count(event_data, "target_amount"),
            is_achieved: wire_flag(event_data, "is_achieved"),
            ended_at: wire_text(event_data, "ended_at"),
        };

        debug!(goal_id = %goal.id, is_achieved = goal.is_achieved, "goal ended");

        self.publish_typed(GoalEnded { goal });
    }
}
