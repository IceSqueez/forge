use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use tokio::task::JoinHandle;

pub(crate) struct HoldRegistry {
    holds: Mutex<HashMap<String, JoinHandle<()>>>,
}

impl HoldRegistry {
    pub(crate) fn new() -> Self {
        Self {
            holds: Mutex::new(HashMap::new()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, JoinHandle<()>>> {
        self.holds
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn abort(&self, param_id: &str) {
        let mut holds = self.lock();
        holds.retain(|_, handle| !handle.is_finished());
        if let Some(previous) = holds.remove(param_id) {
            previous.abort();
        }
    }

    pub(crate) fn insert(&self, param_id: String, handle: JoinHandle<()>) {
        let mut holds = self.lock();
        holds.retain(|_, handle| !handle.is_finished());
        if let Some(replaced) = holds.insert(param_id, handle) {
            replaced.abort();
        }
    }

    pub(crate) fn abort_all(&self) {
        let mut holds = self.lock();
        for (_, handle) in holds.drain() {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::oneshot;

    use super::*;

    fn hold_until_aborted() -> (JoinHandle<()>, oneshot::Receiver<()>) {
        let (alive, ended) = oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            let _alive = alive;
            std::future::pending::<()>().await;
        });
        (handle, ended)
    }

    #[tokio::test]
    async fn a_second_hold_on_one_parameter_aborts_the_first_and_stays_running() {
        let registry = HoldRegistry::new();
        let (first, first_ended) = hold_until_aborted();
        let (second, mut second_ended) = hold_until_aborted();

        registry.insert("ParamAngleX".to_owned(), first);
        registry.insert("ParamAngleX".to_owned(), second);

        assert!(first_ended.await.is_err(), "the replaced hold kept running");
        assert!(
            matches!(
                second_ended.try_recv(),
                Err(oneshot::error::TryRecvError::Empty)
            ),
            "the replacing hold was aborted with the one it replaced"
        );
    }

    #[tokio::test]
    async fn a_hold_on_another_parameter_is_left_running() {
        let registry = HoldRegistry::new();
        let (first, mut first_ended) = hold_until_aborted();
        let (second, _second_ended) = hold_until_aborted();

        registry.insert("ParamAngleX".to_owned(), first);
        registry.insert("ParamAngleY".to_owned(), second);
        tokio::task::yield_now().await;

        assert!(matches!(
            first_ended.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
    }

    #[tokio::test]
    async fn abort_all_ends_every_hold() {
        let registry = HoldRegistry::new();
        let (first, first_ended) = hold_until_aborted();
        let (second, second_ended) = hold_until_aborted();
        registry.insert("ParamAngleX".to_owned(), first);
        registry.insert("ParamAngleY".to_owned(), second);

        registry.abort_all();

        assert!(first_ended.await.is_err());
        assert!(second_ended.await.is_err());
    }
}
