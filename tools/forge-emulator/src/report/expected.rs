use forge_events::EventSource;
use serde_json::Value;

use super::text::{clip, code, compact, quoted, span_ms};
use crate::scenario::{
    Expectation, ObservedCount, PayloadMatchers, RequestCount, StepAction, ValueMatcher,
};

const VALUE_CHARS: usize = 120;

/// Event kinds whose pinned name reads better as the subject: kind, pointer, verb.
const SUBJECT_POINTERS: [(&str, &str, &str); 3] = [
    ("action.start", "/action_name", "starts"),
    ("action.done", "/action_name", "finishes"),
    ("command.matched", "/command", "matches"),
];

pub(crate) fn expected(expectation: &Expectation, action: &StepAction) -> String {
    match expectation {
        Expectation::Event(event) => {
            let mut payload = event.payload.clone();
            let subject = match take_subject(&event.kind, &mut payload) {
                Some(subject) => subject,
                None => format!(
                    "forge publishes {}{}",
                    code(&event.kind),
                    from(event.source)
                ),
            };
            let this_message = take_this_message(action, &mut payload);
            format!(
                "{subject}{this_message}{}{} within {}",
                count_phrase(event.count),
                conditions(&payload),
                span_ms(event.within_ms)
            )
        }
        Expectation::EventAbsent(absent) => {
            let mut payload = absent.payload.clone();
            let this_message = take_this_message(action, &mut payload);
            format!(
                "forge publishes no {}{}{this_message}{} within {}",
                code(&absent.kind),
                from(absent.source),
                conditions(&payload),
                span_ms(absent.window_ms)
            )
        }
        Expectation::CausedBy(causation) => format!(
            "the event named {} is directly caused by the event named {}",
            code(&causation.effect),
            code(&causation.cause)
        ),
        Expectation::TwitchSubscription(subscription) => format!(
            "forge holds a live EventSub subscription to {}{} within {}",
            code(&subscription.subscription_type),
            subscription
                .version
                .as_ref()
                .map(|version| format!(" (version {})", code(version)))
                .unwrap_or_default(),
            span_ms(subscription.within_ms)
        ),
        Expectation::TwitchNoUnexpectedRequests {} => {
            "forge sends the fake Twitch only requests it models, over the whole run so far"
                .to_owned()
        }
        Expectation::TwitchRequestCount(count) => format!(
            "forge sends {} {} request(s) over the whole run so far",
            count_range(count.min, count.max),
            code(&request_label(count))
        ),
        Expectation::OverlayContent(content) => {
            let values: Vec<String> = content
                .values
                .0
                .iter()
                .map(|(key, value)| format!("{} = {}", code(key), quoted(value)))
                .collect();
            format!(
                "the page for overlay {} receives a content frame carrying {} as plain values within {}",
                code(&content.overlay),
                values.join(", "),
                span_ms(content.within_ms)
            )
        }
        Expectation::LogLine(line) => {
            let fields: Vec<String> = line
                .fields
                .0
                .iter()
                .map(|(key, value)| format!("{} = {}", code(key), quoted(value)))
                .collect();
            let fields = if fields.is_empty() {
                "any fields".to_owned()
            } else {
                fields.join(", ")
            };
            format!(
                "forge logs a line on target {} with {fields} within {}",
                code(&line.target),
                span_ms(line.within_ms)
            )
        }
    }
}

/// Scenario-file JSON with absent optional fields spelled as null.
pub(crate) fn matcher<T: serde::Serialize>(item: &T) -> String {
    serde_json::to_string(item).unwrap_or_default()
}

pub(crate) fn describe_matcher(matcher: &ValueMatcher) -> String {
    match matcher {
        ValueMatcher::Equals(value) => format!("equals {}", compact(value, VALUE_CHARS)),
        ValueMatcher::Contains(needle) => format!("contains {}", quoted(needle)),
        ValueMatcher::Present(true) => "is present".to_owned(),
        ValueMatcher::Present(false) => "is absent".to_owned(),
    }
}

