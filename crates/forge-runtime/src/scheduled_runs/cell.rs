use std::sync::Arc;

use arc_swap::ArcSwapOption;

use super::handle::ScheduledRunsHandle;

#[derive(Clone, Default)]
pub struct ScheduledRunsCell {
    inner: Arc<ArcSwapOption<ScheduledRunsHandle>>,
}

impl ScheduledRunsCell {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, handle: ScheduledRunsHandle) {
        self.inner.store(Some(Arc::new(handle)));
    }

    pub fn get(&self) -> Option<ScheduledRunsHandle> {
        self.inner.load_full().map(|handle| (*handle).clone())
    }
}
