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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_with_vts_field_names() {
        let req = new_request("APIStateRequest", serde_json::json!({}));
        let val = serde_json::to_value(&req).unwrap();
        assert!(val.get("apiName").is_some());
        assert!(val.get("apiVersion").is_some());
        assert!(val.get("requestID").is_some());
        assert!(val.get("messageType").is_some());
        assert!(val.get("data").is_some());
    }

    #[test]
    fn request_roundtrip_preserves_message_type_and_api_fields() {
        let req = new_request(
            "AuthenticationTokenRequest",
            serde_json::json!({ "pluginName": "forge" }),
        );
        let json = serde_json::to_string(&req).unwrap();
        let back: VTubeRequest<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.api_name, "VTubeStudioPublicAPI");
        assert_eq!(back.api_version, "1.0");
        assert_eq!(back.message_type, "AuthenticationTokenRequest");
        assert_eq!(back.data["pluginName"], "forge");
    }

    #[test]
    fn successive_requests_have_distinct_request_ids() {
        let a = new_request::<serde_json::Value>("A", serde_json::json!(null));
        let b = new_request::<serde_json::Value>("B", serde_json::json!(null));
        assert_ne!(a.request_id, b.request_id);
    }

    #[test]
    fn check_response_turns_a_top_level_error_id_into_a_rejection() {
        for (data, expected) in [
            (serde_json::json!({}), None),
            (serde_json::json!({ "hotkeyID": "abc" }), None),
            (
                serde_json::json!({ "errorID": 100, "message": "no such hotkey" }),
                Some((100, "no such hotkey")),
            ),
            (
                serde_json::json!({ "errorID": 8 }),
                Some((8, "no reason given")),
            ),
        ] {
            let rejection = match check_response(&data) {
                Ok(()) => None,
                Err(VTubeError::Rejected { error_id, message }) => Some((error_id, message)),
                Err(other) => Some((i64::MIN, other.to_string())),
            };

            assert_eq!(
                rejection,
                expected.map(|(id, reason)| (id, reason.to_owned())),
                "{data}"
            );
        }
    }
}