pub(crate) fn pointer_label(pointer: &str) -> String {
    if pointer.is_empty() {
        "the payload".to_owned()
    } else {
        code(pointer)
    }
}

pub(crate) fn source_name(source: EventSource) -> String {
    match serde_json::to_value(source) {
        Ok(Value::String(name)) => name,
        _ => format!("{source:?}"),
    }
}

/// `kind`, plus the action or command it pins when there is one.
pub(crate) fn event_label(kind: &str, payload: &PayloadMatchers) -> String {
    let pinned = SUBJECT_POINTERS
        .iter()
        .filter(|(subject_kind, _, _)| *subject_kind == kind)
        .find_map(|(_, pointer, _)| pinned_string(payload, pointer));
    match pinned {
        Some(name) => format!("{} for {}", code(kind), code(&clip(name, VALUE_CHARS))),
        None => code(kind),
    }
}

pub(crate) fn request_label(count: &RequestCount) -> String {
    match &count.method {
        Some(method) => format!("{method} {}", count.path),
        None => count.path.clone(),
    }
}

pub(crate) fn count_range(min: Option<u32>, max: Option<u32>) -> String {
    match (min, max) {
        (Some(min), Some(max)) if min == max => format!("exactly {min}"),
        (Some(min), Some(max)) => format!("between {min} and {max}"),
        (Some(min), None) => format!("at least {min}"),
        (None, Some(max)) => format!("at most {max}"),
        (None, None) => "any number of".to_owned(),
    }
}

fn take_subject(kind: &str, payload: &mut PayloadMatchers) -> Option<String> {
    SUBJECT_POINTERS
        .iter()
        .filter(|(subject_kind, _, _)| *subject_kind == kind)
        .find_map(|(_, pointer, verb)| {
            let name = pinned_string(payload, pointer)?.to_owned();
            payload.0.remove(*pointer);
            let noun = if kind == "command.matched" {
                "command"
            } else {
                "action"
            };
            Some(format!("{noun} {} {verb}", code(&clip(&name, VALUE_CHARS))))
        })
}

fn take_this_message(action: &StepAction, payload: &mut PayloadMatchers) -> &'static str {
    let StepAction::Chat(message) = action else {
        return "";
    };
    let pinned = payload
        .0
        .iter()
        .find(|(_, matcher)| match matcher {
            ValueMatcher::Equals(Value::String(text)) => *text == message.text,
            ValueMatcher::Contains(needle) => !needle.is_empty() && message.text.contains(needle),
            ValueMatcher::Equals(_) | ValueMatcher::Present(_) => false,
        })
        .map(|(pointer, _)| pointer.clone());
    match pinned {
        Some(pointer) => {
            payload.0.remove(&pointer);
            " for this message"
        }
        None => "",
    }
}

fn pinned_string<'a>(payload: &'a PayloadMatchers, pointer: &str) -> Option<&'a str> {
    match payload.0.get(pointer) {
        Some(ValueMatcher::Equals(Value::String(text))) => Some(text),
        _ => None,
    }
}

fn from(source: Option<EventSource>) -> String {
    source
        .map(|source| format!(" from {}", source_name(source)))
        .unwrap_or_default()
}

fn count_phrase(count: ObservedCount) -> String {
    match count {
        ObservedCount::AtLeast(1) => String::new(),
        ObservedCount::AtLeast(needed) => format!(" at least {needed} times"),
        ObservedCount::Exactly(1) => " exactly once".to_owned(),
        ObservedCount::Exactly(expected) => format!(" exactly {expected} times"),
    }
}

fn conditions(payload: &PayloadMatchers) -> String {
    if payload.0.is_empty() {
        return String::new();
    }
    let clauses: Vec<String> = payload
        .0
        .iter()
        .map(|(pointer, matcher)| {
            format!("{} {}", pointer_label(pointer), describe_matcher(matcher))
        })
        .collect();
    format!(" where {}", clauses.join(" and "))
}
