use std::sync::Arc;

use forge_events::{
    ActingSender, ChatSendFailed, ChatSendRequest, ChatSent, Event, EventSource, EventsError,
    TypedEvent, WhisperSent,
};
use forge_platform_core::{ChatPlatform, SenderPreference, unknown_sender_reason};
use forge_types::{EventId, PlatformId, requested_chat_target};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::{EventBus, TaskGroup};

const CHAT_SEND_QUEUE_CAPACITY: usize = 64;

enum ChatSendIntent {
    Plain,
    Reply { parent_message_id: String },
    Whisper { recipient_login: String },
}

impl ChatSendIntent {
    fn of(request: &ChatSendRequest) -> Self {
        if let Some(recipient) = &request.whisper_to_login {
            return Self::Whisper {
                recipient_login: recipient.trim().to_owned(),
            };
        }
        match request
            .reply_to_message_id
            .as_deref()
            .map(str::trim)
            .filter(|parent| !parent.is_empty())
        {
            Some(parent) => Self::Reply {
                parent_message_id: parent.to_owned(),
            },
            None => Self::Plain,
        }
    }
}

struct QueuedChatSend {
    message: String,
    intent: ChatSendIntent,
    sender: SenderPreference,
    caused_by: EventId,
}

struct RouteTarget {
    platform: PlatformId,
    source: EventSource,
}

impl RouteTarget {
    fn channel(&self) -> &'static str {
        self.platform.as_str()
    }

    fn failed(&self, error: String, caused_by: Option<EventId>) -> Event {
        ChatSendFailed {
            channel: self.channel().to_owned(),
            error,
        }
        .into_event(self.source, caused_by)
    }

    fn sent(
        &self,
        message: String,
        reply_to_message_id: Option<String>,
        sent_as: Option<ActingSender>,
        caused_by: EventId,
    ) -> Event {
        ChatSent {
            channel: self.channel().to_owned(),
            message,
            reply_to_message_id,
            sent_as,
        }
        .into_event(self.source, Some(caused_by))
    }

    fn whispered(
        &self,
        recipient_login: String,
        sent_as: Option<ActingSender>,
        caused_by: EventId,
    ) -> Event {
        WhisperSent {
            channel: self.channel().to_owned(),
            whisper_to_login: recipient_login,
            sent_as,
        }
        .into_event(self.source, Some(caused_by))
    }

    fn addressed_by(&self, requested: Option<&str>) -> bool {
        requested.is_none_or(|requested| PlatformId::from_wire(requested) == Some(self.platform))
    }
}

fn whispers_unsupported_reason(channel: &str) -> String {
    format!("whispers are not supported on {channel}")
}

fn sender_of(request: &ChatSendRequest) -> Result<SenderPreference, String> {
    match request.sender.as_deref() {
        None => Ok(SenderPreference::Default),
        Some(raw) => SenderPreference::parse(raw).ok_or_else(|| unknown_sender_reason(raw)),
    }
}

fn is_script_or_core_request(event: &Event) -> bool {
    event.kind == ChatSendRequest::KIND
        && matches!(event.source, EventSource::Core | EventSource::Rhai)
}

fn spawn_chat_send_worker(
    bus: Arc<EventBus>,
    platform: Arc<dyn ChatPlatform>,
    route: Arc<RouteTarget>,
    mut requests: mpsc::Receiver<QueuedChatSend>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let channel = route.channel();
        while let Some(request) = requests.recv().await {
            let QueuedChatSend {
                message,
                intent,
                sender,
                caused_by,
            } = request;
            let delivered = match intent {
                ChatSendIntent::Plain => platform
                    .send_message_as(channel, &message, sender)
                    .await
                    .map(|sent_as| route.sent(message, None, sent_as, caused_by)),
                ChatSendIntent::Reply { parent_message_id } => platform
                    .send_reply_as(channel, &parent_message_id, &message, sender)
                    .await
                    .map(|sent_as| {
                        route.sent(message, Some(parent_message_id), sent_as, caused_by)
                    }),
                ChatSendIntent::Whisper { recipient_login } => platform
                    .send_whisper_as(&recipient_login, &message, sender)
                    .await
                    .map(|acting| route.whispered(recipient_login, acting, caused_by)),
            };
            match delivered {
                Ok(event) => bus.publish(event),
                Err(e) => bus.publish(route.failed(e.to_string(), Some(caused_by))),
            }
        }
    })
}

