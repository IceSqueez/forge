use std::collections::HashMap;
use std::sync::Arc;

use forge_types::QueueId;
use tokio::sync::watch;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QueueDepth {
    pub pending: usize,
    pub in_flight: usize,
    /// Reset to zero whenever the queue enters a different mode.
    pub overflowed: u64,
}

pub type QueueDepths = HashMap<QueueId, QueueDepth>;

#[derive(Default)]
struct Board {
    depths: QueueDepths,
    epochs: HashMap<QueueId, u64>,
    next_epoch: u64,
}

/// Yields the latest depth of every registered queue; bursts between reads coalesce into one.
pub struct QueueDepthWatch {
    board: watch::Receiver<Board>,
}

impl QueueDepthWatch {
    pub fn current(&self) -> QueueDepths {
        self.board.borrow().depths.clone()
    }

    /// `None` once the scheduler behind this watch is gone.
    pub async fn changed(&mut self) -> Option<QueueDepths> {
        self.board.changed().await.ok()?;
        Some(self.board.borrow_and_update().depths.clone())
    }
}

#[derive(Clone)]
pub(crate) struct DepthBoard {
    board: Arc<watch::Sender<Board>>,
}

impl DepthBoard {
    pub(crate) fn new() -> Self {
        Self {
            board: Arc::new(watch::Sender::new(Board::default())),
        }
    }

    pub(crate) fn watch(&self) -> QueueDepthWatch {
        QueueDepthWatch {
            board: self.board.subscribe(),
        }
    }

    /// A re-registered id starts a fresh entry; late updates from the replaced slot are ignored.
    pub(crate) fn register(&self, id: QueueId) -> DepthCell {
        let mut epoch = 0;
        self.board.send_modify(|board| {
            epoch = board.next_epoch;
            board.next_epoch = board.next_epoch.wrapping_add(1);
            board.epochs.insert(id, epoch);
            board.depths.insert(id, QueueDepth::default());
        });
        DepthCell {
            id,
            epoch,
            board: Arc::clone(&self.board),
        }
    }

    pub(crate) fn deregister(&self, id: &QueueId) {
        self.board.send_if_modified(|board| {
            board.epochs.remove(id);
            board.depths.remove(id).is_some()
        });
    }
}

#[derive(Clone)]
pub(crate) struct DepthCell {
    id: QueueId,
    epoch: u64,
    board: Arc<watch::Sender<Board>>,
}

impl DepthCell {
    pub(crate) fn update(&self, apply: impl FnOnce(&mut QueueDepth)) {
        self.board.send_if_modified(|board| {
            if board.epochs.get(&self.id) != Some(&self.epoch) {
                return false;
            }
            let Some(depth) = board.depths.get_mut(&self.id) else {
                return false;
            };
            let before = *depth;
            apply(depth);
            *depth != before
        });
    }
}

