use std::sync::{Arc, Mutex, MutexGuard};

use forge_events::{Event, EventPublisher};
use forge_platform_core::{ConnectionState, ConnectionTransition, connection_state_changed_event};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::chat::{ChatLinkStatus, KickChatHandle};
use crate::event_channel::PlatformEventChannel;

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
    events: Arc<PlatformEventChannel>,
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
    pub(crate) fn new(events: Arc<PlatformEventChannel>) -> Self {
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
