use forge_events::Event;
use forge_types::{ChatViewer, DeclaredVariable, TriggerConfig, Variant, VariantKind};

use crate::form::FormField;

pub const FIRST_CHATTERS_ONLY: &str = "first_chatters_only";
pub const FIRST_MESSAGE_VARIABLE: &str = "first_message";

const ANY_MESSAGE_CONDITION: &str = "any";
const FIRST_CHATTERS_CONDITION: &str = "first-time chatters";

pub fn first_chatters_only_field() -> FormField {
    FormField::Toggle {
        key: FIRST_CHATTERS_ONLY,
        label: "Only first-time chatters",
    }
}

pub fn first_message_declaration() -> DeclaredVariable {
    DeclaredVariable {
        name: FIRST_MESSAGE_VARIABLE.to_owned(),
        kind: VariantKind::Bool,
        label: "First message forge has seen from this viewer".to_owned(),
        synthesis: None,
    }
}

pub fn is_first_chat_message(event: &Event) -> bool {
    ChatViewer::read(&event.payload).is_some_and(|viewer| viewer.first_message)
}

pub fn first_chatters_only(config: &TriggerConfig) -> bool {
    config
        .get(FIRST_CHATTERS_ONLY)
        .and_then(Variant::as_bool)
        .unwrap_or(false)
}

pub fn admits_chatter(config: &TriggerConfig, event: &Event) -> bool {
    !first_chatters_only(config) || is_first_chat_message(event)
}

pub fn chat_message_condition(config: &TriggerConfig) -> String {
    if first_chatters_only(config) {
        FIRST_CHATTERS_CONDITION.to_owned()
    } else {
        ANY_MESSAGE_CONDITION.to_owned()
    }
}
