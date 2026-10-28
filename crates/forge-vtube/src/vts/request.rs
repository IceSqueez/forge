use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::{Mutex, mpsc, oneshot};

use crate::error::VTubeError;
use crate::vts::protocol::new_request;

pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct PendingRequest {
    pub(crate) request_id: String,
    pub(crate) payload: String,
    pub(crate) respond_to: oneshot::Sender<Value>,
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
    pub(crate) async fn current(&self) -> mpsc::UnboundedSender<PendingRequest> {
        match self {
            ReqTxHandle::Fixed(tx) => tx.clone(),
            ReqTxHandle::Shared(slot) => slot.lock().await.clone(),
        }
    }
}

pub(crate) async fn round_trip(
    req_tx: &mpsc::UnboundedSender<PendingRequest>,
    msg_type: &str,
    data: Value,
    limit: Duration,
) -> Result<Value, VTubeError> {
    let req = new_request(msg_type, data);
    let request_id = req.request_id.clone();
    let payload = serde_json::to_string(&req).map_err(VTubeError::Json)?;
    let (respond_to, rx) = oneshot::channel();
    req_tx
        .send(PendingRequest {
            request_id,
            payload,
            respond_to,
        })
        .map_err(|_| VTubeError::NotConnected)?;
    tokio::time::timeout(limit, rx)
        .await
        .map_err(|_| VTubeError::Timeout)?
        .map_err(|_| VTubeError::NotConnected)
}
