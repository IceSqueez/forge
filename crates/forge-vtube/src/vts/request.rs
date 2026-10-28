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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const LIMIT: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn the_reply_to_the_queued_request_is_returned_to_the_caller() {
        let (req_tx, mut req_rx) = mpsc::unbounded_channel::<PendingRequest>();
        let peer = tokio::spawn(async move {
            let req = req_rx.recv().await.unwrap();
            let sent: Value = serde_json::from_str(&req.payload).unwrap();
            req.respond_to.send(json!({ "ok": 1 })).unwrap();
            (req.request_id, sent)
        });

        let reply = round_trip(
            &req_tx,
            "HotkeyTriggerRequest",
            json!({ "hotkeyID": "hk" }),
            LIMIT,
        )
        .await
        .unwrap();

        assert_eq!(reply, json!({ "ok": 1 }));
        let (request_id, sent) = peer.await.unwrap();
        assert_eq!(sent["requestID"], request_id.as_str());
        assert_eq!(sent["messageType"], "HotkeyTriggerRequest");
        assert_eq!(sent["data"], json!({ "hotkeyID": "hk" }));
    }

    #[tokio::test]
    async fn a_closed_request_channel_reports_not_connected() {
        let (req_tx, req_rx) = mpsc::unbounded_channel::<PendingRequest>();
        drop(req_rx);

        let result = round_trip(&req_tx, "APIStateRequest", json!({}), LIMIT).await;

        assert!(matches!(result, Err(VTubeError::NotConnected)));
    }

    #[tokio::test]
    async fn a_request_dropped_without_a_reply_reports_not_connected() {
        let (req_tx, mut req_rx) = mpsc::unbounded_channel::<PendingRequest>();
        tokio::spawn(async move {
            drop(req_rx.recv().await);
        });

        let result = round_trip(&req_tx, "APIStateRequest", json!({}), LIMIT).await;

        assert!(matches!(result, Err(VTubeError::NotConnected)));
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_left_unanswered_past_the_limit_times_out() {
        let (req_tx, _req_rx) = mpsc::unbounded_channel::<PendingRequest>();
        let started = tokio::time::Instant::now();

        let result = round_trip(&req_tx, "APIStateRequest", json!({}), LIMIT).await;

        assert!(matches!(result, Err(VTubeError::Timeout)));
        assert!(started.elapsed() >= LIMIT);
    }
}
