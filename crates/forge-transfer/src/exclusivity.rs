use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;

#[derive(Debug, Clone, Default)]
pub struct TransferExclusivity {
    held: Arc<AtomicBool>,
    released: Arc<Notify>,
}

#[derive(Debug)]
pub struct TransferGuard {
    held: Arc<AtomicBool>,
    released: Arc<Notify>,
}

impl TransferExclusivity {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_acquire(&self) -> Option<TransferGuard> {
        self.held
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| TransferGuard {
                held: Arc::clone(&self.held),
                released: Arc::clone(&self.released),
            })
    }

    pub async fn acquire_when_free(&self) -> TransferGuard {
        loop {
            let released = self.released.notified();
            tokio::pin!(released);
            released.as_mut().enable();
            if let Some(guard) = self.try_acquire() {
                return guard;
            }
            released.await;
        }
    }

    pub fn is_held(&self) -> bool {
        self.held.load(Ordering::Acquire)
    }
}

impl Drop for TransferGuard {
    fn drop(&mut self) {
        self.held.store(false, Ordering::Release);
        self.released.notify_waiters();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::TransferExclusivity;

    const SCHEDULER_TURNS: usize = 8;

    async fn let_waiters_run() {
        for _ in 0..SCHEDULER_TURNS {
            tokio::task::yield_now().await;
        }
    }

    #[test]
    fn a_second_acquire_fails_while_the_first_guard_lives() {
        let exclusivity = TransferExclusivity::new();
        let _guard = exclusivity.try_acquire();

        assert!(exclusivity.try_acquire().is_none());
    }

    #[test]
    fn a_guard_taken_through_one_clone_blocks_every_other_clone_until_dropped() {
        let scheduler = TransferExclusivity::new();
        let manual = scheduler.clone();
        let guard = scheduler.try_acquire().unwrap();

        let while_held = (manual.is_held(), manual.try_acquire().is_some());
        drop(guard);
        let after_release = (manual.is_held(), manual.try_acquire().is_some());

        assert_eq!((while_held, after_release), ((true, false), (false, true)));
    }

    #[tokio::test]
    async fn waiting_on_a_free_exclusivity_takes_it_at_once() {
        let exclusivity = TransferExclusivity::new();

        let _guard = exclusivity.acquire_when_free().await;

        assert!(exclusivity.try_acquire().is_none());
    }

    #[tokio::test]
    async fn a_waiter_stays_parked_while_held_and_takes_the_guard_on_release() {
        let exclusivity = TransferExclusivity::new();
        let running = exclusivity.try_acquire().unwrap();
        let waiting = exclusivity.clone();
        let waiter = tokio::spawn(async move { waiting.acquire_when_free().await });
        let_waiters_run().await;
        let parked_while_held = !waiter.is_finished();

        drop(running);
        let _taken = waiter.await.unwrap();

        assert_eq!(
            (parked_while_held, exclusivity.try_acquire().is_none()),
            (true, true)
        );
    }

    #[tokio::test]
    async fn a_waiter_beaten_to_the_release_keeps_waiting_for_the_next_one() {
        let exclusivity = TransferExclusivity::new();
        let first = exclusivity.try_acquire().unwrap();
        let waiting = exclusivity.clone();
        let waiter = tokio::spawn(async move { waiting.acquire_when_free().await });
        let_waiters_run().await;

        drop(first);
        let regrabbed = exclusivity.try_acquire().unwrap();
        let_waiters_run().await;
        let parked_after_losing = !waiter.is_finished();
        drop(regrabbed);
        let _taken = waiter.await.unwrap();

        assert_eq!((parked_after_losing, exclusivity.is_held()), (true, true));
    }

    #[tokio::test]
    async fn two_waiters_take_the_guard_one_release_at_a_time() {
        let exclusivity = TransferExclusivity::new();
        let running = exclusivity.try_acquire().unwrap();
        let spawn_waiter = |exclusivity: TransferExclusivity| {
            tokio::spawn(async move { exclusivity.acquire_when_free().await })
        };
        let first = spawn_waiter(exclusivity.clone());
        let second = spawn_waiter(exclusivity.clone());
        let_waiters_run().await;

        drop(running);
        let_waiters_run().await;
        let finished_after_one_release =
            usize::from(first.is_finished()) + usize::from(second.is_finished());
        let (winner, loser) = if first.is_finished() {
            (first, second)
        } else {
            (second, first)
        };
        drop(winner.await.unwrap());
        let _last = loser.await.unwrap();

        assert_eq!(
            (finished_after_one_release, exclusivity.is_held()),
            (1, true)
        );
    }
}
