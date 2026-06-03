use async_trait::async_trait;
use serde_json::json;

use crate::client::VTubeClient;
use crate::error::VTubeError;
use crate::sink::VTubeSink;

#[async_trait]
impl VTubeSink for VTubeClient {
    async fn trigger_hotkey(&self, hotkey_id: &str) -> Result<(), VTubeError> {
        let data = json!({ "hotkeyID": hotkey_id });
        let resp = self.send_json_request("HotkeyTriggerRequest", data).await?;
        check_response(&resp)
    }

    async fn set_expression(&self, expression_file: &str, active: bool) -> Result<(), VTubeError> {
        let data = json!({ "expressionFile": expression_file, "active": active });
        let resp = self
            .send_json_request("ExpressionActivationRequest", data)
            .await?;
        check_response(&resp)
    }

    async fn set_param(&self, param_id: &str, value: f64) -> Result<(), VTubeError> {
        let data = json!({
            "faceFound": false,
            "mode": "set",
            "parameterValues": [{ "id": param_id, "value": value }]
        });
        let resp = self
            .send_json_request("InjectParameterDataRequest", data)
            .await?;
        check_response(&resp)
    }

    async fn load_model(&self, model_id: &str) -> Result<(), VTubeError> {
        let data = json!({ "modelID": model_id });
        let resp = self.send_json_request("ModelLoadRequest", data).await?;
        check_response(&resp)?;
        self.content_notifier.notify_model_changed();
        Ok(())
    }

    async fn reset_params(&self) -> Result<(), VTubeError> {
        let data = json!({
            "faceFound": false,
            "mode": "set",
            "parameterValues": []
        });
        let resp = self
            .send_json_request("InjectParameterDataRequest", data)
            .await?;
        check_response(&resp)
    }

    async fn move_model(
        &self,
        x: Option<f64>,
        y: Option<f64>,
        rotation: Option<f64>,
        time_in_seconds: f64,
    ) -> Result<(), VTubeError> {
        let mut data = json!({
            "timeInSeconds": time_in_seconds,
            "valuesAreRelativeToModel": false
        });
        if let Some(v) = x {
            data["positionX"] = json!(v);
        }
        if let Some(v) = y {
            data["positionY"] = json!(v);
        }
        if let Some(v) = rotation {
            data["rotation"] = json!(v);
        }
        let resp = self.send_json_request("MoveModelRequest", data).await?;
        check_response(&resp)
    }
}

fn check_response(data: &serde_json::Value) -> Result<(), VTubeError> {
    if let Some(error_id) = data.get("errorID").and_then(|v| v.as_i64()) {
        let message = data["message"]
            .as_str()
            .unwrap_or("request rejected by VTube Studio");
        return Err(VTubeError::Request {
            message: format!("{message} (errorID={error_id})"),
        });
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn check_response_passes_empty_data() {
        assert!(check_response(&serde_json::json!({})).is_ok());
    }

    #[test]
    fn check_response_passes_success_data() {
        assert!(check_response(&serde_json::json!({ "hotkeyID": "abc" })).is_ok());
    }

    #[test]
    fn check_response_fails_on_error_id() {
        let data = serde_json::json!({ "errorID": 100, "message": "no such hotkey" });
        let err = check_response(&data).unwrap_err();
        assert!(matches!(err, VTubeError::Request { .. }));
        assert!(err.to_string().contains("errorID=100"));
    }
}
