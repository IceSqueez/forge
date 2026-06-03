use serde::{Deserialize, Serialize};
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VTubeRequest<T> {
    #[serde(rename = "apiName")]
    pub api_name: String,
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    #[serde(rename = "requestID")]
    pub request_id: String,
    #[serde(rename = "messageType")]
    pub message_type: String,
    pub data: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VTubeResponse<T> {
    #[serde(rename = "apiName")]
    pub api_name: String,
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    #[serde(rename = "requestID")]
    pub request_id: String,
    #[serde(rename = "messageType")]
    pub message_type: String,
    pub data: T,
}

pub fn new_request<T>(msg_type: &str, data: T) -> VTubeRequest<T> {
    VTubeRequest {
        api_name: "VTubeStudioPublicAPI".to_owned(),
        api_version: "1.0".to_owned(),
        request_id: Ulid::new().to_string(),
        message_type: msg_type.to_owned(),
        data,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
    fn response_roundtrip_extracts_authentication_token() {
        let raw = r#"{
            "apiName":"VTubeStudioPublicAPI",
            "apiVersion":"1.0",
            "requestID":"test-req-001",
            "messageType":"AuthenticationTokenResponse",
            "data":{"authenticationToken":"tok-xyz","granted":true}
        }"#;
        let resp: VTubeResponse<serde_json::Value> = serde_json::from_str(raw).unwrap();
        assert_eq!(resp.message_type, "AuthenticationTokenResponse");
        assert_eq!(resp.data["authenticationToken"], "tok-xyz");
        assert_eq!(resp.data["granted"], true);
    }

    #[test]
    fn request_id_is_nonempty_string() {
        let req = new_request::<serde_json::Value>("TestRequest", serde_json::json!(null));
        assert!(!req.request_id.is_empty());
        assert!(req.request_id.len() > 8);
    }
}