pub fn spawn_chat_send_router(
    bus: Arc<EventBus>,
    platform: Arc<dyn ChatPlatform>,
    platform_id: PlatformId,
) -> TaskGroup {
    let route = Arc::new(RouteTarget {
        platform: platform_id,
        source: EventSource::from(platform_id),
    });
    let (tx, rx) = mpsc::channel(CHAT_SEND_QUEUE_CAPACITY);
    let mut tasks = TaskGroup::default();
    tasks.track(spawn_chat_send_worker(
        Arc::clone(&bus),
        platform,
        Arc::clone(&route),
        rx,
    ));

    let whispers_supported = platform_id.supports_whispers();
    tasks.track(tokio::spawn(async move {
        let channel = route.channel();
        let mut sub = bus.subscribe();
        let mut lag_count: u64 = 0;
        let mut lagging = false;
        loop {
            let event = match sub.recv().await {
                Ok(e) => {
                    lagging = false;
                    e
                }
                Err(EventsError::BusClosed) => break,
                Err(EventsError::LaggingReceiver) if lagging => continue,
                Err(EventsError::LaggingReceiver) => {
                    lagging = true;
                    lag_count += 1;
                    tracing::warn!(channel, lag_count, "chat send bridge lagging");
                    bus.publish(route.failed(
                        format!(
                            "event bus receiver lagging ({lag_count} lag event(s) observed); {} events may have been dropped",
                            ChatSendRequest::KIND
                        ),
                        None,
                    ));
                    continue;
                }
                Err(_) => continue,
            };
            if !is_script_or_core_request(&event) {
                continue;
            }
            let Some(request) = ChatSendRequest::from_event(&event) else {
                continue;
            };
            let requested = request.target.as_deref().and_then(requested_chat_target);
            if !route.addressed_by(requested) {
                continue;
            }
            let caused_by = event.id;
            let sender = match sender_of(&request) {
                Ok(sender) => sender,
                Err(reason) => {
                    bus.publish(route.failed(reason, Some(caused_by)));
                    continue;
                }
            };
            let intent = ChatSendIntent::of(&request);
            if matches!(intent, ChatSendIntent::Whisper { .. }) && !whispers_supported {
                if requested.is_some() {
                    bus.publish(route.failed(whispers_unsupported_reason(channel), Some(caused_by)));
                }
                continue;
            }
            let queued = QueuedChatSend {
                message: request.message,
                intent,
                sender,
                caused_by,
            };
            if tx.try_send(queued).is_err() {
                bus.publish(route.failed(
                    "chat send queue full or worker unavailable; request dropped".to_owned(),
                    Some(caused_by),
                ));
            }
        }
    }));
    tasks
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use forge_events::{EventChannel, EventSubscription, SenderRole};
    use forge_platform_core::{ConnectionState, PlatformError};
    use serde_json::json;
    use tokio::sync::Semaphore;

    use super::*;
    use crate::{Config, NullEventLogRepo};

    const ARRIVAL: Duration = Duration::from_secs(30);

    #[derive(Debug, PartialEq)]
    enum PlatformCall {
        Message {
            channel: String,
            text: String,
            sender: SenderPreference,
        },
        Reply {
            channel: String,
            parent: String,
            text: String,
            sender: SenderPreference,
        },
        Whisper {
            recipient: String,
            text: String,
            sender: SenderPreference,
        },
        Unrouted,
    }

    #[derive(Default)]
    struct MockSpec {
        gate: Option<Arc<Semaphore>>,
        failure: Option<&'static str>,
        acting: Option<ActingSender>,
    }

    struct MockPlatform {
        calls: mpsc::UnboundedSender<PlatformCall>,
        spec: MockSpec,
    }

    impl MockPlatform {
        async fn record(&self, call: PlatformCall) -> Result<Option<ActingSender>, PlatformError> {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the test may have stopped listening"
            )]
            let _ = self.calls.send(call);
            if let Some(gate) = &self.spec.gate {
                gate.acquire().await.unwrap().forget();
            }
            match self.spec.failure {
                Some(reason) => Err(PlatformError::Network {
                    reason: reason.to_owned(),
                }),
                None => Ok(self.spec.acting.clone()),
            }
        }
    }

    #[async_trait::async_trait]
    impl ChatPlatform for MockPlatform {
        fn connection_state(&self) -> ConnectionState {
            ConnectionState::Connected
        }
        async fn connect(&self) -> Result<(), PlatformError> {
            Ok(())
        }
        async fn disconnect(&self) -> Result<(), PlatformError> {
            Ok(())
        }
        async fn send_message(&self, _channel: &str, _text: &str) -> Result<(), PlatformError> {
            self.record(PlatformCall::Unrouted).await.map(|_| ())
        }
        async fn send_message_as(
            &self,
            channel: &str,
            text: &str,
            sender: SenderPreference,
        ) -> Result<Option<ActingSender>, PlatformError> {
            self.record(PlatformCall::Message {
                channel: channel.to_owned(),
                text: text.to_owned(),
                sender,
            })
            .await
        }
        async fn send_reply_as(
            &self,
            channel: &str,
            reply_parent_message_id: &str,
            text: &str,
            sender: SenderPreference,
        ) -> Result<Option<ActingSender>, PlatformError> {
            self.record(PlatformCall::Reply {
                channel: channel.to_owned(),
                parent: reply_parent_message_id.to_owned(),
                text: text.to_owned(),
                sender,
            })
            .await
        }
        async fn send_whisper_as(
            &self,
            recipient_login: &str,
            text: &str,
            sender: SenderPreference,
        ) -> Result<Option<ActingSender>, PlatformError> {
            self.record(PlatformCall::Whisper {
                recipient: recipient_login.to_owned(),
                text: text.to_owned(),
                sender,
            })
            .await
        }
        fn events(&self) -> EventSubscription {
            EventChannel::new(1).subscribe()
        }
    }

    fn mock(spec: MockSpec) -> (Arc<MockPlatform>, mpsc::UnboundedReceiver<PlatformCall>) {
        let (calls, rx) = mpsc::unbounded_channel();
        (Arc::new(MockPlatform { calls, spec }), rx)
    }

    fn bus() -> Arc<EventBus> {
        EventBus::new(Arc::new(NullEventLogRepo))
    }

    async fn route(
        bus: &Arc<EventBus>,
        platform: Arc<MockPlatform>,
        platform_id: PlatformId,
    ) -> TaskGroup {
        let router = spawn_chat_send_router(Arc::clone(bus), platform, platform_id);
        tokio::task::yield_now().await;
        router
    }

    fn request(source: EventSource, payload: serde_json::Value) -> Event {
        Event::new(source, ChatSendRequest::KIND, payload)
    }

    fn rhai(payload: serde_json::Value) -> Event {
        request(EventSource::Rhai, payload)
    }

    fn plain(channel: &str, text: &str) -> PlatformCall {
        PlatformCall::Message {
            channel: channel.to_owned(),
            text: text.to_owned(),
            sender: SenderPreference::Default,
        }
    }

    fn bot_acting() -> ActingSender {
        ActingSender {
            role: SenderRole::Bot,
            login: "forgebot".to_owned(),
        }
    }

    async fn next_call(rx: &mut mpsc::UnboundedReceiver<PlatformCall>) -> PlatformCall {
        tokio::time::timeout(ARRIVAL, rx.recv())
            .await
            .expect("a platform send call was expected but never arrived")
            .expect("mock send channel closed")
    }

    async fn next_of_kind(sub: &mut EventSubscription, kind: &str) -> Event {
        tokio::time::timeout(ARRIVAL, async {
            loop {
                let event = sub.recv().await.expect("observer subscription failed");
                if event.kind == kind {
                    return event;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("no {kind} event was published"))
    }

    fn drain_of_kind(sub: &mut EventSubscription, kind: &str) -> Vec<Event> {
        let mut seen = Vec::new();
        while let Some(event) = sub.try_recv().expect("observer subscription failed") {
            if event.kind == kind {
                seen.push(event);
            }
        }
        seen
    }

    fn assert_matches_golden(events: &[Event], fixture: &str) {
        let emitted: serde_json::Value = events
            .iter()
            .map(|event| json!({ "source": event.source, "payload": event.payload }))
            .collect();
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/observability")
            .join(fixture);
        let golden: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(emitted, golden, "{fixture} drifted");
    }

    #[tokio::test]
    async fn broadcast_request_reaches_every_platform_router() {
        let bus = bus();
        let (twitch, mut twitch_rx) = mock(MockSpec::default());
        let (kick, mut kick_rx) = mock(MockSpec::default());
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;
        let _kick = route(&bus, kick, PlatformId::Kick).await;

        bus.publish(rhai(json!({ "message": "hello" })));

        assert_eq!(
            (
                next_call(&mut twitch_rx).await,
                next_call(&mut kick_rx).await
            ),
            (plain("twitch", "hello"), plain("kick", "hello"))
        );
    }

    #[tokio::test]
    async fn blank_target_request_reaches_every_platform_router() {
        for blank in ["", "   "] {
            let bus = bus();
            let (twitch, mut twitch_rx) = mock(MockSpec::default());
            let (kick, mut kick_rx) = mock(MockSpec::default());
            let _twitch = route(&bus, twitch, PlatformId::Twitch).await;
            let _kick = route(&bus, kick, PlatformId::Kick).await;

            bus.publish(rhai(json!({ "target": blank, "message": "everyone" })));

            assert_eq!(
                (
                    next_call(&mut twitch_rx).await,
                    next_call(&mut kick_rx).await
                ),
                (plain("twitch", "everyone"), plain("kick", "everyone")),
                "target {blank:?}"
            );
        }
    }

    #[tokio::test]
    async fn targeted_request_routes_only_to_the_named_platform_in_any_case_or_padding() {
        for target in ["twitch", " twitch ", "Twitch", "TWITCH"] {
            let bus = bus();
            let (twitch, mut twitch_rx) = mock(MockSpec::default());
            let (kick, mut kick_rx) = mock(MockSpec::default());
            let _twitch = route(&bus, twitch, PlatformId::Twitch).await;
            let _kick = route(&bus, kick, PlatformId::Kick).await;

            bus.publish(rhai(json!({ "target": target, "message": "for-twitch" })));
            bus.publish(rhai(json!({ "target": "kick", "message": "sentinel" })));

            assert_eq!(
                (
                    next_call(&mut twitch_rx).await,
                    next_call(&mut kick_rx).await
                ),
                (plain("twitch", "for-twitch"), plain("kick", "sentinel")),
                "target {target:?}"
            );
        }
    }

    #[tokio::test]
    async fn request_targeted_at_a_non_platform_word_reaches_no_router() {
        let bus = bus();
        let (twitch, mut twitch_rx) = mock(MockSpec::default());
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;

        for target in ["discord", "obs", "twitch.tv"] {
            bus.publish(rhai(json!({ "target": target, "message": "nowhere" })));
        }
        bus.publish(rhai(json!({ "message": "sentinel" })));

        assert_eq!(next_call(&mut twitch_rx).await, plain("twitch", "sentinel"));
    }

    #[tokio::test]
    async fn only_core_and_rhai_requests_are_routed() {
        let bus = bus();
        let (twitch, mut twitch_rx) = mock(MockSpec::default());
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;

        for (source, message) in [
            (EventSource::Twitch, "loop-risk"),
            (EventSource::Server, "external"),
            (EventSource::Http, "external"),
            (EventSource::Core, "from-core"),
            (EventSource::Rhai, "from-rhai"),
        ] {
            bus.publish(request(
                source,
                json!({ "target": "twitch", "message": message }),
            ));
        }

        assert_eq!(
            (
                next_call(&mut twitch_rx).await,
                next_call(&mut twitch_rx).await
            ),
            (plain("twitch", "from-core"), plain("twitch", "from-rhai"))
        );
    }

    #[tokio::test]
    async fn successful_send_publishes_chat_send_from_the_platform_source_caused_by_the_request() {
        for platform_id in PlatformId::ALL {
            let bus = bus();
            let mut observer = bus.subscribe();
            let (platform, _rx) = mock(MockSpec::default());
            let _router = route(&bus, platform, platform_id).await;
            let req = rhai(json!({ "message": "hi" }));
            let req_id = req.id;

            bus.publish(req);

            let sent = next_of_kind(&mut observer, ChatSent::KIND).await;
            assert_eq!(
                (sent.source, sent.caused_by, ChatSent::from_event(&sent)),
                (
                    EventSource::from(platform_id),
                    Some(req_id),
                    Some(ChatSent {
                        channel: platform_id.as_str().to_owned(),
                        message: "hi".to_owned(),
                        reply_to_message_id: None,
                        sent_as: None,
                    })
                ),
                "{platform_id:?}"
            );
        }
    }

    #[tokio::test]
    async fn platform_send_error_publishes_failure_with_its_text_caused_by_the_request() {
        let bus = bus();
        let mut observer = bus.subscribe();
        let (twitch, _rx) = mock(MockSpec {
            failure: Some("msg_duplicate: identical message"),
            ..MockSpec::default()
        });
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;
        let req = rhai(json!({ "message": "hi" }));
        let req_id = req.id;

        bus.publish(req);

        let failed = next_of_kind(&mut observer, ChatSendFailed::KIND).await;
        assert_eq!(
            (failed.caused_by, ChatSendFailed::from_event(&failed)),
            (
                Some(req_id),
                Some(ChatSendFailed {
                    channel: "twitch".to_owned(),
                    error: "network failure: msg_duplicate: identical message".to_owned(),
                })
            )
        );
    }

    #[tokio::test]
    async fn requests_are_sent_one_at_a_time_in_publish_order() {
        let gate = Arc::new(Semaphore::new(0));
        let bus = bus();
        let (twitch, mut twitch_rx) = mock(MockSpec {
            gate: Some(Arc::clone(&gate)),
            ..MockSpec::default()
        });
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;

        for message in ["first", "second", "third"] {
            bus.publish(rhai(json!({ "message": message })));
        }
        assert_eq!(next_call(&mut twitch_rx).await, plain("twitch", "first"));
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        assert!(
            twitch_rx.try_recv().is_err(),
            "a second send started while the first was still in flight"
        );
        gate.add_permits(3);

        assert_eq!(
            (
                next_call(&mut twitch_rx).await,
                next_call(&mut twitch_rx).await
            ),
            (plain("twitch", "second"), plain("twitch", "third"))
        );
    }

    #[tokio::test]
    async fn blocked_send_queues_up_to_capacity_and_fails_only_the_first_overflow_request() {
        let bus = bus();
        let mut observer = bus.subscribe();
        let (twitch, mut twitch_rx) = mock(MockSpec {
            gate: Some(Arc::new(Semaphore::new(0))),
            ..MockSpec::default()
        });
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;
        bus.publish(rhai(json!({ "message": "in-flight" })));
        next_call(&mut twitch_rx).await;

        for n in 0..CHAT_SEND_QUEUE_CAPACITY {
            bus.publish(rhai(json!({ "message": format!("queued-{n}") })));
        }
        let overflow = rhai(json!({ "message": "overflow" }));
        let overflow_id = overflow.id;
        bus.publish(overflow);

        let failed = next_of_kind(&mut observer, ChatSendFailed::KIND).await;
        assert_eq!(
            failed.caused_by,
            Some(overflow_id),
            "the reader must keep accepting while a send blocks, rejecting only past capacity"
        );
    }

    #[tokio::test]
    async fn lagged_bus_reader_publishes_one_uncaused_failure_naming_the_lag_and_keeps_serving() {
        const OBSERVER_CAPACITY: usize = 64;
        const FLOOD: usize = OBSERVER_CAPACITY * 2;
        let config = Config {
            bus_observer_capacity: OBSERVER_CAPACITY,
            ..Config::default()
        };
        let bus = EventBus::with_config(Arc::new(NullEventLogRepo), &config);
        let (twitch, mut twitch_rx) = mock(MockSpec::default());
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;
        for _ in 0..FLOOD {
            bus.publish(Event::new(
                EventSource::Core,
                "noise",
                serde_json::Value::Null,
            ));
        }
        let mut observer = bus.subscribe();

        bus.publish(rhai(json!({ "message": "after-lag" })));
        assert_eq!(
            next_call(&mut twitch_rx).await,
            plain("twitch", "after-lag")
        );

        let failures: Vec<_> = drain_of_kind(&mut observer, ChatSendFailed::KIND)
            .iter()
            .map(|event| {
                (
                    event.caused_by,
                    ChatSendFailed::from_event(event).is_some_and(|f| f.error.contains("lagging")),
                )
            })
            .collect();
        assert_eq!(failures, vec![(None, true)]);
    }

    #[tokio::test]
    async fn whisper_request_calls_send_whisper_with_the_trimmed_recipient() {
        let bus = bus();
        let (twitch, mut twitch_rx) = mock(MockSpec::default());
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;

        bus.publish(rhai(
            json!({ "message": "psst", "whisper_to_login": " viewer " }),
        ));

        assert_eq!(
            next_call(&mut twitch_rx).await,
            PlatformCall::Whisper {
                recipient: "viewer".to_owned(),
                text: "psst".to_owned(),
                sender: SenderPreference::Default,
            }
        );
    }

    #[tokio::test]
    async fn null_whisper_recipient_is_sent_as_a_plain_message() {
        let bus = bus();
        let (twitch, mut twitch_rx) = mock(MockSpec::default());
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;

        bus.publish(rhai(
            json!({ "message": "hello", "whisper_to_login": null }),
        ));

        assert_eq!(next_call(&mut twitch_rx).await, plain("twitch", "hello"));
    }

    #[tokio::test]
    async fn failed_whisper_is_never_retried_as_a_public_message() {
        let bus = bus();
        let (twitch, mut twitch_rx) = mock(MockSpec {
            failure: Some("missing whisper scope"),
            ..MockSpec::default()
        });
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;

        bus.publish(rhai(json!({
            "target": "twitch",
            "message": "psst",
            "whisper_to_login": "viewer",
        })));
        bus.publish(rhai(json!({ "message": "sentinel" })));

        let first = next_call(&mut twitch_rx).await;
        assert_eq!(
            (
                matches!(first, PlatformCall::Whisper { .. }),
                next_call(&mut twitch_rx).await
            ),
            (true, plain("twitch", "sentinel")),
            "a failed whisper must not be retried as a public message"
        );
    }

    #[tokio::test]
    async fn targeted_whisper_on_a_platform_without_whispers_fails_unsent() {
        let bus = bus();
        let mut observer = bus.subscribe();
        let (kick, mut kick_rx) = mock(MockSpec::default());
        let _kick = route(&bus, kick, PlatformId::Kick).await;
        let req = rhai(json!({
            "target": "kick",
            "message": "psst",
            "whisper_to_login": "viewer",
        }));
        let req_id = req.id;

        bus.publish(req);
        bus.publish(rhai(json!({ "message": "sentinel" })));

        let failed = next_of_kind(&mut observer, ChatSendFailed::KIND).await;
        assert_eq!(
            (
                failed.caused_by,
                ChatSendFailed::from_event(&failed).map(|f| f.error),
                next_call(&mut kick_rx).await,
            ),
            (
                Some(req_id),
                Some(whispers_unsupported_reason("kick")),
                plain("kick", "sentinel"),
            )
        );
    }

    #[tokio::test]
    async fn broadcast_whisper_goes_to_twitch_while_kick_stays_silent_without_failing() {
        let bus = bus();
        let mut observer = bus.subscribe();
        let (twitch, mut twitch_rx) = mock(MockSpec::default());
        let (kick, mut kick_rx) = mock(MockSpec::default());
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;
        let _kick = route(&bus, kick, PlatformId::Kick).await;

        bus.publish(rhai(
            json!({ "message": "psst", "whisper_to_login": "viewer" }),
        ));
        bus.publish(rhai(json!({ "target": "kick", "message": "sentinel" })));

        let twitch_call = next_call(&mut twitch_rx).await;
        let kick_call = next_call(&mut kick_rx).await;
        assert_eq!(
            (
                matches!(twitch_call, PlatformCall::Whisper { .. }),
                kick_call,
                drain_of_kind(&mut observer, ChatSendFailed::KIND).len(),
            ),
            (true, plain("kick", "sentinel"), 0)
        );
    }

    #[tokio::test]
    async fn delivered_whisper_publishes_whisper_sent_without_the_message_text() {
        let bus = bus();
        let mut observer = bus.subscribe();
        let (twitch, _rx) = mock(MockSpec::default());
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;
        let req = rhai(json!({
            "message": "WHISPER_TEXT_SENTINEL",
            "whisper_to_login": "viewer",
        }));
        let req_id = req.id;

        bus.publish(req);

        let sent = next_of_kind(&mut observer, WhisperSent::KIND).await;
        assert_eq!(
            (
                sent.source,
                sent.caused_by,
                sent.payload.to_string().contains("WHISPER_TEXT_SENTINEL"),
                WhisperSent::from_event(&sent),
            ),
            (
                EventSource::Twitch,
                Some(req_id),
                false,
                Some(WhisperSent {
                    channel: "twitch".to_owned(),
                    whisper_to_login: "viewer".to_owned(),
                    sent_as: None,
                })
            )
        );
    }

    #[tokio::test]
    async fn reply_request_calls_send_reply_with_the_trimmed_parent_id() {
        let bus = bus();
        let (twitch, mut twitch_rx) = mock(MockSpec::default());
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;

        bus.publish(rhai(
            json!({ "message": "hello", "reply_to_message_id": " msg-1 " }),
        ));

        assert_eq!(
            next_call(&mut twitch_rx).await,
            PlatformCall::Reply {
                channel: "twitch".to_owned(),
                parent: "msg-1".to_owned(),
                text: "hello".to_owned(),
                sender: SenderPreference::Default,
            }
        );
    }

    #[tokio::test]
    async fn reply_request_with_a_blank_or_null_parent_is_sent_as_a_plain_message() {
        let bus = bus();
        let (twitch, mut twitch_rx) = mock(MockSpec::default());
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;

        for parent in [json!(""), json!("  "), json!(null)] {
            bus.publish(rhai(
                json!({ "message": "hello", "reply_to_message_id": parent }),
            ));

            assert_eq!(
                next_call(&mut twitch_rx).await,
                plain("twitch", "hello"),
                "parent {parent}"
            );
        }
    }

    #[tokio::test]
    async fn request_carrying_both_reply_and_whisper_keys_is_sent_as_a_whisper() {
        let bus = bus();
        let (twitch, mut twitch_rx) = mock(MockSpec::default());
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;

        bus.publish(rhai(json!({
            "message": "psst",
            "reply_to_message_id": "msg-1",
            "whisper_to_login": "viewer",
        })));

        assert!(matches!(
            next_call(&mut twitch_rx).await,
            PlatformCall::Whisper { .. }
        ));
    }

    #[tokio::test]
    async fn each_send_kind_hands_the_requested_sender_to_the_platform() {
        let bus = bus();
        let (twitch, mut twitch_rx) = mock(MockSpec::default());
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;

        for (payload, expected) in [
            (
                json!({ "message": "m", "sender": null }),
                plain("twitch", "m"),
            ),
            (
                json!({ "message": "m", "sender": "default" }),
                plain("twitch", "m"),
            ),
            (
                json!({ "message": "m", "sender": "streamer" }),
                PlatformCall::Message {
                    channel: "twitch".to_owned(),
                    text: "m".to_owned(),
                    sender: SenderPreference::Streamer,
                },
            ),
            (
                json!({ "message": "m", "reply_to_message_id": "msg-1", "sender": "bot" }),
                PlatformCall::Reply {
                    channel: "twitch".to_owned(),
                    parent: "msg-1".to_owned(),
                    text: "m".to_owned(),
                    sender: SenderPreference::BotOnly,
                },
            ),
            (
                json!({ "message": "m", "whisper_to_login": "viewer", "sender": "streamer" }),
                PlatformCall::Whisper {
                    recipient: "viewer".to_owned(),
                    text: "m".to_owned(),
                    sender: SenderPreference::Streamer,
                },
            ),
        ] {
            bus.publish(rhai(payload.clone()));

            assert_eq!(next_call(&mut twitch_rx).await, expected, "{payload}");
        }
    }

    #[tokio::test]
    async fn unknown_sender_fails_the_request_unsent_naming_the_value() {
        let bus = bus();
        let mut observer = bus.subscribe();
        let (twitch, mut twitch_rx) = mock(MockSpec::default());
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;
        let req = rhai(json!({ "message": "hi", "sender": "Bot" }));
        let req_id = req.id;

        bus.publish(req);
        bus.publish(rhai(json!({ "message": "sentinel" })));

        let failed = next_of_kind(&mut observer, ChatSendFailed::KIND).await;
        assert_eq!(
            (
                failed.caused_by,
                ChatSendFailed::from_event(&failed).map(|f| f.error),
                next_call(&mut twitch_rx).await,
            ),
            (
                Some(req_id),
                Some(unknown_sender_reason("Bot")),
                plain("twitch", "sentinel"),
            )
        );
    }

    #[tokio::test]
    async fn request_with_a_malformed_field_is_dropped_without_sending_or_failing() {
        let bus = bus();
        let mut observer = bus.subscribe();
        let (twitch, mut twitch_rx) = mock(MockSpec::default());
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;

        for malformed in [
            serde_json::Value::Null,
            json!({}),
            json!({ "message": 7 }),
            json!({ "message": "m", "sender": 7 }),
            json!({ "message": "m", "target": 7 }),
            json!({ "message": "m", "whisper_to_login": 7 }),
            json!({ "message": "m", "reply_to_message_id": ["msg-1"] }),
        ] {
            bus.publish(rhai(malformed));
        }
        bus.publish(rhai(json!({ "message": "sentinel" })));

        assert_eq!(next_call(&mut twitch_rx).await, plain("twitch", "sentinel"));
        next_of_kind(&mut observer, ChatSent::KIND).await;
        assert_eq!(drain_of_kind(&mut observer, ChatSendFailed::KIND).len(), 0);
    }

    #[tokio::test]
    async fn every_outcome_event_records_the_identity_the_platform_says_acted() {
        let bus = bus();
        let mut observer = bus.subscribe();
        let (twitch, _rx) = mock(MockSpec {
            acting: Some(bot_acting()),
            ..MockSpec::default()
        });
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;

        bus.publish(rhai(json!({ "message": "plain" })));
        let plain_sent = next_of_kind(&mut observer, ChatSent::KIND).await;
        bus.publish(rhai(
            json!({ "message": "reply", "reply_to_message_id": "msg-1" }),
        ));
        let reply_sent = next_of_kind(&mut observer, ChatSent::KIND).await;
        bus.publish(rhai(
            json!({ "message": "psst", "whisper_to_login": "viewer" }),
        ));
        let whisper_sent = next_of_kind(&mut observer, WhisperSent::KIND).await;

        assert_eq!(
            (
                ChatSent::from_event(&plain_sent).and_then(|e| e.sent_as),
                ChatSent::from_event(&reply_sent).and_then(|e| e.sent_as),
                WhisperSent::from_event(&whisper_sent).and_then(|e| e.sent_as),
            ),
            (Some(bot_acting()), Some(bot_acting()), Some(bot_acting()))
        );
    }

    #[tokio::test]
    async fn aborting_the_router_tasks_releases_the_platform() {
        let bus = bus();
        let (twitch, mut twitch_rx) = mock(MockSpec::default());
        let router = route(&bus, twitch, PlatformId::Twitch).await;

        router.abort_all();

        assert_eq!(
            tokio::time::timeout(ARRIVAL, twitch_rx.recv())
                .await
                .expect("an aborted router must drop its platform handle"),
            None
        );
    }

    #[tokio::test]
    async fn chat_sent_payloads_match_the_golden() {
        let bus = bus();
        let mut observer = bus.subscribe();
        let (twitch, _twitch_rx) = mock(MockSpec {
            acting: Some(bot_acting()),
            ..MockSpec::default()
        });
        let (kick, _kick_rx) = mock(MockSpec::default());
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;
        let _kick = route(&bus, kick, PlatformId::Kick).await;

        let mut sent = Vec::new();
        for payload in [
            json!({ "message": "golden plain", "target": "twitch" }),
            json!({ "message": "golden reply", "target": "twitch", "reply_to_message_id": "msg-1" }),
            json!({ "message": "golden kick", "target": "kick" }),
        ] {
            bus.publish(rhai(payload));
            sent.push(next_of_kind(&mut observer, ChatSent::KIND).await);
        }

        assert_matches_golden(&sent, "chat.send.json");
    }

    #[tokio::test]
    async fn chat_send_failed_payloads_match_the_golden() {
        let bus = bus();
        let mut observer = bus.subscribe();
        let (twitch, _rx) = mock(MockSpec {
            failure: Some("msg_duplicate: identical message"),
            ..MockSpec::default()
        });
        let _twitch = route(&bus, twitch, PlatformId::Twitch).await;

        let mut failed = Vec::new();
        for payload in [
            json!({ "message": "golden", "sender": "Bot" }),
            json!({ "message": "golden" }),
        ] {
            bus.publish(rhai(payload));
            failed.push(next_of_kind(&mut observer, ChatSendFailed::KIND).await);
        }

        assert_matches_golden(&failed, "chat.send.failed.json");
    }
}
