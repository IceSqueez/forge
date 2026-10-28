use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::error::VTubeError;

pub(crate) const API_NAME: &str = "VTubeStudioPublicAPI";
pub(crate) const API_VERSION: &str = "1.0";
pub(crate) const API_ERROR_MESSAGE_TYPE: &str = "APIError";
pub(crate) const TOKEN_REQUEST_DENIED_ERROR_ID: i64 = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct VTubeRequest<T> {
    #[serde(rename = "apiName")]
    pub(crate) api_name: String,
    #[serde(rename = "apiVersion")]
    pub(crate) api_version: String,
    #[serde(rename = "requestID")]
    pub(crate) request_id: String,
    #[serde(rename = "messageType")]
    pub(crate) message_type: String,
    pub(crate) data: T,
}

pub(crate) fn new_request<T>(msg_type: &str, data: T) -> VTubeRequest<T> {
    VTubeRequest {
        api_name: API_NAME.to_owned(),
        api_version: API_VERSION.to_owned(),
        request_id: Ulid::generate().to_string(),
        message_type: msg_type.to_owned(),
        data,
    }
}

pub(crate) fn is_reply(message_type: &str) -> bool {
    message_type == API_ERROR_MESSAGE_TYPE || message_type.ends_with("Response")
}

pub(crate) fn check_response(data: &serde_json::Value) -> Result<(), VTubeError> {
    match data.get("errorID").and_then(serde_json::Value::as_i64) {
        Some(error_id) => Err(VTubeError::Rejected {
            error_id,
            message: data["message"]
                .as_str()
                .unwrap_or("no reason given")
                .to_owned(),
        }),
        None => Ok(()),
    }
}
