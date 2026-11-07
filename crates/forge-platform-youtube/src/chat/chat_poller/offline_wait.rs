use std::ops::ControlFlow;

use super::*;
use crate::broadcast::lookup_wake::LookupWakeListener;

const WAKE_FOLLOW_UP_LOOKUPS: u32 = 3;

#[derive(Default)]
pub(super) struct OfflineWait {
    follow_ups_left: u32,
}

impl OfflineWait {
    pub(super) fn settle(&mut self) {
        self.follow_ups_left = 0;
    }
}

impl YouTubeChatPoller {
    pub(super) async fn wait_offline(
        &self,
        offline: &mut OfflineWait,
        wake: &mut LookupWakeListener,
        cancel: &CancellationToken,
    ) -> ControlFlow<()> {
        let pause = match offline.follow_ups_left.checked_sub(1) {
            Some(remaining) => {
                offline.follow_ups_left = remaining;
                self.timings.wake_follow_up_interval
            }
            None => self.timings.offline_cadence,
        };
        tokio::select! {
            biased;
            () = cancel.cancelled() => return ControlFlow::Break(()),
            () = wake.woken() => offline.follow_ups_left = WAKE_FOLLOW_UP_LOOKUPS,
            () = tokio::time::sleep(pause) => {}
        }
        ControlFlow::Continue(())
    }
}
