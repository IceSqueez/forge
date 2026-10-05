use forge_events::Event;
use forge_types::{ArgStack, ChatPayload, ChatSegment, DeclaredVariable, Variant, VariantKind};

pub const MESSAGE_EMOTES_VARIABLE: &str = "message_emotes";

pub fn message_emotes_declaration() -> DeclaredVariable {
    DeclaredVariable {
        name: MESSAGE_EMOTES_VARIABLE.to_owned(),
        kind: VariantKind::Array,
        label: "Emote codes in the message".to_owned(),
        synthesis: None,
    }
}

pub fn chat_emote_codes(event: &Event) -> Vec<String> {
    let Some(envelope) = event.payload.get(ChatPayload::KEY) else {
        return Vec::new();
    };
    let Ok(chat) = serde_json::from_value::<ChatPayload>(envelope.clone()) else {
        return Vec::new();
    };
    chat.segments
        .into_iter()
        .filter_map(|segment| match segment {
            ChatSegment::Emote { name, .. } if !name.trim().is_empty() => Some(name),
            _ => None,
        })
        .collect()
}

pub fn message_emote_codes(args: &ArgStack) -> Vec<String> {
    args.get(MESSAGE_EMOTES_VARIABLE)
        .and_then(Variant::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Variant::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
