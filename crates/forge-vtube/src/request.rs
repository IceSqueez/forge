use serde_json::Value;
use tokio::sync::oneshot;

pub(crate) struct PendingRequest {
    pub request_id: String,
    pub payload: String,
    pub respond_to: oneshot::Sender<Value>,
}
