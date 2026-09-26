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

#[cfg(test)]
mod tests {
    use futures_util::FutureExt as _;

    use super::*;

    fn pending(depths: &QueueDepths, id: QueueId) -> Option<usize> {
        depths.get(&id).map(|depth| depth.pending)
    }

    #[tokio::test]
    async fn a_burst_between_reads_coalesces_into_one_change_carrying_the_final_depth() {
        let board = DepthBoard::new();
        let id = QueueId::new();
        let cell = board.register(id);
        let mut watch = board.watch();

        for n in 1..=3 {
            cell.update(|depth| depth.pending = n);
        }

        let first = watch.changed().await.map(|depths| pending(&depths, id));
        let second = watch.changed().now_or_never();
        assert_eq!(
            (first, second.is_none()),
            (Some(Some(3)), true),
            "one read must see the last value of the burst and nothing may be left over"
        );
    }

    #[test]
    fn a_write_that_changes_nothing_wakes_no_watcher() {
        let board = DepthBoard::new();
        let id = QueueId::new();
        let cell = board.register(id);
        let mut watch = board.watch();

        cell.update(|depth| depth.pending = 0);
        board.deregister(&QueueId::new());

        assert!(watch.changed().now_or_never().is_none());
    }

    #[test]
    fn a_cell_from_a_replaced_registration_no_longer_writes_the_depth() {
        let board = DepthBoard::new();
        let id = QueueId::new();
        let stale = board.register(id);
        let fresh = board.register(id);

        stale.update(|depth| depth.pending = 5);
        fresh.update(|depth| depth.in_flight = 1);

        assert_eq!(
            board.watch().current().get(&id).copied(),
            Some(QueueDepth {
                pending: 0,
                in_flight: 1,
                overflowed: 0,
            })
        );
    }

    #[test]
    fn a_deregistered_queue_leaves_the_depths_and_its_cell_cannot_bring_it_back() {
        let board = DepthBoard::new();
        let id = QueueId::new();
        let cell = board.register(id);
        cell.update(|depth| depth.pending = 2);

        board.deregister(&id);
        cell.update(|depth| depth.pending = 7);

        assert!(!board.watch().current().contains_key(&id));
    }

    #[tokio::test]
    async fn the_watch_ends_once_the_board_and_every_cell_are_gone() {
        let board = DepthBoard::new();
        let cell = board.register(QueueId::new());
        let mut watch = board.watch();

        drop(cell);
        drop(board);

        assert!(watch.changed().await.is_none());
    }
}
