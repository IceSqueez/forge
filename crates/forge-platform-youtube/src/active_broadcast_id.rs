use std::sync::Arc;

/// Shared handle to the active broadcast (video) id discovered by the poller.
///
/// `None` when no broadcast is active or the poller has not yet resolved one.
/// Uses `std::sync::Mutex` because the critical section is a single clone - never
/// held across an `await`.
#[derive(Debug, Clone, Default)]
pub struct ActiveBroadcastIdHandle {
    inner: Arc<std::sync::Mutex<Option<String>>>,
}

impl ActiveBroadcastIdHandle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, id: Option<String>) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = id;
        }
    }

    pub fn get(&self) -> Option<String> {
        self.inner.lock().ok().and_then(|g| g.clone())
    }
}
