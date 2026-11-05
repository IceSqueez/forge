use forge_events::Event;
use forge_types::{ChatPayload, DeclaredVariable, UserBadge, VariantKind};

pub const USER_BADGES_VARIABLE: &str = "user_badges";
pub const USER_BADGE_IMAGES_VARIABLE: &str = "user_badge_images";
pub const USER_COLOR_VARIABLE: &str = "user_color";
pub const USER_AVATAR_VARIABLE: &str = "user_avatar";

const BADGE_SEPARATOR: &str = " ";
const EMPTY_IMAGE_LIST: &str = "[]";

pub(crate) fn user_badges_declaration() -> DeclaredVariable {
    text_declaration(USER_BADGES_VARIABLE)
}

pub(crate) fn user_badge_images_declaration() -> DeclaredVariable {
    text_declaration(USER_BADGE_IMAGES_VARIABLE)
}

pub(crate) fn user_color_declaration() -> DeclaredVariable {
    text_declaration(USER_COLOR_VARIABLE)
}

pub(crate) fn user_avatar_declaration() -> DeclaredVariable {
    text_declaration(USER_AVATAR_VARIABLE)
}

fn text_declaration(name: &str) -> DeclaredVariable {
    DeclaredVariable {
        name: name.to_owned(),
        kind: VariantKind::String,
        synthesis: None,
    }
}

fn chat_payload(event: &Event) -> Option<ChatPayload> {
    let envelope = event.payload.get(ChatPayload::KEY)?;
    serde_json::from_value(envelope.clone()).ok()
}

pub(crate) fn chat_badges_text(event: &Event) -> String {
    chat_payload(event)
        .map(|chat| {
            chat.badges
                .iter()
                .map(badge_label)
                .collect::<Vec<_>>()
                .join(BADGE_SEPARATOR)
        })
        .unwrap_or_default()
}

pub(crate) fn chat_badge_images_json(event: &Event) -> String {
    chat_payload(event)
        .and_then(|chat| serde_json::to_string(&chat.badge_images).ok())
        .unwrap_or_else(|| EMPTY_IMAGE_LIST.to_owned())
}

pub(crate) fn chat_name_color(event: &Event) -> String {
    chat_payload(event)
        .and_then(|chat| chat.author_color)
        .unwrap_or_default()
}

fn badge_label(badge: &UserBadge) -> &'static str {
    match badge {
        UserBadge::Broadcaster => "OWNER",
        UserBadge::Moderator => "MOD",
        UserBadge::Vip => "VIP",
        UserBadge::Subscriber { .. } => "SUB",
        UserBadge::Member { .. } => "MEMBER",
        UserBadge::Bot => "BOT",
        UserBadge::Partner => "PARTNER",
        UserBadge::Premium => "PRIME",
        UserBadge::Founder => "FOUNDER",
        UserBadge::Turbo => "TURBO",
        UserBadge::HypeTrain => "HYPE",
        UserBadge::Bits { .. } => "BITS",
        UserBadge::BitsLeader { .. } => "BITS-LEADER",
    }
}
