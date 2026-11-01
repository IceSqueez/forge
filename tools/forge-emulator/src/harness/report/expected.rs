use super::text::VALUE_CHARS;
use forge_events::{ActionDone, ActionStarted, CommandMatched, EventSource, TypedEvent};
use serde_json::Value;

use super::text::{clip, code, compact, quoted, span_ms};
use crate::harness::scenario::{
    AbsentEvent, DiscordPost, Expectation, HttpRequestSeen, LogLine, ObsRequestSeen, ObservedCount,
    ObservedEvent, OverlayContent, PayloadMatchers, RequestCount, StepAction, TwitchSubscription,
    VTubeRequestSeen, ValueMatcher,
};

const SUBJECT_POINTERS: [(&str, &str, &str); 3] = [
    (ActionStarted::KIND, "/action_name", "starts"),
    (ActionDone::KIND, "/action_name", "finishes"),
    (CommandMatched::KIND, "/command", "matches"),
];

pub(crate) fn expected(expectation: &Expectation, action: &StepAction) -> String {
    match expectation {
        Expectation::Event(event) => event_expected(event, action),
        Expectation::EventAbsent(absent) => absent_expected(absent, action),
        Expectation::CausedBy(causation) => format!(
            "the event named {} is directly caused by the event named {}",
            code(&causation.effect),
            code(&causation.cause)
        ),
        Expectation::TwitchSubscription(subscription) => subscription_expected(subscription),
        Expectation::TwitchNoUnexpectedRequests {} => only_modeled_requests("Twitch"),
        Expectation::TwitchRequestCount(count) => format!(
            "forge sends {} {} request(s) over the whole run so far",
            count_range(count.min, count.max),
            code(&request_label(count))
        ),
        Expectation::KickRequest(request) => http_request_expected("Kick", request),
        Expectation::KickRequestCount(count) => http_count_expected("Kick", count),
        Expectation::KickNoUnexpectedRequests {} => only_modeled_requests("Kick"),
        Expectation::YouTubeRequest(request) => http_request_expected("YouTube", request),
        Expectation::YouTubeRequestCount(count) => http_count_expected("YouTube", count),
        Expectation::YouTubeNoUnexpectedRequests {} => only_modeled_requests("YouTube"),
        Expectation::OverlayContent(content) => overlay_expected(content),
        Expectation::DiscordPost(post) => discord_expected(post),
        Expectation::ObsRequest(request) => obs_request_expected(request),
        Expectation::ObsAuth(auth) => format!(
            "{} within {}",
            if auth.accepted {
                "forge identifies with OBS"
            } else {
                "OBS rejects forge's authentication"
            },
            span_ms(auth.within_ms)
        ),
        Expectation::VtubeRequest(request) => vtube_request_expected(request),
        Expectation::VtubeAuth(auth) => format!(
            "{} within {}",
            if auth.accepted {
                "VTube Studio accepts forge's token"
            } else {
                "VTube Studio rejects forge's token"
            },
            span_ms(auth.within_ms)
        ),
        Expectation::LogLine(line) => log_line_expected(line),
    }
}

fn event_expected(event: &ObservedEvent, action: &StepAction) -> String {
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

fn absent_expected(absent: &AbsentEvent, action: &StepAction) -> String {
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

fn subscription_expected(subscription: &TwitchSubscription) -> String {
    format!(
        "forge holds a live EventSub subscription to {}{} within {}",
        code(&subscription.subscription_type),
        subscription
            .version
            .as_ref()
            .map(|version| format!(" (version {})", code(version)))
            .unwrap_or_default(),
        span_ms(subscription.within_ms)
    )
}

fn overlay_expected(content: &OverlayContent) -> String {
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

fn discord_expected(post: &DiscordPost) -> String {
    let mut conditions = Vec::new();
    if let Some(needle) = &post.content_contains {
        conditions.push(format!("content containing {}", quoted(needle)));
    }
    if let Some(kinds) = &post.mention_parse {
        conditions.push(format!(
            "allowed_mentions.parse = [{}]",
            kinds
                .iter()
                .map(|kind| code(kind))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let conditions = if conditions.is_empty() {
        String::new()
    } else {
        format!(" with {}", conditions.join(" and "))
    };
    format!(
        "forge posts to the Discord webhook {}{conditions} within {}",
        code(&post.webhook),
        span_ms(post.within_ms)
    )
}

fn obs_request_expected(request: &ObsRequestSeen) -> String {
    format!(
        "forge sends OBS a {} request{}{} within {}",
        code(&request.request_type),
        conditions(&request.request_data),
        request
            .code
            .map(|status| format!(" answered with status {status}"))
            .unwrap_or_default(),
        span_ms(request.within_ms)
    )
}

fn vtube_request_expected(request: &VTubeRequestSeen) -> String {
    format!(
        "forge sends VTube Studio a {}{}{} within {}",
        code(&request.message_type),
        conditions(&request.data),
        match (request.succeeded, request.error_id) {
            (_, Some(error_id)) => format!(" answered with error {error_id}"),
            (Some(true), None) => " answered without an error".to_owned(),
            (Some(false), None) => " answered with an error".to_owned(),
            (None, None) => String::new(),
        },
        span_ms(request.within_ms)
    )
}

fn log_line_expected(line: &LogLine) -> String {
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

fn http_request_expected(service: &str, request: &HttpRequestSeen) -> String {
    format!(
        "forge sends the fake {service} {}{}{} within {}",
        code(&match &request.method {
            Some(method) => format!("{method} {}", request.path),
            None => request.path.clone(),
        }),
        conditions(&request.body),
        request
            .status
            .map(|status| format!(" answered with status {status}"))
            .unwrap_or_default(),
        span_ms(request.within_ms)
    )
}

fn http_count_expected(service: &str, count: &RequestCount) -> String {
    format!(
        "forge sends the fake {service} {} {} request(s) over the whole run so far",
        count_range(count.min, count.max),
        code(&request_label(count))
    )
}

fn only_modeled_requests(service: &str) -> String {
    format!("forge sends the fake {service} only requests it models, over the whole run so far")
}

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
            let noun = if kind == CommandMatched::KIND {
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
