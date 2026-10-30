#![allow(dead_code, clippy::panic, clippy::unwrap_used)]

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

pub fn rejected_field(text: &UserText) -> Option<&str> {
    let UserText::Message(message) = text else {
        return None;
    };
    match message.arg("field") {
        Some(MessageArg::StepFields { keys, .. }) if keys.len() == 1 => Some(keys[0].as_str()),
        _ => None,
    }
}

pub fn step_problem(kind: &str, problem: &forge_registry::ConfigProblem) -> UserText {
    problem
        .message(|keys| MessageArg::StepFields {
            kind: kind.to_owned(),
            keys: keys.to_vec(),
        })
        .into()
}

pub fn nested_problem(text: &UserText) -> Option<&MessageRef> {
    let UserText::Message(message) = text else {
        return None;
    };
    match message.arg("problem") {
        Some(MessageArg::Message(problem)) => Some(problem),
        _ => None,
    }
}

pub fn is_field_problem(text: &UserText, key: &str, code: &str) -> bool {
    rejected_field(text) == Some(key)
        && nested_problem(text).is_some_and(|problem| problem.code().to_string() == code)
}

pub fn plain_field_problem(kind: &str, key: &str, code: &str) -> UserText {
    let reason = forge_registry::ConfigReason::Coded(MessageRef::new(
        forge_types::MessageCode::parse(code).unwrap(),
    ));
    step_problem(kind, &forge_registry::ConfigProblem::field(key, reason))
}
