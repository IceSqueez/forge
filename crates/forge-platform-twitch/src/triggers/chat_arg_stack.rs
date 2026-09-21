use forge_events::Event;
use forge_registry::{ActorBlock, ActorIdentity, LoginSlot, TriggerVariables};
use forge_types::{ActorRole, DeclaredVariable, PlatformId, Variant, VariantKind};

use crate::payload_fields::chat as fields;

const CHATTER: ActorBlock = ActorBlock {
    role: ActorRole::Principal,
    platform: PlatformId::Twitch,
    login: LoginSlot::Declared,
};

pub(super) fn base_chat_variables() -> TriggerVariables {
    TriggerVariables::new()
        .actor(CHATTER, chatter_identity)
        .message_text(chat_message_text)
        .event_specific(
            DeclaredVariable {
                name: "channel".to_owned(),
                kind: VariantKind::String,
                label: "Channel login".to_owned(),
                synthesis: None,
            },
            |event| Variant::String(text_field(event, fields::CHANNEL)),
        )
        .event_specific(
            DeclaredVariable {
                name: "user_color".to_owned(),
                kind: VariantKind::String,
                label: "Username color".to_owned(),
                synthesis: None,
            },
            |event| Variant::String(text_field(event, fields::COLOR)),
        )
}

fn chatter_identity(event: &Event) -> ActorIdentity {
    ActorIdentity {
        id: user_field(event, fields::USER_ID),
        display_name: user_field(event, fields::USER_DISPLAY_NAME),
        login: Some(user_field(event, fields::USER_LOGIN)),
    }
}

fn chat_message_text(event: &Event) -> String {
    text_field(event, fields::MESSAGE)
}

fn text_field(event: &Event, key: &str) -> String {
    event
        .payload
        .get(key)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned()
}

fn user_field(event: &Event, key: &str) -> String {
    event
        .payload
        .get(fields::USER)
        .and_then(|user| user.get(key))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned()
}
