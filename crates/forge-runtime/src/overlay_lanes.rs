use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError, Weak};

use forge_storage::OverlayId;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

/// Holders of one identity run one at a time in arrival order; different identities never wait
/// on each other. A lane lives only while someone holds or awaits it.
#[derive(Default)]
pub(crate) struct OverlayLanes {
    lanes: Mutex<HashMap<OverlayId, Weak<AsyncMutex<()>>>>,
}

impl OverlayLanes {
    pub(crate) async fn enter(&self, id: &OverlayId) -> OwnedMutexGuard<()> {
        self.lane(id).lock_owned().await
    }

    fn lane(&self, id: &OverlayId) -> Arc<AsyncMutex<()>> {
        let mut lanes = self.lanes.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(lane) = lanes.get(id).and_then(Weak::upgrade) {
            return lane;
        }
        lanes.retain(|_, lane| lane.strong_count() > 0);
        let lane = Arc::new(AsyncMutex::new(()));
        lanes.insert(id.clone(), Arc::downgrade(&lane));
        lane
    }
}
