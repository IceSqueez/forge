use super::*;

pub(super) fn twitch_event(payload: impl TypedEvent) -> Event {
    payload.into_event(EventSource::Twitch, None)
}

pub(super) struct WireUserKeys {
    id: &'static str,
    login: &'static str,
    name: &'static str,
}

pub(super) const USER_KEYS: WireUserKeys = WireUserKeys {
    id: "user_id",
    login: "user_login",
    name: "user_name",
};

pub(super) const MODERATOR_KEYS: WireUserKeys = WireUserKeys {
    id: "moderator_user_id",
    login: "moderator_user_login",
    name: "moderator_user_name",
};

pub(super) const TARGET_USER_KEYS: WireUserKeys = WireUserKeys {
    id: "target_user_id",
    login: "target_user_login",
    name: "target_user_name",
};

pub(super) const FROM_BROADCASTER_KEYS: WireUserKeys = WireUserKeys {
    id: "from_broadcaster_user_id",
    login: "from_broadcaster_user_login",
    name: "from_broadcaster_user_name",
};

pub(super) const TO_BROADCASTER_KEYS: WireUserKeys = WireUserKeys {
    id: "to_broadcaster_user_id",
    login: "to_broadcaster_user_login",
    name: "to_broadcaster_user_name",
};

pub(super) const HOST_BROADCASTER_KEYS: WireUserKeys = WireUserKeys {
    id: "host_broadcaster_user_id",
    login: "host_broadcaster_user_login",
    name: "host_broadcaster_user_name",
};

pub(super) const REQUESTER_KEYS: WireUserKeys = WireUserKeys {
    id: "requester_user_id",
    login: "requester_user_login",
    name: "requester_user_name",
};

pub(super) const WHISPER_SENDER_KEYS: WireUserKeys = WireUserKeys {
    id: "from_user_id",
    login: "from_user_login",
    name: "from_user_name",
};

pub(super) const BLOCKED_TERM_REASON: &str = "blocked_term";
pub(super) const WIRE_AUTOMOD_REASON: &str = "reason";
pub(super) const WIRE_BLOCKED_TERM: &str = "blocked_term";
pub(super) const WIRE_TERMS_FOUND: &str = "terms_found";
pub(super) const WIRE_TERM_ID: &str = "term_id";

static WIRE_ABSENT: serde_json::Value = serde_json::Value::Null;

pub(super) fn wire_child<'a>(data: &'a serde_json::Value, key: &str) -> &'a serde_json::Value {
    data.get(key).unwrap_or(&WIRE_ABSENT)
}

pub(super) fn wire_text(data: &serde_json::Value, key: &str) -> String {
    data.get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

pub(super) fn wire_non_empty<'a>(data: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    data.get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
}

pub(super) fn wire_count(data: &serde_json::Value, key: &str) -> i64 {
    wire_optional_count(data, key).unwrap_or_default()
}

pub(super) fn wire_optional_count(data: &serde_json::Value, key: &str) -> Option<i64> {
    data.get(key).and_then(serde_json::Value::as_i64)
}

pub(super) fn wire_flag(data: &serde_json::Value, key: &str) -> bool {
    data.get(key)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or_default()
}

pub(super) fn wire_list(data: &serde_json::Value, key: &str) -> Vec<serde_json::Value> {
    data.get(key)
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default()
}

pub(super) fn wire_user(data: &serde_json::Value, keys: &WireUserKeys) -> TwitchUser {
    TwitchUser {
        id: wire_text(data, keys.id),
        login: wire_text(data, keys.login),
        display_name: wire_text(data, keys.name),
    }
}

pub(super) fn wire_broadcaster(data: &serde_json::Value) -> TwitchBroadcaster {
    TwitchBroadcaster {
        id: wire_text(data, "broadcaster_user_id"),
        login: wire_text(data, "broadcaster_user_login"),
    }
}

pub(super) fn wire_redemption(event_data: &serde_json::Value) -> Redemption {
    Redemption {
        id: wire_text(event_data, "id"),
        status: wire_text(event_data, "status"),
        user_input: wire_text(event_data, "user_input"),
        redeemed_at: wire_text(event_data, "redeemed_at"),
    }
}

pub(super) fn wire_custom_reward(event_data: &serde_json::Value) -> CustomRewardState {
    CustomRewardState {
        reward: CustomReward {
            id: wire_text(event_data, "id"),
            title: wire_text(event_data, "title"),
            cost: wire_count(event_data, "cost"),
            prompt: wire_text(event_data, "prompt"),
            is_enabled: wire_flag(event_data, "is_enabled"),
        },
    }
}

pub(super) fn wire_shared_chat(event_data: &serde_json::Value) -> SharedChatState {
    SharedChatState {
        shared_chat: SharedChatSession {
            session_id: wire_text(event_data, "session_id"),
        },
        host: wire_user(event_data, &HOST_BROADCASTER_KEYS),
    }
}

