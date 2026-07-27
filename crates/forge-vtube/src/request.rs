use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{Mutex, mpsc, oneshot};

pub(crate) struct PendingRequest {
    pub request_id: String,
    pub payload: String,
    pub respond_to: oneshot::Sender<Value>,
}

pub(crate) type ReqTxSlot = Arc<Mutex<mpsc::UnboundedSender<PendingRequest>>>;

pub(crate) enum ReqTxHandle {
    Fixed(mpsc::UnboundedSender<PendingRequest>),
    Shared(ReqTxSlot),
}

impl From<mpsc::UnboundedSender<PendingRequest>> for ReqTxHandle {
    fn from(tx: mpsc::UnboundedSender<PendingRequest>) -> Self {
        ReqTxHandle::Fixed(tx)
    }
}

impl From<ReqTxSlot> for ReqTxHandle {
    fn from(slot: ReqTxSlot) -> Self {
        ReqTxHandle::Shared(slot)
    }
}

impl ReqTxHandle {
    // Resolves through the slot on every call so a sender swapped mid-session by reconnect
    // reaches the next send instead of a stale construction-time clone.
    pub(crate) async fn current(&self) -> mpsc::UnboundedSender<PendingRequest> {
        match self {
            ReqTxHandle::Fixed(tx) => tx.clone(),
            ReqTxHandle::Shared(slot) => slot.lock().await.clone(),
        }
    }
}
