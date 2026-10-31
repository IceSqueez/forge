use std::future::Future;
use std::sync::Arc;

use tokio::sync::Notify;

use crate::HANG_GUARD;

#[derive(Clone)]
pub struct Quiescence(Arc<Notify>);

impl Quiescence {
    pub async fn reached(&self) {
        let parked = tokio::time::timeout(HANG_GUARD, self.0.notified()).await;
        assert!(parked.is_ok(), "some task never stopped running");
    }
}

pub fn run_with_quiescence<Fut: Future>(test: impl FnOnce(Quiescence) -> Fut) -> Fut::Output {
    let parks = Arc::new(Notify::new());
    let hook = Arc::clone(&parks);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .on_thread_park(move || hook.notify_waiters())
        .build()
        .expect("a current-thread test runtime");
    runtime.block_on(test(Quiescence(parks)))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    #[test]
    fn every_runnable_task_has_run_as_far_as_it_can_once_reached_returns() {
        const NESTED_SPAWNS: usize = 300;
        let reached_the_bottom = Arc::new(AtomicBool::new(false));

        fn descend(depth: usize, bottom: Arc<AtomicBool>) {
            tokio::spawn(async move {
                tokio::task::yield_now().await;
                if depth == 0 {
                    bottom.store(true, Ordering::SeqCst);
                } else {
                    descend(depth - 1, bottom);
                }
            });
        }

        run_with_quiescence({
            let bottom = Arc::clone(&reached_the_bottom);
            |quiet| async move {
                descend(NESTED_SPAWNS, bottom);
                quiet.reached().await;
            }
        });

        assert!(reached_the_bottom.load(Ordering::SeqCst));
    }

    #[test]
    fn a_task_parked_on_a_gate_stays_unfinished_after_quiescence() {
        run_with_quiescence(|quiet| async move {
            let (open, gate) = tokio::sync::oneshot::channel::<()>();
            let waiter = tokio::spawn(gate);

            quiet.reached().await;
            let finished_before_opening = waiter.is_finished();
            open.send(()).unwrap();
            waiter.await.unwrap().unwrap();

            assert!(!finished_before_opening);
        });
    }
}
