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

/// Chat/resub/cheer families mirror the flat variable names the matching trigger's
/// `output_schema` declares (so a stored `%token%` resolves the same way it would against a
/// live `ArgStack`); the rest still mirror the raw wire payload shape.
pub fn sample_payload(event_kind: &str) -> Value {
    match family(event_kind) {
        Family::ChatMessage => json!({
            "message_text": "first time here, hi!",
            "user_login": "pixel_pal",
            "user_id": "104857392",
            "channel": SAMPLE_CHANNEL,
            "user_color": "#f38ba8",
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
            "user_login": "pixel_pal",
            "user_id": "104857392",
            "sub_tier": "1000",
            "sub_cumulative_months": 7,
            "sub_streak_months": 3,
            "sub_message": "love the content",
        }),
        Family::GiftSubscription => json!({
            "tier": "1000",
            "is_anonymous": false,
            "gifter": viewer(),
            "recipient": { "id": null, "login": null, "display_name": null },
        }),
        Family::Cheer => json!({
            "bits_amount": 500,
            "cheer_message": "take my bits",
            "cheer_is_anonymous": false,
            "user_login": "pixel_pal",
            "user_id": "104857392",
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
    match event_kind {
        "overlay.alert" => return Family::Resubscribe,
        "overlay.ticker" => return Family::Cheer,
        "overlay.chat" => return Family::ChatMessage,
        _ => {}
    }
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
