#![allow(dead_code, clippy::panic)]

use forge_types::{MessageArg, MessageRef, SubActionOutcome, UserText, VerbatimText};

pub fn message_of(text: &UserText) -> &MessageRef {
    match text {
        UserText::Message(message) => message,
        UserText::Verbatim(verbatim) => panic!("expected a coded message, got {verbatim:?}"),
    }
}

pub fn failure_message(outcome: &SubActionOutcome) -> &MessageRef {
    match outcome {
        SubActionOutcome::Failed(text) => message_of(text),
        other => panic!("expected Failed, got {other:?}"),
    }
}

pub fn code_of(message: &MessageRef) -> String {
    message.code().to_string()
}

pub fn verbatim_arg<'a>(message: &'a MessageRef, name: &str) -> Option<&'a str> {
    match message.arg(name) {
        Some(MessageArg::Verbatim(text)) => Some(text.as_str()),
        _ => None,
    }
}

pub fn detail_of(message: &MessageRef) -> Option<&str> {
    message.detail().map(|detail| detail.as_str())
}

pub fn verbatim(text: &str) -> MessageArg {
    MessageArg::Verbatim(VerbatimText::new(text))
}
