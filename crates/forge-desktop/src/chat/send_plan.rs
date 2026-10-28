use std::time::Duration;

use forge_events::{Delivery, Event, EventSource, EventSubscription};
use forge_platform_core::{SENDER_FIELD, SenderPreference};
use forge_types::{EventId, PlatformId};

use super::platform_gate::platform_integration;
use crate::home_stats::Integration;

pub(crate) const CHAT_SEND_REQUEST_KIND: &str = "chat.send.request";
pub(crate) const CHAT_SENT_KIND: &str = "chat.send";
pub(crate) const CHAT_SEND_FAILED_KIND: &str = "chat.send.failed";
pub(crate) const CHAT_SEND_RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

const TWITCH_MAX_MESSAGE_CHARS: usize = 500;
const YOUTUBE_MAX_MESSAGE_CHARS: usize = 200;
const KICK_MAX_MESSAGE_CHARS: usize = 500;

pub(crate) fn max_message_chars(platform: PlatformId) -> usize {
    match platform {
        PlatformId::Twitch => TWITCH_MAX_MESSAGE_CHARS,
        PlatformId::YouTube => YOUTUBE_MAX_MESSAGE_CHARS,
        PlatformId::Kick => KICK_MAX_MESSAGE_CHARS,
    }
}

fn platform_of_channel(channel: &str) -> Option<PlatformId> {
    match Integration::from_id(channel)? {
        Integration::Twitch => Some(PlatformId::Twitch),
        Integration::YouTube => Some(PlatformId::YouTube),
        Integration::Kick => Some(PlatformId::Kick),
        Integration::Obs | Integration::VTube => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PlatformReach {
    pub enabled: Vec<PlatformId>,
    pub connected: Vec<PlatformId>,
}

impl PlatformReach {
    pub(crate) fn selectable(&self) -> Vec<PlatformId> {
        self.enabled
            .iter()
            .copied()
            .filter(|platform| self.connected.contains(platform))
            .collect()
    }

    pub(crate) fn route(&self, effective: &[PlatformId]) -> SendRoute {
        let selectable = self.selectable();
        let chosen: Vec<PlatformId> = effective
            .iter()
            .copied()
            .filter(|platform| selectable.contains(platform))
            .collect();
        let covers_selectable = selectable.iter().all(|platform| chosen.contains(platform));
        match chosen.as_slice() {
            [] => SendRoute::Broadcast,
            [_] => SendRoute::Targeted(chosen),
            _ if covers_selectable => SendRoute::Broadcast,
            _ => SendRoute::Targeted(chosen),
        }
    }

    pub(crate) fn recipients(&self, route: &SendRoute) -> Vec<PlatformId> {
        match route {
            SendRoute::Broadcast => self.selectable(),
            SendRoute::Targeted(platforms) => platforms.clone(),
        }
    }

    pub(crate) fn char_limit(&self, effective: &[PlatformId]) -> Option<usize> {
        self.recipients(&self.route(effective))
            .into_iter()
            .map(max_message_chars)
            .min()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SendRoute {
    Broadcast,
    Targeted(Vec<PlatformId>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SendPlan {
    pub message: String,
    pub route: SendRoute,
    pub recipients: Vec<PlatformId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SendRefusal {
    Blank,
    NoPlatformEnabled,
    NoPlatformConnected,
    TooLong { count: usize, limit: usize },
}

pub(crate) fn plan_send(
    text: &str,
    effective: &[PlatformId],
    reach: &PlatformReach,
) -> Result<SendPlan, SendRefusal> {
    let message = text.trim();
    if message.is_empty() {
        return Err(SendRefusal::Blank);
    }
    if reach.enabled.is_empty() {
        return Err(SendRefusal::NoPlatformEnabled);
    }
    if reach.selectable().is_empty() {
        return Err(SendRefusal::NoPlatformConnected);
    }
    let route = reach.route(effective);
    let recipients = reach.recipients(&route);
    let count = message.chars().count();
    if let Some(limit) = recipients.iter().copied().map(max_message_chars).min()
        && count > limit
    {
        return Err(SendRefusal::TooLong { count, limit });
    }
    Ok(SendPlan {
        message: message.to_owned(),
        route,
        recipients,
    })
}

impl SendPlan {
    pub(crate) fn request_events(&self) -> Vec<Event> {
        match &self.route {
            SendRoute::Broadcast => vec![Event::new(
                EventSource::Core,
                CHAT_SEND_REQUEST_KIND,
                serde_json::json!({ "message": self.message }),
            )],
            SendRoute::Targeted(platforms) => platforms
                .iter()
                .map(|platform| {
                    Event::new(
                        EventSource::Core,
                        CHAT_SEND_REQUEST_KIND,
                        serde_json::json!({
                            "target": platform_integration(*platform).id_str(),
                            "message": self.message,
                        }),
                    )
                })
                .collect(),
        }
    }
}

pub(crate) fn attach_twitch_sender(requests: &mut [Event], sender: SenderPreference) {
    let twitch = platform_integration(PlatformId::Twitch).id_str();
    for request in requests {
        let reaches_twitch = match request.payload.get("target").and_then(|t| t.as_str()) {
            None => true,
            Some(target) => target == twitch,
        };
        if reaches_twitch && let Some(fields) = request.payload.as_object_mut() {
            fields.insert(SENDER_FIELD.to_owned(), sender.as_str().into());
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeliveryResult {
    Delivered,
    Failed(String),
    NoResponse,
}

pub(crate) fn delivery_response(
    event: &Event,
    requests: &[EventId],
) -> Option<(PlatformId, DeliveryResult)> {
    let caused_by = event.caused_by?;
    if !requests.contains(&caused_by) {
        return None;
    }
    let platform = event
        .payload
        .get("channel")
        .and_then(|v| v.as_str())
        .and_then(platform_of_channel)?;
    match event.kind.as_str() {
        CHAT_SENT_KIND => Some((platform, DeliveryResult::Delivered)),
        CHAT_SEND_FAILED_KIND => {
            let error = event
                .payload
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_owned();
            Some((platform, DeliveryResult::Failed(error)))
        }
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SendReport {
    pub message: String,
    pub results: Vec<(PlatformId, DeliveryResult)>,
}

impl SendReport {
    pub(crate) fn unanswered(message: String, recipients: &[PlatformId]) -> Self {
        Self {
            message,
            results: recipients
                .iter()
                .map(|platform| (*platform, DeliveryResult::NoResponse))
                .collect(),
        }
    }

    pub(crate) fn record(&mut self, platform: PlatformId, result: DeliveryResult) -> bool {
        match self.results.iter_mut().find(|(recipient, current)| {
            *recipient == platform && *current == DeliveryResult::NoResponse
        }) {
            Some(slot) => {
                slot.1 = result;
                true
            }
            None => false,
        }
    }

    pub(crate) fn is_settled(&self) -> bool {
        self.results
            .iter()
            .all(|(_, result)| *result != DeliveryResult::NoResponse)
    }

    pub(crate) fn any_delivered(&self) -> bool {
        self.results
            .iter()
            .any(|(_, result)| *result == DeliveryResult::Delivered)
    }

    pub(crate) fn problems(&self) -> impl Iterator<Item = &(PlatformId, DeliveryResult)> {
        self.results
            .iter()
            .filter(|(_, result)| *result != DeliveryResult::Delivered)
    }
}

pub(crate) async fn collect_delivery(
    mut subscription: EventSubscription,
    requests: Vec<EventId>,
    mut report: SendReport,
    timeout: Duration,
) -> SendReport {
    let deadline = tokio::time::Instant::now() + timeout;
    while !report.is_settled() {
        let delivery = match tokio::time::timeout_at(deadline, subscription.next()).await {
            Ok(delivery) => delivery,
            Err(_) => break,
        };
        match delivery {
            Delivery::Event(event) => {
                if let Some((platform, result)) = delivery_response(&event, &requests) {
                    report.record(platform, result);
                }
            }
            Delivery::Skipped(_) => {}
            Delivery::Closed => break,
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use forge_events::{Event, EventSource};
    use forge_runtime::{EventBus, NullEventLogRepo};
    use forge_types::EventId;
    use forge_types::PlatformId;

    use super::{
        CHAT_SEND_FAILED_KIND, CHAT_SEND_REQUEST_KIND, CHAT_SENT_KIND, DeliveryResult,
        PlatformReach, SendPlan, SendRefusal, SendReport, SendRoute, attach_twitch_sender,
        collect_delivery, delivery_response, max_message_chars, plan_send,
    };
    use forge_platform_core::{SENDER_FIELD, SenderPreference};

    const T: PlatformId = PlatformId::Twitch;
    const Y: PlatformId = PlatformId::YouTube;
    const K: PlatformId = PlatformId::Kick;
    const LONG_WAIT: Duration = Duration::from_secs(3600);

    fn reach(enabled: &[PlatformId], connected: &[PlatformId]) -> PlatformReach {
        PlatformReach {
            enabled: enabled.to_vec(),
            connected: connected.to_vec(),
        }
    }

    fn answer(kind: &str, payload: serde_json::Value, request: EventId) -> Event {
        Event::caused_by(EventSource::Twitch, kind, payload, request)
    }

    #[test]
    fn route_broadcasts_only_when_nothing_or_everything_reachable_is_chosen() {
        let all = reach(&[T, Y, K], &[T, Y, K]);
        let youtube_down = reach(&[T, Y, K], &[T, K]);
        for (reach, effective, expected) in [
            (&all, vec![], SendRoute::Broadcast),
            (&all, vec![T], SendRoute::Targeted(vec![T])),
            (&all, vec![T, Y, K], SendRoute::Broadcast),
            (&all, vec![T, K], SendRoute::Targeted(vec![T, K])),
            (&youtube_down, vec![T, Y], SendRoute::Targeted(vec![T])),
            (&youtube_down, vec![T, Y, K], SendRoute::Broadcast),
            (&youtube_down, vec![T, K], SendRoute::Broadcast),
        ] {
            assert_eq!(
                reach.route(&effective),
                expected,
                "route for {effective:?} under {reach:?}"
            );
        }
    }

    #[test]
    fn broadcast_recipients_exclude_enabled_platforms_that_are_not_connected() {
        let reach = reach(&[T, Y, K], &[T, K]);

        assert_eq!(reach.recipients(&SendRoute::Broadcast), vec![T, K]);
        assert_eq!(reach.recipients(&SendRoute::Targeted(vec![T])), vec![T]);
    }

    #[test]
    fn plan_send_broadcast_skips_a_failed_platform() {
        let plan = plan_send("hi", &[], &reach(&[T, Y], &[T])).unwrap();

        assert_eq!(plan.route, SendRoute::Broadcast);
        assert_eq!(plan.recipients, vec![T]);
    }

    #[test]
    fn plan_send_filters_a_targeted_disconnected_platform_out_of_the_recipients() {
        let plan = plan_send("hi", &[T, Y], &reach(&[T, Y], &[T])).unwrap();

        assert_eq!(plan.route, SendRoute::Targeted(vec![T]));
        assert_eq!(plan.recipients, vec![T]);
    }

    #[test]
    fn plan_send_refuses_a_target_that_is_only_a_disconnected_platform() {
        assert_eq!(
            plan_send("hi", &[Y], &reach(&[T, Y], &[])),
            Err(SendRefusal::NoPlatformConnected)
        );
    }

    #[test]
    fn plan_send_refuses_blank_text_before_checking_platforms() {
        for text in ["", "   ", "\t\r\n "] {
            assert_eq!(
                plan_send(text, &[T], &reach(&[], &[])),
                Err(SendRefusal::Blank),
                "text {text:?}"
            );
        }
    }

    #[test]
    fn plan_send_reports_no_platform_enabled_ahead_of_no_platform_connected() {
        assert_eq!(
            plan_send("hi", &[T], &reach(&[], &[])),
            Err(SendRefusal::NoPlatformEnabled)
        );
    }

    #[test]
    fn plan_send_refuses_when_every_enabled_platform_is_disconnected() {
        assert_eq!(
            plan_send("hi", &[T], &reach(&[T, K], &[])),
            Err(SendRefusal::NoPlatformConnected)
        );
    }

    #[test]
    fn twitch_and_kick_limits_match_their_documented_500_character_send_caps() {
        assert_eq!(max_message_chars(T), 500);
        assert_eq!(max_message_chars(K), 500);
    }

    #[test]
    fn plan_send_accepts_text_at_the_limit_and_refuses_one_char_over() {
        let limit = max_message_chars(T);
        let twitch_only = reach(&[T], &[T]);
        for (text, expected) in [
            ("a".repeat(limit), Ok(limit)),
            ("ж".repeat(limit), Ok(limit)),
            (format!("  {}\n", "a".repeat(limit)), Ok(limit)),
            (
                "a".repeat(limit + 1),
                Err(SendRefusal::TooLong {
                    count: limit + 1,
                    limit,
                }),
            ),
        ] {
            let outcome =
                plan_send(&text, &[T], &twitch_only).map(|plan| plan.message.chars().count());
            assert_eq!(outcome, expected, "text of {} chars", text.chars().count());
        }
    }

    #[test]
    fn plan_send_limit_is_the_tightest_among_the_connected_recipients() {
        let youtube = max_message_chars(Y);
        let text = "a".repeat(youtube + 1);

        assert_eq!(
            plan_send(&text, &[], &reach(&[T, Y], &[T, Y])),
            Err(SendRefusal::TooLong {
                count: youtube + 1,
                limit: youtube,
            })
        );
        assert!(plan_send(&text, &[T], &reach(&[T, Y, K], &[T, Y, K])).is_ok());
    }

    #[test]
    fn a_disconnected_platform_does_not_clamp_the_broadcast_limit() {
        let text = "a".repeat(max_message_chars(Y) + 1);
        let youtube_down = reach(&[T, Y], &[T]);

        assert!(plan_send(&text, &[], &youtube_down).is_ok());
        assert_eq!(youtube_down.char_limit(&[]), Some(max_message_chars(T)));
    }

    #[test]
    fn plan_send_carries_the_trimmed_message_and_its_route() {
        let plan = plan_send("  hello there \n", &[K], &reach(&[T, K], &[T, K])).unwrap();

        assert_eq!(
            plan,
            SendPlan {
                message: "hello there".to_owned(),
                route: SendRoute::Targeted(vec![K]),
                recipients: vec![K],
            }
        );
    }

    #[test]
    fn a_broadcast_plan_publishes_one_untargeted_core_request() {
        let plan = SendPlan {
            message: "hi all".to_owned(),
            route: SendRoute::Broadcast,
            recipients: vec![T, K],
        };

        let events = plan.request_events();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source, EventSource::Core);
        assert_eq!(events[0].kind, CHAT_SEND_REQUEST_KIND);
        assert_eq!(
            events[0].payload,
            serde_json::json!({ "message": "hi all" })
        );
    }

    #[test]
    fn a_targeted_plan_publishes_one_core_request_per_platform_by_wire_id() {
        let plan = SendPlan {
            message: "hi".to_owned(),
            route: SendRoute::Targeted(vec![Y, K]),
            recipients: vec![Y, K],
        };

        let events = plan.request_events();

        let shapes: Vec<_> = events
            .iter()
            .map(|event| (event.source, event.kind.as_str(), event.payload.clone()))
            .collect();
        assert_eq!(
            shapes,
            vec![
                (
                    EventSource::Core,
                    CHAT_SEND_REQUEST_KIND,
                    serde_json::json!({ "target": "youtube", "message": "hi" })
                ),
                (
                    EventSource::Core,
                    CHAT_SEND_REQUEST_KIND,
                    serde_json::json!({ "target": "kick", "message": "hi" })
                ),
            ]
        );
        assert_ne!(events[0].id, events[1].id);
    }

    #[test]
    fn delivery_response_maps_only_answers_to_our_requests_from_chat_platforms() {
        let ours = EventId::new();
        let foreign = EventId::new();
        let requests = [ours];
        let cases = [
            (
                Event::new(
                    EventSource::Twitch,
                    CHAT_SENT_KIND,
                    serde_json::json!({ "channel": "twitch" }),
                ),
                None,
            ),
            (
                answer(
                    CHAT_SENT_KIND,
                    serde_json::json!({ "channel": "twitch" }),
                    foreign,
                ),
                None,
            ),
            (
                answer(
                    CHAT_SENT_KIND,
                    serde_json::json!({ "channel": "twitch" }),
                    ours,
                ),
                Some((T, DeliveryResult::Delivered)),
            ),
            (
                answer(
                    CHAT_SEND_FAILED_KIND,
                    serde_json::json!({ "channel": "kick", "error": "slow mode" }),
                    ours,
                ),
                Some((K, DeliveryResult::Failed("slow mode".to_owned()))),
            ),
            (
                answer(
                    CHAT_SEND_FAILED_KIND,
                    serde_json::json!({ "channel": "youtube" }),
                    ours,
                ),
                Some((Y, DeliveryResult::Failed(String::new()))),
            ),
            (
                answer(
                    CHAT_SENT_KIND,
                    serde_json::json!({ "channel": "obs" }),
                    ours,
                ),
                None,
            ),
            (
                answer(
                    CHAT_SENT_KIND,
                    serde_json::json!({ "channel": "myspace" }),
                    ours,
                ),
                None,
            ),
            (answer(CHAT_SENT_KIND, serde_json::json!({}), ours), None),
            (
                answer(
                    "chat.whisper.sent",
                    serde_json::json!({ "channel": "twitch" }),
                    ours,
                ),
                None,
            ),
        ];

        for (event, expected) in cases {
            assert_eq!(
                delivery_response(&event, &requests),
                expected,
                "{} {}",
                event.kind,
                event.payload
            );
        }
    }

    #[test]
    fn record_keeps_the_first_answer_per_recipient() {
        let mut report = SendReport::unanswered("hi".to_owned(), &[T]);

        let first = report.record(T, DeliveryResult::Failed("boom".to_owned()));
        let second = report.record(T, DeliveryResult::Delivered);

        assert_eq!((first, second), (true, false));
        assert_eq!(
            report.results,
            vec![(T, DeliveryResult::Failed("boom".to_owned()))]
        );
    }

    #[test]
    fn record_ignores_platforms_that_were_not_recipients() {
        let mut report = SendReport::unanswered("hi".to_owned(), &[T]);

        assert!(!report.record(K, DeliveryResult::Delivered));
        assert_eq!(report.results, vec![(T, DeliveryResult::NoResponse)]);
    }

    #[test]
    fn a_report_settles_only_once_every_recipient_answered() {
        let mut report = SendReport::unanswered("hi".to_owned(), &[T, K]);
        let mut settled = vec![report.is_settled()];

        report.record(T, DeliveryResult::Delivered);
        settled.push(report.is_settled());
        report.record(K, DeliveryResult::Failed("x".to_owned()));
        settled.push(report.is_settled());

        assert_eq!(settled, vec![false, false, true]);
    }

    #[test]
    fn problems_list_every_recipient_that_did_not_deliver() {
        let mut report = SendReport::unanswered("hi".to_owned(), &[T, Y, K]);
        report.record(T, DeliveryResult::Delivered);
        report.record(K, DeliveryResult::Failed("banned".to_owned()));

        let problems: Vec<_> = report.problems().cloned().collect();

        assert!(report.any_delivered());
        assert_eq!(
            problems,
            vec![
                (Y, DeliveryResult::NoResponse),
                (K, DeliveryResult::Failed("banned".to_owned())),
            ]
        );
    }

    #[test]
    fn any_delivered_is_false_when_nothing_landed() {
        let mut report = SendReport::unanswered("hi".to_owned(), &[T, K]);
        report.record(T, DeliveryResult::Failed("x".to_owned()));

        assert!(!report.any_delivered());
    }

    fn bus() -> Arc<EventBus> {
        EventBus::new(Arc::new(NullEventLogRepo))
    }

    #[tokio::test(start_paused = true)]
    async fn collect_delivery_returns_as_soon_as_every_recipient_answered() {
        let bus = bus();
        let request = EventId::new();
        let subscription = bus.subscribe();
        bus.publish(answer(
            CHAT_SENT_KIND,
            serde_json::json!({ "channel": "twitch" }),
            request,
        ));
        bus.publish(answer(
            CHAT_SEND_FAILED_KIND,
            serde_json::json!({ "channel": "kick", "error": "x" }),
            request,
        ));
        let started = tokio::time::Instant::now();

        let report = collect_delivery(
            subscription,
            vec![request],
            SendReport::unanswered("hi".to_owned(), &[T, K]),
            LONG_WAIT,
        )
        .await;

        assert!(report.is_settled());
        assert!(started.elapsed() < LONG_WAIT);
    }

    #[tokio::test(start_paused = true)]
    async fn collect_delivery_marks_silent_recipients_as_no_response_at_the_deadline() {
        let bus = bus();
        let request = EventId::new();
        let subscription = bus.subscribe();
        bus.publish(answer(
            CHAT_SENT_KIND,
            serde_json::json!({ "channel": "twitch" }),
            request,
        ));
        bus.publish(answer(
            CHAT_SENT_KIND,
            serde_json::json!({ "channel": "kick" }),
            EventId::new(),
        ));

        let report = collect_delivery(
            subscription,
            vec![request],
            SendReport::unanswered("hi".to_owned(), &[T, K]),
            Duration::from_millis(50),
        )
        .await;

        assert_eq!(
            report.results,
            vec![
                (T, DeliveryResult::Delivered),
                (K, DeliveryResult::NoResponse)
            ]
        );
    }

    fn plan(route: SendRoute) -> SendPlan {
        let recipients = match &route {
            SendRoute::Broadcast => vec![T, Y, K],
            SendRoute::Targeted(platforms) => platforms.clone(),
        };
        SendPlan {
            message: "hi".to_owned(),
            route,
            recipients,
        }
    }

    fn senders(requests: &[Event]) -> Vec<(Option<String>, Option<String>)> {
        requests
            .iter()
            .map(|request| {
                let read = |key: &str| {
                    request
                        .payload
                        .get(key)
                        .and_then(|value| value.as_str())
                        .map(str::to_owned)
                };
                (read("target"), read(SENDER_FIELD))
            })
            .collect()
    }

    #[test]
    fn broadcast_request_carries_the_twitch_sender() {
        let mut requests = plan(SendRoute::Broadcast).request_events();

        attach_twitch_sender(&mut requests, SenderPreference::BotOnly);

        assert_eq!(senders(&requests), vec![(None, Some("bot".to_owned()))]);
    }

    #[test]
    fn targeted_requests_carry_the_sender_only_on_the_twitch_request() {
        let mut requests = plan(SendRoute::Targeted(vec![Y, T, K])).request_events();

        attach_twitch_sender(&mut requests, SenderPreference::Streamer);

        assert_eq!(
            senders(&requests),
            vec![
                (Some("youtube".to_owned()), None),
                (Some("twitch".to_owned()), Some("streamer".to_owned())),
                (Some("kick".to_owned()), None),
            ]
        );
    }

    #[test]
    fn requests_without_twitch_are_left_without_a_sender() {
        let mut requests = plan(SendRoute::Targeted(vec![Y, K])).request_events();
        let before: Vec<serde_json::Value> = requests
            .iter()
            .map(|request| request.payload.clone())
            .collect();

        attach_twitch_sender(&mut requests, SenderPreference::BotOnly);

        let after: Vec<serde_json::Value> = requests
            .iter()
            .map(|request| request.payload.clone())
            .collect();
        assert_eq!(after, before);
    }
}
