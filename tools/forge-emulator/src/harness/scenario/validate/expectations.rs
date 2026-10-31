use forge_events::{ActionStarted, CommandMatched, TypedEvent};

use super::{MAX_WAIT_MS, NamedEvent, Validator};
use crate::harness::scenario::expectation::{
    AbsentEvent, Causation, DiscordPost, Expectation, HttpRequestSeen, LogLine, ObservedEvent,
    OverlayContent, RequestCount, TwitchSubscription,
};
use crate::harness::scenario::matcher::{PayloadMatchers, ValueMatcher};

const PAYLOAD_FIELD: &str = "payload";
const BODY_FIELD: &str = "body";
const REQUEST_DATA_FIELD: &str = "request_data";
const DATA_FIELD: &str = "data";
const COMMAND_MATCHED: &str = CommandMatched::KIND;
const COMMAND_POINTER: &str = "/command";
const ACTION_START: &str = ActionStarted::KIND;
const ACTION_NAME_POINTER: &str = "/action_name";
const PROSE_FIELD: &str = "message";
const MENTION_KINDS: [&str; 3] = ["users", "roles", "everyone"];

impl<'a> Validator<'a> {
    pub(super) fn check_http_request(&mut self, location: &str, request: &HttpRequestSeen) {
        self.check_request_count(
            location,
            &RequestCount {
                method: request.method.clone(),
                path: request.path.clone(),
                min: Some(1),
                max: None,
            },
        );
        self.check_within(location, request.within_ms);
        self.check_payload(location, BODY_FIELD, &request.body);
    }

    pub(super) fn check_expectation(&mut self, location: &str, expectation: &'a Expectation) {
        match expectation {
            Expectation::Event(event) => self.check_observed(location, event),
            Expectation::EventAbsent(absent) => self.check_absent(location, absent),
            Expectation::CausedBy(causation) => self.check_causation(location, causation),
            Expectation::TwitchSubscription(subscription) => {
                self.check_subscription_expectation(location, subscription);
            }
            Expectation::TwitchNoUnexpectedRequests {} => {}
            Expectation::TwitchRequestCount(count)
            | Expectation::KickRequestCount(count)
            | Expectation::YoutubeRequestCount(count) => {
                self.check_request_count(location, count);
            }
            Expectation::KickRequest(request) => self.check_http_request(location, request),
            Expectation::KickNoUnexpectedRequests {} => {}
            Expectation::YoutubeRequest(request) => self.check_http_request(location, request),
            Expectation::YoutubeNoUnexpectedRequests {} => {}
            Expectation::OverlayContent(content) => self.check_overlay_content(location, content),
            Expectation::LogLine(line) => self.check_log_line(location, line),
            Expectation::DiscordPost(post) => self.check_discord_post(location, post),
            Expectation::ObsRequest(request) => {
                self.not_blank(format!("{location}.request_type"), &request.request_type);
                self.check_payload(location, REQUEST_DATA_FIELD, &request.request_data);
                self.check_within(location, request.within_ms);
            }
            Expectation::ObsAuth(auth) => {
                self.check_within(location, auth.within_ms);
            }
            Expectation::VtubeRequest(request) => {
                self.not_blank(format!("{location}.message_type"), &request.message_type);
                self.check_payload(location, DATA_FIELD, &request.data);
                if request.succeeded == Some(true) && request.error_id.is_some() {
                    self.report(
                        format!("{location}.error_id"),
                        "contradicts succeeded: a request answered with an error did not succeed",
                    );
                }
                self.check_within(location, request.within_ms);
            }
            Expectation::VtubeAuth(auth) => {
                self.check_within(location, auth.within_ms);
            }
        }
    }

    pub(super) fn check_payload(
        &mut self,
        location: &str,
        field: &str,
        matchers: &PayloadMatchers,
    ) {
        for (pointer, matcher) in &matchers.0 {
            if !pointer.is_empty() && !pointer.starts_with('/') {
                self.report(
                    format!("{location}.{field}"),
                    format!("key `{pointer}` is not a JSON pointer; write `/{pointer}`"),
                );
            }
            if matches!(matcher, ValueMatcher::Contains(needle) if needle.is_empty()) {
                self.report(
                    format!("{location}.{field}[{pointer}].contains"),
                    "must not be empty: it would match every string",
                );
            }
        }
    }

    pub(super) fn check_observed(&mut self, location: &str, event: &'a ObservedEvent) {
        self.not_blank(format!("{location}.kind"), &event.kind);
        self.check_within(location, event.within_ms);
        self.check_payload(location, PAYLOAD_FIELD, &event.payload);
        if event.count.minimum() == 0 {
            self.report(
                format!("{location}.count"),
                "must be at least 1; expect absence with event_absent",
            );
        }
        if event.kind == COMMAND_MATCHED {
            self.check_command_expectation(location, event);
        }
        if event.kind == ACTION_START
            && let Some(action) = event.pattern().pinned_string(ACTION_NAME_POINTER)
            && !self.defines_action(action)
        {
            self.report(
                format!("{location}.payload[{ACTION_NAME_POINTER}]"),
                format!("expects action `{action}` to start, which the fixture does not define"),
            );
        }
        if let Some(name) = &event.name {
            self.declare_name(location, name, event.count.is_single());
        }
    }

    pub(super) fn check_command_expectation(&mut self, location: &str, event: &ObservedEvent) {
        if self.command_phrases().next().is_none() {
            self.report(
                format!("{location}.kind"),
                format!("expects {COMMAND_MATCHED}, but the fixture defines no chat command"),
            );
            return;
        }
        if let Some(command) = event.pattern().pinned_string(COMMAND_POINTER)
            && !self.command_phrases().any(|phrase| phrase == command)
        {
            self.report(
                format!("{location}.payload[{COMMAND_POINTER}]"),
                format!("expects command `{command}`, which the fixture does not define"),
            );
        }
        let needed = u64::from(event.count.minimum());
        if needed > self.command_matches_sent {
            let message = format!(
                "needs {needed} {COMMAND_MATCHED} event(s), but chat sent up to this step can produce at most {}",
                self.command_matches_sent
            );
            self.report(location, message);
        }
    }

