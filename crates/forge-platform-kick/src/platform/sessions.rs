use std::sync::{Arc, Mutex, MutexGuard};

use forge_events::{Event, EventPublisher};
use forge_platform_core::{ConnectionState, ConnectionTransition, connection_state_changed_event};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::pusher::chat::{ChatLinkStatus, KickChatHandle};
use forge_events::EventChannel;

pub(crate) const PLATFORM_ID: &str = "kick";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionGeneration(u64);

impl SessionGeneration {
    fn successor(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

struct LiveSession {
    cancel: CancellationToken,
    handle: Option<KickChatHandle>,
}

impl LiveSession {
    async fn stop(self) {
        self.cancel.cancel();
        if let Some(handle) = self.handle {
            handle.stop().await;
        }
    }
}

struct SessionSlot {
    live: SessionGeneration,
    session: Option<LiveSession>,
}

impl SessionSlot {
    fn retire(&mut self) -> (SessionGeneration, Option<LiveSession>) {
        self.live = self.live.successor();
        (self.live, self.session.take())
    }

    fn is_live(&self, generation: SessionGeneration) -> bool {
        self.live == generation
    }
}

#[derive(Clone)]
struct SessionOutlet {
    slot: Arc<Mutex<SessionSlot>>,
    events: Arc<EventChannel>,
    state_tx: watch::Sender<ConnectionState>,
}

impl SessionOutlet {
    fn lock(&self) -> MutexGuard<'_, SessionSlot> {
        self.slot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn announce(&self, status: ChatLinkStatus) {
        self.events.publish(connection_state_changed_event(
            PLATFORM_ID,
            status.state,
            status.transition,
        ));
        self.state_tx.send_replace(status.state);
    }
}

pub(crate) struct SessionTicket {
    generation: SessionGeneration,
    cancel: CancellationToken,
}

impl SessionTicket {
    pub(crate) fn cancellation(&self) -> CancellationToken {
        self.cancel.clone()
    }
}

#[derive(Clone)]
pub(crate) struct SessionPublisher {
    outlet: SessionOutlet,
    generation: SessionGeneration,
}

impl SessionPublisher {
    pub(crate) fn publish_event(&self, event: Event) -> bool {
        let slot = self.outlet.lock();
        if !slot.is_live(self.generation) {
            return false;
        }
        self.outlet.events.publish(event);
        true
    }

    pub(crate) fn publish_status(&self, status: ChatLinkStatus) -> bool {
        let slot = self.outlet.lock();
        if !slot.is_live(self.generation) {
            return false;
        }
        self.outlet.announce(status);
        true
    }
}

pub(crate) struct ChatSessions {
    outlet: SessionOutlet,
}

impl ChatSessions {
    pub(crate) fn new(events: Arc<EventChannel>) -> Self {
        let (state_tx, _) = watch::channel(ConnectionState::Disconnected);
        Self {
            outlet: SessionOutlet {
                slot: Arc::new(Mutex::new(SessionSlot {
                    live: SessionGeneration(0),
                    session: None,
                })),
                events,
                state_tx,
            },
        }
    }

    pub(crate) fn state_receiver(&self) -> watch::Receiver<ConnectionState> {
        self.outlet.state_tx.subscribe()
    }

    pub(crate) fn connection_state(&self) -> ConnectionState {
        match &self.outlet.lock().session {
            Some(LiveSession {
                handle: Some(handle),
                ..
            }) => handle.connection_state(),
            Some(LiveSession { handle: None, .. }) => ConnectionState::Connecting,
            None => *self.outlet.state_tx.borrow(),
        }
    }

    pub(crate) async fn begin(&self) -> SessionTicket {
        let cancel = CancellationToken::new();
        let (generation, previous) = {
            let mut slot = self.outlet.lock();
            let retired = slot.retire();
            slot.session = Some(LiveSession {
                cancel: cancel.clone(),
                handle: None,
            });
            retired
        };
        if let Some(previous) = previous {
            previous.stop().await;
        }
        SessionTicket { generation, cancel }
    }

    pub(crate) fn publisher(&self, ticket: &SessionTicket) -> SessionPublisher {
        SessionPublisher {
            outlet: self.outlet.clone(),
            generation: ticket.generation,
        }
    }

    pub(crate) async fn install(&self, ticket: SessionTicket, handle: KickChatHandle) {
        let superseded = {
            let mut slot = self.outlet.lock();
            if slot.is_live(ticket.generation)
                && let Some(session) = slot.session.as_mut()
            {
                session.handle = Some(handle);
                None
            } else {
                Some(handle)
            }
        };
        if let Some(handle) = superseded {
            handle.stop().await;
        }
    }

    pub(crate) fn abandon(&self, ticket: &SessionTicket) {
        let mut slot = self.outlet.lock();
        if !slot.is_live(ticket.generation) {
            return;
        }
        slot.session = None;
        self.settle_disconnected(ConnectionTransition::Unexpected);
    }

    pub(crate) async fn end(&self) {
        let (generation, previous) = self.outlet.lock().retire();
        if let Some(previous) = previous {
            previous.stop().await;
        }
        let slot = self.outlet.lock();
        if slot.is_live(generation) {
            self.settle_disconnected(ConnectionTransition::Expected);
        }
    }

    fn settle_disconnected(&self, transition: ConnectionTransition) {
        if *self.outlet.state_tx.borrow() == ConnectionState::Disconnected {
            return;
        }
        self.outlet.announce(ChatLinkStatus {
            state: ConnectionState::Disconnected,
            transition,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use forge_events::{EventChannel, EventSource, EventSubscription};
    use forge_events::{PlatformConnectionChanged, TypedEvent};
    use futures::FutureExt;

    use super::*;
    use crate::pusher::chat::tests::handle_reporting;

    fn sessions() -> (ChatSessions, EventSubscription) {
        let events = Arc::new(EventChannel::new(crate::platform::EVENT_CHANNEL_CAPACITY));
        let stream = events.subscribe();
        (ChatSessions::new(events), stream)
    }

    fn status(state: ConnectionState) -> ChatLinkStatus {
        ChatLinkStatus {
            state,
            transition: ConnectionTransition::Unexpected,
        }
    }

    fn chat_event() -> Event {
        Event::new(
            EventSource::Kick,
            "kick.chat.message.sent",
            serde_json::Value::Null,
        )
    }

    fn drain(stream: &mut EventSubscription) -> Vec<Event> {
        let mut published = Vec::new();
        while let Some(Ok(event)) = stream.recv().now_or_never() {
            published.push(event);
        }
        published
    }

    fn link_announcements(stream: &mut EventSubscription) -> Vec<(String, Option<bool>)> {
        drain(stream)
            .into_iter()
            .filter(|event| event.kind == PlatformConnectionChanged::KIND)
            .map(|event| {
                (
                    event.payload["state"].as_str().unwrap().to_owned(),
                    event.payload["expected"].as_bool(),
                )
            })
            .collect()
    }

    #[derive(Debug, Clone, Copy)]
    enum Retirement {
        NewerBegin,
        End,
    }

    #[tokio::test]
    async fn a_retired_session_publishes_neither_events_nor_status() {
        for retirement in [Retirement::NewerBegin, Retirement::End] {
            let (sessions, mut stream) = sessions();
            let retired = sessions.publisher(&sessions.begin().await);
            match retirement {
                Retirement::NewerBegin => drop(sessions.begin().await),
                Retirement::End => sessions.end().await,
            }

            let accepted = (
                retired.publish_event(chat_event()),
                retired.publish_status(status(ConnectionState::Connected)),
            );

            assert_eq!(
                (
                    accepted,
                    drain(&mut stream).len(),
                    *sessions.state_receiver().borrow()
                ),
                ((false, false), 0, ConnectionState::Disconnected),
                "{retirement:?}"
            );
        }
    }

    #[tokio::test]
    async fn the_live_session_publishes_its_events_and_status() {
        let (sessions, mut stream) = sessions();
        let live = sessions.publisher(&sessions.begin().await);
        let event = chat_event();
        let event_id = event.id;

        let accepted = (
            live.publish_event(event),
            live.publish_status(status(ConnectionState::Connected)),
        );

        let published: Vec<_> = drain(&mut stream).into_iter().map(|e| e.id).collect();
        assert_eq!(
            (
                accepted,
                published.first().copied(),
                published.len(),
                *sessions.state_receiver().borrow()
            ),
            ((true, true), Some(event_id), 2, ConnectionState::Connected)
        );
    }

    #[tokio::test]
    async fn installing_a_superseded_connect_stops_its_handle_instead_of_installing_it() {
        let (sessions, _stream) = sessions();
        let superseded = sessions.begin().await;
        drop(sessions.begin().await);
        let handle_cancel = CancellationToken::new();
        let mut handle = handle_reporting(ConnectionState::Connected, handle_cancel.clone());
        let joined = Arc::new(AtomicBool::new(false));
        let joined_flag = Arc::clone(&joined);
        handle.attach(async move {
            handle_cancel.cancelled().await;
            joined_flag.store(true, Ordering::SeqCst);
        });

        sessions.install(superseded, handle).await;

        assert_eq!(
            (joined.load(Ordering::SeqCst), sessions.connection_state()),
            (true, ConnectionState::Connecting)
        );
    }

    #[tokio::test]
    async fn abandoning_a_superseded_connect_leaves_the_newer_session_in_place() {
        let (sessions, mut stream) = sessions();
        let superseded = sessions.begin().await;
        drop(sessions.begin().await);

        sessions.abandon(&superseded);

        assert_eq!(
            (sessions.connection_state(), drain(&mut stream).len()),
            (ConnectionState::Connecting, 0)
        );
    }

    #[tokio::test]
    async fn a_failed_reconnect_after_a_live_link_settles_disconnected_unexpected() {
        let (sessions, mut stream) = sessions();
        let previous = sessions.publisher(&sessions.begin().await);
        previous.publish_status(status(ConnectionState::Connected));
        let failing = sessions.begin().await;
        drain(&mut stream);

        sessions.abandon(&failing);

        assert_eq!(
            (link_announcements(&mut stream), sessions.connection_state()),
            (
                vec![("disconnected".to_owned(), Some(false))],
                ConnectionState::Disconnected
            )
        );
    }

    #[tokio::test]
    async fn ending_twice_announces_disconnected_expected_only_once() {
        let (sessions, mut stream) = sessions();
        let live = sessions.publisher(&sessions.begin().await);
        live.publish_status(status(ConnectionState::Connected));
        drain(&mut stream);

        sessions.end().await;
        sessions.end().await;

        assert_eq!(
            link_announcements(&mut stream),
            vec![("disconnected".to_owned(), Some(true))]
        );
    }
}