pub(super) fn wire_charity_campaign(event_data: &serde_json::Value) -> CharityCampaignState {
    let current = wire_child(event_data, "current_amount");
    CharityCampaignState {
        charity: CharityCampaignTotals {
            id: wire_text(event_data, "id"),
            name: wire_text(event_data, "charity_name"),
            current_amount_cents: wire_count(current, "value"),
            target_amount_cents: wire_count(wire_child(event_data, "target_amount"), "value"),
            currency_code: wire_text(current, "currency"),
            decimal_places: wire_count(current, "decimal_places"),
        },
    }
}

pub(super) struct AutomodClassification {
    pub(super) category: String,
    pub(super) level: i64,
    pub(super) is_blocked_term: bool,
    pub(super) terms_found: Vec<String>,
}

impl AutomodClassification {
    pub(super) fn category_unless_blocked_term(&self) -> Option<String> {
        (!self.is_blocked_term).then(|| self.category.clone())
    }

    pub(super) fn level_unless_blocked_term(&self) -> Option<i64> {
        (!self.is_blocked_term).then_some(self.level)
    }

    pub(super) fn blocked_term_match(&self) -> Option<BlockedTermMatch> {
        self.is_blocked_term.then(|| BlockedTermMatch {
            terms_found: self.terms_found.clone(),
        })
    }
}

pub(super) fn wire_automod_classification(
    event_data: &serde_json::Value,
    reason: &str,
) -> AutomodClassification {
    let automod = wire_child(event_data, "automod");
    AutomodClassification {
        category: wire_text(automod, "category"),
        level: wire_count(automod, "level"),
        is_blocked_term: reason == BLOCKED_TERM_REASON,
        terms_found: blocked_terms_found(event_data),
    }
}

pub(super) fn extract_poll_choices(event_data: &serde_json::Value) -> Vec<PollChoice> {
    event_data
        .get("choices")
        .and_then(serde_json::Value::as_array)
        .map(|choices| {
            choices
                .iter()
                .map(|choice| PollChoice {
                    id: wire_text(choice, "id"),
                    title: wire_text(choice, "title"),
                    votes: wire_optional_count(choice, "votes"),
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn extract_prediction_outcomes(
    event_data: &serde_json::Value,
) -> Vec<PredictionOutcome> {
    event_data
        .get("outcomes")
        .and_then(serde_json::Value::as_array)
        .map(|outcomes| {
            outcomes
                .iter()
                .map(|outcome| PredictionOutcome {
                    id: wire_text(outcome, "id"),
                    title: wire_text(outcome, "title"),
                    color: wire_text(outcome, "color"),
                    users: wire_optional_count(outcome, "users"),
                    channel_points: wire_optional_count(outcome, "channel_points"),
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn blocked_terms_found(event_data: &serde_json::Value) -> Vec<String> {
    event_data
        .get(WIRE_BLOCKED_TERM)
        .and_then(|b| b.get(WIRE_TERMS_FOUND))
        .and_then(|v| v.as_array())
        .map(|terms| {
            terms
                .iter()
                .filter_map(|t| t.get(WIRE_TERM_ID))
                .filter_map(|v| v.as_str())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn attach_chat_payload(forge_payload: &mut serde_json::Value, chat: ChatPayload) {
    match serde_json::to_value(&chat) {
        Ok(chat_value) => {
            if let serde_json::Value::Object(map) = forge_payload {
                map.insert(ChatPayload::KEY.to_owned(), chat_value);
            }
        }
        Err(e) => {
            warn!(error = %e, "failed to serialize ChatPayload; _chat key omitted");
        }
    }
}

pub(super) fn attach_moderation_payload(
    forge_payload: &mut serde_json::Value,
    action: ChatModerationAction,
) {
    match serde_json::to_value(&ChatModerationPayload { action }) {
        Ok(mod_value) => {
            if let serde_json::Value::Object(map) = forge_payload {
                map.insert(ChatModerationPayload::KEY.to_owned(), mod_value);
            }
        }
        Err(e) => {
            warn!(error = %e, "failed to serialize ChatModerationPayload; _chat_mod key omitted");
        }
    }
}

pub(super) fn attach_chat_reply_payload(forge_payload: &mut serde_json::Value, reply: ChatReply) {
    match serde_json::to_value(&reply) {
        Ok(reply_value) => {
            if let serde_json::Value::Object(map) = forge_payload {
                map.insert(ChatReply::KEY.to_owned(), reply_value);
            }
        }
        Err(e) => {
            warn!(error = %e, "failed to serialize ChatReply; _chat_reply key omitted");
        }
    }
}

pub(super) fn extract_roles_from_badges(badges: Option<&serde_json::Value>) -> Vec<String> {
    badges
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|b| b.get("set_id").and_then(|s| s.as_str()))
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
