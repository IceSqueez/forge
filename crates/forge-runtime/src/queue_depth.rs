use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use forge_types::{ActionId, QueueId};
use tokio::sync::watch;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QueueDepth {
    pub pending: usize,
    pub in_flight: usize,
    pub overflowed: u64,
}

pub type QueueDepths = HashMap<QueueId, QueueDepth>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunningAction {
    pub action_id: ActionId,
    pub started_at: Instant,
}

pub type QueueRunning = HashMap<QueueId, Vec<RunningAction>>;

#[derive(Default)]
struct Board {
    depths: QueueDepths,
    running: QueueRunning,
    epochs: HashMap<QueueId, u64>,
    next_epoch: u64,
}

pub struct QueueDepthWatch {
    board: watch::Receiver<Board>,
}

impl QueueDepthWatch {
    pub fn current(&self) -> QueueDepths {
        self.board.borrow().depths.clone()
    }

    pub fn running(&self) -> QueueRunning {
        self.board.borrow().running.clone()
    }

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

    pub(crate) fn register(&self, id: QueueId) -> DepthCell {
        let mut epoch = 0;
        self.board.send_modify(|board| {
            epoch = board.next_epoch;
            board.next_epoch = board.next_epoch.wrapping_add(1);
            board.epochs.insert(id, epoch);
            board.depths.insert(id, QueueDepth::default());
            board.running.remove(&id);
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
            board.running.remove(id);
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

    pub(crate) fn publish_running(&self, running: Vec<RunningAction>) {
        self.board.send_if_modified(|board| {
            if board.epochs.get(&self.id) != Some(&self.epoch) {
                return false;
            }
            let Some(depth) = board.depths.get_mut(&self.id) else {
                return false;
            };
            let published = board.running.get(&self.id).map_or(&[][..], Vec::as_slice);
            if depth.in_flight == running.len() && published == running {
                return false;
            }
            depth.in_flight = running.len();
            board.running.insert(self.id, running);
            true
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

    fn run() -> RunningAction {
        RunningAction {
            action_id: ActionId::new(),
            started_at: Instant::now(),
        }
    }

    #[test]
    fn publishing_the_running_list_keeps_the_in_flight_depth_in_step() {
        let board = DepthBoard::new();
        let id = QueueId::new();
        let cell = board.register(id);
        let runs = vec![run(), run()];

        cell.publish_running(runs.clone());

        let watch = board.watch();
        assert_eq!(
            (
                watch.current().get(&id).map(|depth| depth.in_flight),
                watch.running().get(&id).cloned(),
            ),
            (Some(2), Some(runs))
        );
    }

    #[test]
    fn a_stale_registration_cannot_publish_runs_or_wake_a_watcher() {
        let board = DepthBoard::new();
        let id = QueueId::new();
        let stale = board.register(id);
        let fresh = board.register(id);
        let live = run();
        fresh.publish_running(vec![live]);
        let mut watch = board.watch();

        stale.publish_running(vec![run(), run()]);

        assert_eq!(
            (
                watch.running().get(&id).cloned(),
                watch.current().get(&id).map(|depth| depth.in_flight),
                watch.changed().now_or_never().is_none(),
            ),
            (Some(vec![live]), Some(1), true)
        );
    }

    #[test]
    fn republishing_the_same_runs_wakes_no_watcher() {
        let board = DepthBoard::new();
        let cell = board.register(QueueId::new());
        let mut idle = board.watch();
        cell.publish_running(Vec::new());
        assert!(
            idle.changed().now_or_never().is_none(),
            "an idle queue reporting no runs is not a change"
        );

        let runs = vec![run()];
        cell.publish_running(runs.clone());
        let mut watch = board.watch();
        cell.publish_running(runs);

        assert!(watch.changed().now_or_never().is_none());
    }

    #[tokio::test]
    async fn swapping_one_run_for_another_wakes_the_watcher_at_the_same_depth() {
        let board = DepthBoard::new();
        let id = QueueId::new();
        let cell = board.register(id);
        cell.publish_running(vec![run()]);
        let mut watch = board.watch();
        let next = run();

        cell.publish_running(vec![next]);

        assert!(watch.changed().await.is_some());
        assert_eq!(watch.running().get(&id).cloned(), Some(vec![next]));
    }

    #[test]
    fn leaving_or_rejoining_the_board_forgets_the_runs() {
        for rejoin in [false, true] {
            let board = DepthBoard::new();
            let id = QueueId::new();
            let cell = board.register(id);
            cell.publish_running(vec![run()]);

            if rejoin {
                let _fresh = board.register(id);
            } else {
                board.deregister(&id);
            }
            cell.publish_running(vec![run()]);

            assert!(
                !board.watch().running().contains_key(&id),
                "rejoin={rejoin}"
            );
        }
    }
}
