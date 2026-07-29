use forge_types::{ArgStack, Variant};
use serde_json::{Value, json};

use crate::content::delivered_content;
use crate::descriptor::{OverlayConfig, OverlayKindDescriptor};

const SAMPLE_CHANNEL: &str = "forge_demo";
const SAMPLE_TIME: &str = "2026-07-29T18:24:05Z";

enum Family {
    ChatMessage,
    Follow,
    Subscribe,
    Resubscribe,
    GiftSubscription,
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
        Family::Subscribe => json!({
            "user": viewer(),
            "tier": "1000",
            "is_gift": false,
        }),
        Family::Resubscribe => json!({
            "user": viewer(),
            "tier": "1000",
            "cumulative_months": 7,
            "streak_months": 3,
            "message": "love the content",
            "share_streak": true,
        }),
        Family::GiftSubscription => json!({
            "tier": "1000",
            "is_anonymous": false,
            "gifter": viewer(),
            "recipient": { "id": null, "login": null, "display_name": null },
        }),
        Family::Cheer => json!({
            "user": viewer(),
            "bits": 500,
            "message": "take my bits",
            "is_anonymous": false,
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

/// The content group a step would supply, with the overlay's own wording expanded against a
/// sample variable context, so a test renders through the same path a real delivery takes.
pub fn sample_content(
    descriptor: &dyn OverlayKindDescriptor,
    stored: &OverlayConfig,
) -> OverlayConfig {
    delivered_content(
        descriptor,
        stored,
        &OverlayConfig::new(),
        &sample_args(descriptor.id()),
    )
}

fn sample_args(event_kind: &str) -> ArgStack {
    let Value::Object(fields) = sample_payload(event_kind) else {
        return ArgStack::new();
    };
    fields
        .into_iter()
        .filter_map(|(name, value)| Variant::from_json(value).ok().map(|held| (name, held)))
        .fold(ArgStack::new(), |args, (name, value)| args.set(name, value))
}

fn family(event_kind: &str) -> Family {
    if event_kind.contains("chat.message") {
        Family::ChatMessage
    } else if event_kind.contains("follow") {
        Family::Follow
    } else if event_kind.contains("subscription.gift") || event_kind.contains("gifts") {
        Family::GiftSubscription
    } else if event_kind.contains("subscription.message") {
        Family::Resubscribe
    } else if event_kind.contains("subscribe") || event_kind.contains("subscription") {
        Family::Subscribe
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
