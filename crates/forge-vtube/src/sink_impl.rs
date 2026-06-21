use std::collections::BTreeMap;

use async_trait::async_trait;
use serde_json::{Value, json};

use forge_types::Variant;

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

    async fn move_item(
        &self,
        item_instance_id: &str,
        x: Option<f64>,
        y: Option<f64>,
        size: Option<f64>,
        rotation: Option<f64>,
        order: Option<i64>,
        time_in_seconds: f64,
        fade_mode: &str,
    ) -> Result<(), VTubeError> {
        let data = json!({
            "itemsToMove": [{
                "itemInstanceID": item_instance_id,
                "timeInSeconds": time_in_seconds,
                "fadeMode": fade_mode,
                "positionX": x.unwrap_or(ITEM_IGNORE_SENTINEL),
                "positionY": y.unwrap_or(ITEM_IGNORE_SENTINEL),
                "size": size.unwrap_or(ITEM_IGNORE_SENTINEL),
                "rotation": rotation.unwrap_or(ITEM_IGNORE_SENTINEL),
                "order": order.unwrap_or(ITEM_IGNORE_SENTINEL_ORDER),
                "setFlip": false,
                "flip": false,
                "userCanStop": false
            }]
        });
        let resp = self.send_json_request("ItemMoveRequest", data).await?;
        check_response(&resp)
    }

    async fn get_current_model(&self) -> Result<Variant, VTubeError> {
        let resp = self
            .send_json_request("CurrentModelRequest", json!({}))
            .await?;
        check_response(&resp)?;
        let mut fields = BTreeMap::new();
        fields.insert("name".to_owned(), string_field(&resp, "modelName"));
        fields.insert("id".to_owned(), string_field(&resp, "modelID"));
        fields.insert(
            "loaded".to_owned(),
            Variant::Bool(resp["modelLoaded"].as_bool().unwrap_or(false)),
        );
        Ok(Variant::Object(fields))
    }

    async fn get_hotkeys(&self) -> Result<Variant, VTubeError> {
        let resp = self
            .send_json_request("HotkeysInCurrentModelRequest", json!({}))
            .await?;
        check_response(&resp)?;
        let entries = resp["availableHotkeys"].as_array();
        let names = string_array(entries, "name");
        let ids = string_array(entries, "hotkeyID");
        let count = names.len() as i64;
        let mut fields = BTreeMap::new();
        fields.insert("names".to_owned(), Variant::Array(names));
        fields.insert("ids".to_owned(), Variant::Array(ids));
        fields.insert("count".to_owned(), Variant::Int(count));
        Ok(Variant::Object(fields))
    }

    async fn get_expressions(&self) -> Result<Variant, VTubeError> {
        let resp = self
            .send_json_request("ExpressionStateRequest", json!({}))
            .await?;
        check_response(&resp)?;
        let entries = resp["expressions"].as_array();
        let names = string_array(entries, "name");
        let active = bool_array(entries, "active");
        let count = names.len() as i64;
        let mut fields = BTreeMap::new();
        fields.insert("names".to_owned(), Variant::Array(names));
        fields.insert("active".to_owned(), Variant::Array(active));
        fields.insert("count".to_owned(), Variant::Int(count));
        Ok(Variant::Object(fields))
    }

    async fn get_parameters(&self) -> Result<Variant, VTubeError> {
        let resp = self
            .send_json_request("InputParameterListRequest", json!({}))
            .await?;
        check_response(&resp)?;
        let mut names = string_array(resp["defaultParameters"].as_array(), "name");
        names.extend(string_array(resp["customParameters"].as_array(), "name"));
        let count = names.len() as i64;
        let mut fields = BTreeMap::new();
        fields.insert("names".to_owned(), Variant::Array(names));
        fields.insert("count".to_owned(), Variant::Int(count));
        Ok(Variant::Object(fields))
    }

    async fn get_items(&self) -> Result<Variant, VTubeError> {
        let data = json!({
            "includeAvailableSpots": false,
            "includeItemInstancesInScene": true,
            "includeAvailableItemFiles": false
        });
        let resp = self.send_json_request("ItemListRequest", data).await?;
        check_response(&resp)?;
        let entries = resp["itemInstancesInScene"].as_array();
        let instance_ids = string_array(entries, "instanceID");
        let file_names = string_array(entries, "fileName");
        let count = instance_ids.len() as i64;
        let mut fields = BTreeMap::new();
        fields.insert("instance_ids".to_owned(), Variant::Array(instance_ids));
        fields.insert("file_names".to_owned(), Variant::Array(file_names));
        fields.insert("count".to_owned(), Variant::Int(count));
        Ok(Variant::Object(fields))
    }
}

fn string_field(data: &Value, key: &str) -> Variant {
    Variant::String(data[key].as_str().unwrap_or("").to_owned())
}

fn string_array(entries: Option<&Vec<Value>>, key: &str) -> Vec<Variant> {
    entries
        .map(|arr| {
            arr.iter()
                .map(|e| Variant::String(e[key].as_str().unwrap_or("").to_owned()))
                .collect()
        })
        .unwrap_or_default()
}

fn bool_array(entries: Option<&Vec<Value>>, key: &str) -> Vec<Variant> {
    entries
        .map(|arr| {
            arr.iter()
                .map(|e| Variant::Bool(e[key].as_bool().unwrap_or(false)))
                .collect()
        })
        .unwrap_or_default()
}

const ITEM_IGNORE_SENTINEL: f64 = -1000.0;
const ITEM_IGNORE_SENTINEL_ORDER: i64 = -1000;

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
