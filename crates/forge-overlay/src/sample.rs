use serde_json::{Value, json};

const SAMPLE_CHANNEL: &str = "forge_demo";
const SAMPLE_TIME: &str = "2026-07-29T18:24:05Z";

enum Family {
    ChatMessage,
    Follow,
    Subscription,
    Cheer,
    Raid,
    Plain,
}

/// Mirrors the real payload shape of the kind's family, entities nested exactly as a live
/// event nests them, so a test renders what a real fire renders instead of a friendlier lie.
pub fn sample_payload(event_kind: &str) -> Value {
    match family(event_kind) {
        Family::ChatMessage => json!({
            "channel": SAMPLE_CHANNEL,
            "user": viewer(),
            "message": "first time here, hi!",
        }),
        Family::Follow => json!({
            "user": viewer(),
            "followed_at": SAMPLE_TIME,
        }),
        Family::Subscription => json!({
            "user": viewer(),
            "tier": "1000",
            "is_gift": false,
            "cumulative_months": 7,
            "streak_months": 3,
            "message": "love the content",
        }),
        Family::Cheer => json!({
            "user": viewer(),
            "bits": 500,
            "message": "take my bits",
        }),
        Family::Raid => json!({
            "direction": "incoming",
            "viewer_count": 42,
            "from_broadcaster": broadcaster(),
            "to_broadcaster": null,
        }),
        Family::Plain => json!({
            "user": viewer(),
            "message": "sample payload",
        }),
    }
}

fn family(event_kind: &str) -> Family {
    if event_kind.contains("chat.message") {
        Family::ChatMessage
    } else if event_kind.contains("follow") {
        Family::Follow
    } else if event_kind.contains("subscribe") || event_kind.contains("subscription") {
        Family::Subscription
    } else if event_kind.contains("cheer") {
        Family::Cheer
    } else if event_kind.contains("raid") {
        Family::Raid
    } else {
        Family::Plain
    }
}

fn viewer() -> Value {
    json!({ "id": "104857392", "login": "pixel_pal", "display_name": "PixelPal" })
}

fn broadcaster() -> Value {
    json!({ "id": "551209874", "login": "night_owl", "display_name": "NightOwl" })
}
