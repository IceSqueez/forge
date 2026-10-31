use forge_types::{Donation, RecoverMutex};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_storage::OverlayId;
use tokio::sync::Notify;

use crate::overlay_service::OverlayConnectListener;

#[async_trait]
pub trait DonationAudience: Send + Sync {
    async fn is_listening(&self) -> bool;
}

#[derive(Default)]
pub(crate) struct CatchUpHold {
    held: Mutex<VecDeque<Donation>>,
    wake: Notify,
}

impl CatchUpHold {
    pub(crate) fn hold(&self, donation: Donation) {
        self.held.lock_recover().push_back(donation);
        self.wake.notify_one();
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.held.lock_recover().is_empty()
    }

    pub(crate) fn take_in_order(&self) -> Vec<Donation> {
        let mut batch: Vec<Donation> = self.held.lock_recover().drain(..).collect();
        batch.sort_by_key(|donation| donation.occurred_at);
        batch
    }

    pub(crate) async fn woken(&self) {
        self.wake.notified().await;
    }
}

#[derive(Clone)]
pub struct CatchUpWaker(pub(crate) Arc<CatchUpHold>);

impl CatchUpWaker {
    pub fn wake(&self) {
        self.0.wake.notify_one();
    }
}

#[async_trait]
impl OverlayConnectListener for CatchUpWaker {
    async fn overlay_connected(&self, _identity: &OverlayId) {
        self.wake();
    }
}
