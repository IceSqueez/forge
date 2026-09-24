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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::time::Duration;

    use super::*;

    const WAIT_BOUND: Duration = Duration::from_secs(5);

    fn tracked(lanes: &OverlayLanes) -> usize {
        lanes.lanes.lock().unwrap().len()
    }

    #[tokio::test]
    async fn lanes_nobody_holds_do_not_accumulate_across_many_distinct_overlays() {
        let lanes = OverlayLanes::default();

        for index in 0..64 {
            drop(
                lanes
                    .enter(&OverlayId::new(format!("overlay-{index}")))
                    .await,
            );
        }

        assert!(
            tracked(&lanes) <= 1,
            "{} lanes are kept for overlays nobody holds, so the map grows with every identity ever pushed",
            tracked(&lanes)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn pruning_never_drops_a_lane_that_is_still_held() {
        let lanes = OverlayLanes::default();
        let held = OverlayId::new("goal-box");
        let _holder = lanes.enter(&held).await;

        for index in 0..8 {
            drop(lanes.enter(&OverlayId::new(format!("other-{index}"))).await);
        }

        assert!(
            tokio::time::timeout(WAIT_BOUND, lanes.enter(&held))
                .await
                .is_err(),
            "a second holder entered a lane that is still held, so pruning replaced a live lane"
        );
    }
}
