use forge_events::Event;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TwitchBadge {
    Moderator,
    Vip,
    Subscriber,
    Broadcaster,
}

#[derive(Debug, Clone)]
pub enum TwitchChatEvent {
    Message {
        username: String,
        text: String,
        badges: Vec<TwitchBadge>,
        color_hex: Option<String>,
    },
    Subscription {
        username: String,
        tier: u8,
        months: Option<u32>,
        message: Option<String>,
        color_hex: Option<String>,
    },
    Cheer {
        username: String,
        bits: u64,
        text: String,
        color_hex: Option<String>,
    },
    Raid {
        username: String,
        viewers: u64,
    },
}

pub fn parse(event: &Event) -> Option<TwitchChatEvent> {
    match event.kind.as_str() {
        "chat.message" => parse_chat_message(event),
        "channel.subscribe" | "channel.subscription.message" => parse_subscription(event),
        "channel.cheer" => parse_cheer(event),
        "channel.raid" => parse_raid(event),
        _ => None,
    }
}

fn parse_chat_message(event: &Event) -> Option<TwitchChatEvent> {
    let payload = event.payload.as_object()?;

    let username = payload
        .get("chatter_user_name")
        .or_else(|| payload.get("username"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_owned();

    let text = payload
        .get("message")
        .and_then(|m| {
            if let Some(obj) = m.as_object() {
                obj.get("text").and_then(|t| t.as_str()).map(str::to_owned)
            } else {
                m.as_str().map(str::to_owned)
            }
        })
        .unwrap_or_default();

    let badges = extract_badges(payload);
    let color_hex = extract_color_hex(payload);

    Some(TwitchChatEvent::Message {
        username,
        text,
        badges,
        color_hex,
    })
}

fn parse_subscription(event: &Event) -> Option<TwitchChatEvent> {
    let payload = event.payload.as_object()?;

    let username = payload
        .get("user_name")
        .or_else(|| payload.get("chatter_user_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_owned();

    let tier = payload
        .get("tier")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u32>().ok())
        .map(|t| {
            if t >= 3000 {
                3u8
            } else if t >= 2000 {
                2u8
            } else {
                1u8
            }
        })
        .unwrap_or(1);

    let months = payload
        .get("cumulative_months")
        .or_else(|| payload.get("duration_months"))
        .and_then(|v| v.as_u64())
        .map(|m| m as u32);

    let message = payload
        .get("message")
        .and_then(|m| {
            if let Some(obj) = m.as_object() {
                obj.get("text").and_then(|t| t.as_str()).map(str::to_owned)
            } else {
                m.as_str().map(str::to_owned)
            }
        })
        .filter(|s| !s.is_empty());

    let color_hex = extract_color_hex(payload);

    Some(TwitchChatEvent::Subscription {
        username,
        tier,
        months,
        message,
        color_hex,
    })
}

fn parse_cheer(event: &Event) -> Option<TwitchChatEvent> {
    let payload = event.payload.as_object()?;

    let username = payload
        .get("user_name")
        .and_then(|v| v.as_str())
        .unwrap_or("anonymous")
        .to_owned();

    let bits = payload.get("bits").and_then(|v| v.as_u64()).unwrap_or(0);

    let text = payload
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();

    let color_hex = extract_color_hex(payload);

    Some(TwitchChatEvent::Cheer {
        username,
        bits,
        text,
        color_hex,
    })
}

fn parse_raid(event: &Event) -> Option<TwitchChatEvent> {
    let payload = event.payload.as_object()?;

    let username = payload
        .get("from_broadcaster_user_name")
        .or_else(|| payload.get("raider_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_owned();

    let viewers = payload.get("viewers").and_then(|v| v.as_u64()).unwrap_or(0);

    Some(TwitchChatEvent::Raid { username, viewers })
}

fn extract_badges(payload: &serde_json::Map<String, serde_json::Value>) -> Vec<TwitchBadge> {
    let Some(badges_val) = payload.get("badges") else {
        return Vec::new();
    };
    let Some(arr) = badges_val.as_array() else {
        return Vec::new();
    };

    arr.iter()
        .filter_map(|b| {
            let set_id = b.as_str().or_else(|| {
                b.as_object()
                    .and_then(|o| o.get("set_id"))
                    .and_then(|v| v.as_str())
            })?;
            match set_id {
                "moderator" => Some(TwitchBadge::Moderator),
                "vip" => Some(TwitchBadge::Vip),
                "subscriber" => Some(TwitchBadge::Subscriber),
                "broadcaster" => Some(TwitchBadge::Broadcaster),
                _ => None,
            }
        })
        .collect()
}

fn extract_color_hex(payload: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    payload
        .get("color")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
