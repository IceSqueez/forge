use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use tokio::sync::{mpsc, watch};

use super::config::FakeKickConfig;
use super::ledger::{KickLedger, KickRequest, PusherSession};

pub(crate) type Outbox = mpsc::Sender<String>;

pub(crate) struct Tokens {
    pub(crate) access: String,
    pub(crate) refresh: String,
    pub(crate) rotations: u32,
}

pub(crate) struct Channel {
    pub(crate) live: bool,
    pub(crate) viewer_count: u64,
}

pub(crate) struct Inner {
    pub(crate) ledger: KickLedger,
    pub(crate) tokens: Tokens,
    pub(crate) channel: Channel,
    outboxes: HashMap<u64, Outbox>,
    next_session: u64,
}

impl Inner {
    fn new(config: &FakeKickConfig) -> Self {
        Self {
            ledger: KickLedger::default(),
            tokens: Tokens {
                access: config.access_token.clone(),
                refresh: config.refresh_token.clone(),
                rotations: 0,
            },
            channel: Channel {
                live: config.live,
                viewer_count: config.viewer_count,
            },
            outboxes: HashMap::new(),
            next_session: 1,
        }
    }

    pub(crate) fn open_session(&mut self, app_key: String, outbox: Option<Outbox>) -> u64 {
        let id = self.next_session;
        self.next_session += 1;
        let live = outbox.is_some();
        if let Some(outbox) = outbox {
            self.outboxes.insert(id, outbox);
        }
        self.ledger.sessions.push(PusherSession {
            id,
            app_key,
            subscriptions: Vec::new(),
            live,
        });
        id
    }

    pub(crate) fn subscribe(&mut self, session_id: u64, channel: &str) {
        if let Some(session) = self.session_mut(session_id)
            && !session.subscriptions.iter().any(|held| held == channel)
        {
            session.subscriptions.push(channel.to_owned());
        }
    }

    pub(crate) fn unsubscribe(&mut self, session_id: u64, channel: &str) {
        if let Some(session) = self.session_mut(session_id) {
            session.subscriptions.retain(|held| held != channel);
        }
    }

    pub(crate) fn close_session(&mut self, session_id: u64) {
        self.outboxes.remove(&session_id);
        if let Some(session) = self.session_mut(session_id) {
            session.live = false;
        }
    }

    pub(crate) fn deliveries(&self, channel: &str) -> Vec<Outbox> {
        self.ledger
            .live_sessions()
            .filter(|session| session.subscriptions.iter().any(|held| held == channel))
            .filter_map(|session| self.outboxes.get(&session.id).cloned())
            .collect()
    }

    pub(crate) fn record_request(&mut self, request: KickRequest) {
        self.ledger.requests.push(request);
    }

    fn session_mut(&mut self, session_id: u64) -> Option<&mut PusherSession> {
        self.ledger
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
    }
}

pub(crate) struct Shared {
    config: FakeKickConfig,
    inner: Mutex<Inner>,
    changes: watch::Sender<u64>,
}

impl Shared {
    pub(crate) fn new(config: FakeKickConfig) -> Self {
        let inner = Inner::new(&config);
        Self {
            config,
            inner: Mutex::new(inner),
            changes: watch::Sender::new(0),
        }
    }

    pub(crate) fn config(&self) -> &FakeKickConfig {
        &self.config
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn read<R>(&self, f: impl FnOnce(&Inner) -> R) -> R {
        f(&self.lock())
    }

    pub(crate) fn mutate<R>(&self, f: impl FnOnce(&mut Inner) -> R) -> R {
        let result = f(&mut self.lock());
        self.changes.send_modify(|generation| *generation += 1);
        result
    }

    pub(crate) fn changes(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }
}
