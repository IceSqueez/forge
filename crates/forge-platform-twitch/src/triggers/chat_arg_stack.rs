use forge_events::Event;
use forge_types::{ArgStack, Variant};

pub(super) fn base_chat_args(event: &Event) -> ArgStack {
    let message_text = event
        .payload
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let user_login = event
        .payload
        .get("user")
        .and_then(|u| u.get("login"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let user_id = event
        .payload
        .get("user")
        .and_then(|u| u.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let channel = event
        .payload
        .get("channel")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let color = event
        .payload
        .get("color")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();

    ArgStack::new()
        .set("message_text".to_owned(), Variant::String(message_text))
        .set("user_login".to_owned(), Variant::String(user_login))
        .set("user_id".to_owned(), Variant::String(user_id))
        .set("channel".to_owned(), Variant::String(channel))
        .set("user_color".to_owned(), Variant::String(color))
}
