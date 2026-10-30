use std::sync::Arc;

use forge_events::{
    ActingSender, ChatSendFailed, ChatSendRequest, ChatSent, Event, EventSource, EventsError,
    TypedEvent, WhisperSent,
};
use forge_platform_core::{ChatPlatform, SenderPreference, unknown_sender_reason};
use forge_types::{EventId, PlatformId, requested_chat_target, whispers_unsupported_reason};
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
