use super::*;

impl YouTubeChatPoller {
    pub(super) fn build_event(
        &self,
        item: &serde_json::Value,
        dedup: &mut DedupSet,
    ) -> Option<Event> {
        let id = item
            .get("id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)?;

        if !dedup.try_insert(id.clone()) {
            return None;
        }

        let snippet = item.get("snippet")?;
        let author_details = item.get("authorDetails");
        let msg_type = snippet.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match msg_type {
            "textMessageEvent" => {
                events::text_message_event(id, &self.channel_id, snippet, author_details)
            }
            "superChatEvent" => events::super_chat_event(id.clone(), snippet, author_details)
                .or_else(|| {
                    events::text_message_event(id, &self.channel_id, snippet, author_details)
                }),
            "superStickerEvent" => events::super_sticker_event(id.clone(), snippet, author_details)
                .or_else(|| {
                    events::text_message_event(id, &self.channel_id, snippet, author_details)
                }),
            "newSponsorEvent" => events::new_sponsor_event(id, snippet, author_details),
            "memberMilestoneChatEvent" => {
                events::member_milestone_event(id, snippet, author_details)
            }
            USER_BANNED_EVENT_TYPE => events::user_banned_event(id, snippet, author_details),
            "tombstone" => events::tombstone_event(id),
            "membershipGiftingEvent" => {
                events::membership_gifting_event(id, snippet, author_details)
            }
            "giftMembershipReceivedEvent" => {
                events::gift_membership_received_event(id, snippet, author_details)
            }
            _ => None,
        }
    }
}

pub(super) fn text_message_event(
    id: String,
    channel_id: &str,
    snippet: &serde_json::Value,
    author_details: Option<&serde_json::Value>,
) -> Option<Event> {
    let text = snippet
        .get("displayMessage")
        .and_then(|v| v.as_str())
        .or_else(|| {
            snippet
                .get("textMessageDetails")
                .and_then(|d| d.get("messageText"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("")
        .to_owned();

    let author = extract_author(author_details);
    let badges = extract_badges(author_details);

    let chat_payload = ChatPayload {
        platform_msg_id: id.clone(),
        author: author.clone(),
        author_color: None,
        segments: vec![ChatSegment::Text { text: text.clone() }],
        badges,
        is_event: false,
        event_detail: None,
        moderation: ModerationMarks::default(),
    };

    let mut payload = serde_json::json!({
        (chat_fields::MESSAGE_ID): id,
        (chat_fields::MESSAGE_TEXT): text,
        (chat_fields::AUTHOR): author_json(author_details),
        (chat_fields::AUTHOR_AVATAR_URL): extract_author_avatar_url(author_details),
        (chat_fields::BROADCASTER_CHANNEL_ID): channel_id,
    });

    payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
    if let Some(channel_id) = extract_author_channel_id(author_details) {
        ChatViewer::new(channel_id, author).attach(&mut payload);
    }
    Some(Event::new(
        EventSource::YouTube,
        event_kinds::CHAT_MESSAGE,
        payload,
    ))
}

pub(super) fn super_chat_event(
    id: String,
    snippet: &serde_json::Value,
    author_details: Option<&serde_json::Value>,
) -> Option<Event> {
    let details = snippet.get("superChatDetails");
    let amount = support_amount(details)?;
    let message = details
        .and_then(|d| d.get("userComment"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);

    let author = extract_author(author_details);
    let badges = extract_badges(author_details);
    let event_detail = ChatEventDetail::SuperChat {
        amount: amount.clone(),
        message: message.clone(),
    };

    let segments = message
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|t| vec![ChatSegment::Text { text: t.to_owned() }])
        .unwrap_or_default();

    let chat_payload = ChatPayload {
        platform_msg_id: id.clone(),
        author: author.clone(),
        author_color: None,
        segments,
        badges,
        is_event: true,
        event_detail: Some(event_detail),
        moderation: ModerationMarks::default(),
    };

    let mut payload = serde_json::json!({
        (chat_fields::AUTHOR): author_json(author_details),
        (support_fields::AMOUNT_MICROS): amount.micros(),
        (support_fields::CURRENCY): amount.currency().as_str(),
        (chat_fields::MESSAGE_TEXT): message,
    });
    payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
    Some(Event::new(
        EventSource::YouTube,
        event_kinds::CHAT_SUPER_CHAT,
        payload,
    ))
}

pub(super) fn super_sticker_event(
    id: String,
    snippet: &serde_json::Value,
    author_details: Option<&serde_json::Value>,
) -> Option<Event> {
    let details = snippet.get("superStickerDetails");
    let amount = support_amount(details)?;
    let sticker_id = details
        .and_then(|d| d.get("superStickerMetadata"))
        .and_then(|m| m.get("stickerId"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();

    let author = extract_author(author_details);
    let badges = extract_badges(author_details);
    let event_detail = ChatEventDetail::SuperChat {
        amount: amount.clone(),
        message: None,
    };

    let chat_payload = ChatPayload {
        platform_msg_id: id.clone(),
        author: author.clone(),
        author_color: None,
        segments: vec![],
        badges,
        is_event: true,
        event_detail: Some(event_detail),
        moderation: ModerationMarks::default(),
    };

    let mut payload = serde_json::json!({
        (chat_fields::AUTHOR): author_json(author_details),
        (support_fields::STICKER_ID): sticker_id,
        (support_fields::AMOUNT_MICROS): amount.micros(),
        (support_fields::CURRENCY): amount.currency().as_str(),
    });
    payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
    Some(Event::new(
        EventSource::YouTube,
        event_kinds::CHAT_SUPER_STICKER,
        payload,
    ))
}

pub(super) fn new_sponsor_event(
    id: String,
    snippet: &serde_json::Value,
    author_details: Option<&serde_json::Value>,
) -> Option<Event> {
    let level = snippet
        .get("newSponsorDetails")
        .and_then(|d| d.get("memberLevelName"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();

    let author = extract_author(author_details);
    let badges = extract_badges(author_details);
    let event_detail = ChatEventDetail::NewMember {
        level: level.clone(),
    };

    let chat_payload = ChatPayload {
        platform_msg_id: id.clone(),
        author: author.clone(),
        author_color: None,
        segments: vec![],
        badges,
        is_event: true,
        event_detail: Some(event_detail),
        moderation: ModerationMarks::default(),
    };

    let mut payload = serde_json::json!({
        (chat_fields::AUTHOR): author_json(author_details),
        (member_fields::MEMBER_LEVEL_NAME): level,
    });
    payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
    Some(Event::new(
        EventSource::YouTube,
        event_kinds::CHANNEL_MEMBER,
        payload,
    ))
}

pub(super) fn member_milestone_event(
    id: String,
    snippet: &serde_json::Value,
    author_details: Option<&serde_json::Value>,
) -> Option<Event> {
    let details = snippet.get("memberMilestoneChatDetails");
    let months = u32::try_from(
        details
            .and_then(|d| d.get("memberMonth"))
            .and_then(|v| {
                v.as_u64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            })
            .unwrap_or(0),
    )
    .unwrap_or(u32::MAX);
    let message = details
        .and_then(|d| d.get("userComment"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);

    let author = extract_author(author_details);
    let badges = extract_badges(author_details);
    let event_detail = ChatEventDetail::MemberMilestone {
        months,
        message: message.clone(),
    };

    let segments = message
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|t| vec![ChatSegment::Text { text: t.to_owned() }])
        .unwrap_or_default();

    let chat_payload = ChatPayload {
        platform_msg_id: id.clone(),
        author: author.clone(),
        author_color: None,
        segments,
        badges,
        is_event: true,
        event_detail: Some(event_detail),
        moderation: ModerationMarks::default(),
    };

    let mut payload = serde_json::json!({
        (chat_fields::AUTHOR): author_json(author_details),
        (member_fields::MEMBER_MONTH): months,
        (chat_fields::MESSAGE_TEXT): message,
    });
    payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
    Some(Event::new(
        EventSource::YouTube,
        event_kinds::CHANNEL_MEMBER_MILESTONE,
        payload,
    ))
}

pub(super) fn user_banned_event(
    id: String,
    snippet: &serde_json::Value,
    author_details: Option<&serde_json::Value>,
) -> Option<Event> {
    let banned_details = snippet.get("userBannedDetails");
    let banned_user = banned_details.and_then(|d| d.get("bannedUserDetails"));
    let target_display_name = non_empty(
        banned_user
            .and_then(|u| u.get("displayName"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned(),
    );
    let target_channel_id = banned_user
        .and_then(|u| u.get("channelId"))
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .filter(|s| !s.is_empty());

    let ban_type = banned_details
        .and_then(|d| d.get("banType"))
        .and_then(|v| v.as_str())
        .unwrap_or("permanent")
        .to_owned();
    let is_temporary = ban_type.eq_ignore_ascii_case("temporary");
    let ban_duration_secs: Option<i64> = if is_temporary {
        banned_details
            .and_then(|d| d.get("banDurationSeconds"))
            .and_then(|v| {
                v.as_u64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            })
            .map(|v| i64::try_from(v).unwrap_or(i64::MAX))
    } else {
        None
    };

    let chat_payload = ChatPayload {
        platform_msg_id: id.clone(),
        author: target_display_name.clone().unwrap_or_default(),
        author_color: None,
        segments: vec![],
        badges: vec![],
        is_event: true,
        event_detail: None,
        moderation: ModerationMarks {
            timed_out: is_temporary,
            banned: !is_temporary,
            deleted: false,
        },
    };

    let mut payload = serde_json::json!({
        (ban_fields::TARGET_USER): entity_json(target_channel_id.clone(), target_display_name.clone()),
        (ban_fields::MODERATOR): author_json(author_details),
        (ban_fields::TYPE): ban_type,
        (ban_fields::DURATION_SECONDS): ban_duration_secs,
    });
    payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
    ChatModerationPayload {
        action: ChatModerationAction::RemoveUser {
            user_id: target_channel_id,
            user_name: target_display_name.unwrap_or_default(),
            removal: if is_temporary {
                UserRemoval::Timeout
            } else {
                UserRemoval::Ban
            },
        },
    }
    .attach(&mut payload);
    Some(Event::new(
        EventSource::YouTube,
        event_kinds::CHANNEL_USER_BANNED,
        payload,
    ))
}

pub(super) fn tombstone_event(id: String) -> Option<Event> {
    let chat_payload = ChatPayload {
        platform_msg_id: id.clone(),
        author: String::new(),
        author_color: None,
        segments: vec![],
        badges: vec![],
        is_event: true,
        event_detail: None,
        moderation: ModerationMarks {
            timed_out: false,
            banned: false,
            deleted: true,
        },
    };

    let mut payload = serde_json::json!({
        (chat_mod_fields::MESSAGE_ID): id,
    });
    payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
    ChatModerationPayload {
        action: ChatModerationAction::DeleteMessage {
            message_id: id.clone(),
        },
    }
    .attach(&mut payload);
    Some(Event::new(
        EventSource::YouTube,
        event_kinds::CHAT_MESSAGE_DELETED,
        payload,
    ))
}

pub(super) fn membership_gifting_event(
    id: String,
    snippet: &serde_json::Value,
    author_details: Option<&serde_json::Value>,
) -> Option<Event> {
    let details = snippet.get("membershipGiftingDetails");
    let count = details
        .and_then(|d| d.get("giftMembershipsCount"))
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(0);
    let level_name = details
        .and_then(|d| d.get("giftMembershipsLevelName"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();

    let gifter_display_name = extract_author(author_details);

    let chat_payload = ChatPayload {
        platform_msg_id: id.clone(),
        author: gifter_display_name.clone(),
        author_color: None,
        segments: vec![],
        badges: extract_badges(author_details),
        is_event: true,
        event_detail: None,
        moderation: ModerationMarks::default(),
    };

    let mut payload = serde_json::json!({
        (gift_fields::COUNT): count,
        (gift_fields::LEVEL_NAME): level_name,
        (gift_fields::GIFTER): author_json(author_details),
    });
    payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
    Some(Event::new(
        EventSource::YouTube,
        event_kinds::CHANNEL_MEMBER_GIFT,
        payload,
    ))
}

pub(super) fn gift_membership_received_event(
    id: String,
    snippet: &serde_json::Value,
    author_details: Option<&serde_json::Value>,
) -> Option<Event> {
    let details = snippet.get("giftMembershipReceivedDetails");
    let level_name = details
        .and_then(|d| d.get("memberLevelName"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let gifter_channel_id = details
        .and_then(|d| d.get("gifterChannelId"))
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .filter(|s| !s.is_empty());

    let recipient_display_name = extract_author(author_details);

    let chat_payload = ChatPayload {
        platform_msg_id: id.clone(),
        author: recipient_display_name.clone(),
        author_color: None,
        segments: vec![],
        badges: extract_badges(author_details),
        is_event: true,
        event_detail: None,
        moderation: ModerationMarks::default(),
    };

    let mut payload = serde_json::json!({
        (gift_fields::LEVEL_NAME): level_name,
        (gift_fields::GIFTER): entity_json(gifter_channel_id, None),
        (gift_fields::RECIPIENT): author_json(author_details),
    });
    payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
    Some(Event::new(
        EventSource::YouTube,
        event_kinds::CHANNEL_MEMBER_GIFT_RECEIVED,
        payload,
    ))
}

fn support_amount(details: Option<&serde_json::Value>) -> Option<MoneyAmount> {
    let micros = details
        .and_then(|d| d.get("amountMicros"))
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(0);
    let code = details
        .and_then(|d| d.get("currency"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    match CurrencyCode::parse(code) {
        Ok(currency) => Some(MoneyAmount::from_micros(micros, currency)),
        Err(_) => {
            tracing::warn!(
                currency = code,
                "support event has an unusable currency code; delivering it as a plain chat line"
            );
            None
        }
    }
}