    pub(super) fn declare_name(&mut self, location: &str, name: &'a str, single: bool) {
        let at = format!("{location}.name");
        if name.trim().is_empty() {
            self.report(at, "must not be blank");
            return;
        }
        if let Some(existing) = self.named_events.get(name) {
            let message = format!("`{name}` is already declared at {}", existing.location);
            self.report(at, message);
            return;
        }
        self.named_events.insert(
            name,
            NamedEvent {
                location: location.to_owned(),
                single,
            },
        );
    }

    pub(super) fn check_absent(&mut self, location: &str, absent: &AbsentEvent) {
        self.not_blank(format!("{location}.kind"), &absent.kind);
        self.in_range(
            format!("{location}.window_ms"),
            absent.window_ms,
            1,
            MAX_WAIT_MS,
        );
        self.check_payload(location, PAYLOAD_FIELD, &absent.payload);
    }

    pub(super) fn check_causation(&mut self, location: &str, causation: &Causation) {
        if causation.effect == causation.cause {
            self.report(
                location,
                format!(
                    "effect and cause are both `{}`; an event cannot cause itself",
                    causation.effect
                ),
            );
            return;
        }
        for (role, name) in [("effect", &causation.effect), ("cause", &causation.cause)] {
            let problem = match self.named_events.get(name.as_str()) {
                None => Some(format!(
                    "names `{name}`, which no event expectation declares at or before this point"
                )),
                Some(named) if !named.single => Some(format!(
                    "`{name}` may match several events; causation needs a name whose count is 1"
                )),
                Some(_) => None,
            };
            if let Some(problem) = problem {
                self.report(format!("{location}.{role}"), problem);
            }
        }
    }

    pub(super) fn check_subscription_expectation(
        &mut self,
        location: &str,
        subscription: &TwitchSubscription,
    ) {
        self.not_blank(format!("{location}.type"), &subscription.subscription_type);
        if let Some(version) = &subscription.version {
            self.not_blank(format!("{location}.version"), version);
        }
        self.check_within(location, subscription.within_ms);
    }

    pub(super) fn check_request_count(&mut self, location: &str, count: &RequestCount) {
        if !count.path.starts_with('/') {
            self.report(format!("{location}.path"), "must start with `/`");
        }
        if let Some(method) = &count.method
            && (method.is_empty() || !method.chars().all(|c| c.is_ascii_uppercase()))
        {
            self.report(
                format!("{location}.method"),
                format!("`{method}` is not an upper-case HTTP method such as GET"),
            );
        }
        match (count.min, count.max) {
            (None, None) => self.report(location, "needs min, max, or both"),
            (Some(min), Some(max)) if min > max => {
                self.report(location, format!("min {min} exceeds max {max}"));
            }
            _ => {}
        }
    }

    pub(super) fn check_overlay_content(&mut self, location: &str, content: &OverlayContent) {
        self.require_seeded_overlay(&format!("{location}.overlay"), &content.overlay);
        if !self.pages_opened.contains(content.overlay.as_str()) {
            let message = format!(
                "expects content on `{}` before any overlay_page step opened it, so nothing can be delivered there",
                content.overlay
            );
            self.report(location, message);
        }
        self.check_within(location, content.within_ms);
        if content.values.0.is_empty() {
            self.report(
                format!("{location}.values"),
                "needs at least one content key; an empty frame proves nothing",
            );
        }
        for key in content.values.0.keys() {
            if key.trim().is_empty() {
                self.report(format!("{location}.values"), "has a blank content key");
            }
        }
    }

    pub(super) fn check_discord_post(&mut self, location: &str, post: &DiscordPost) {
        let declared = self
            .scenario
            .fixture
            .discord_webhooks
            .iter()
            .any(|webhook| webhook.name == post.webhook);
        if !declared {
            self.report(
                format!("{location}.webhook"),
                format!(
                    "expects a post to `{}`, which fixture.discord_webhooks does not declare",
                    post.webhook
                ),
            );
        }
        self.check_within(location, post.within_ms);
        if post.content_contains.as_deref() == Some("") {
            self.report(
                format!("{location}.content_contains"),
                "must not be empty: it would match every post",
            );
        }
        for (index, kind) in post.mention_parse.iter().flatten().enumerate() {
            let repeated = post
                .mention_parse
                .iter()
                .flatten()
                .take(index)
                .any(|earlier| earlier == kind);
            if !MENTION_KINDS.contains(&kind.as_str()) || repeated {
                self.report(
                    format!("{location}.mention_parse[{index}]"),
                    format!(
                        "`{kind}` is repeated or not one of {}",
                        MENTION_KINDS.join(", ")
                    ),
                );
            }
        }
    }

    pub(super) fn check_log_line(&mut self, location: &str, line: &LogLine) {
        self.not_blank(format!("{location}.target"), &line.target);
        self.check_within(location, line.within_ms);
        if line.fields.0.is_empty() {
            self.report(
                format!("{location}.fields"),
                "needs at least one structured field; log prose is never matched",
            );
        }
        for key in line.fields.0.keys() {
            if key.trim().is_empty() {
                self.report(format!("{location}.fields"), "has a blank field name");
            } else if key == PROSE_FIELD {
                self.report(
                    format!("{location}.fields.{PROSE_FIELD}"),
                    "is the prose line, which may be reworded; match structured fields instead",
                );
            }
        }
    }
}
