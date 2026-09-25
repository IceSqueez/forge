use std::collections::BTreeMap;

use async_trait::async_trait;
use serde_json::{Value, json};

use forge_types::Variant;

use crate::client::VTubeClient;
use crate::error::VTubeError;
use crate::protocol::check_response;
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
        let resp = self
            .send_json_request("ExpressionStateRequest", json!({}))
            .await?;
        check_response(&resp)?;
        let active_files: Vec<String> = resp["expressions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| entry["active"].as_bool().unwrap_or(false))
            .filter_map(|entry| entry["file"].as_str().map(str::to_owned))
            .collect();
        let mut failed: Vec<(String, i64)> = Vec::new();
        for file in &active_files {
            if let Err(err) = self.set_expression(file, false).await {
                let error_id = match err {
                    VTubeError::Rejected { error_id, .. } => error_id,
                    _ => EXPRESSION_ERROR_ID_UNKNOWN,
                };
                failed.push((file.clone(), error_id));
            }
        }
        match failed.first() {
            None => Ok(()),
            Some((_, error_id)) => {
                let names: Vec<&str> = failed.iter().map(|(file, _)| file.as_str()).collect();
                Err(VTubeError::Rejected {
                    error_id: *error_id,
                    message: format!("could not deactivate expression(s): {}", names.join(", ")),
                })
            }
        }
    }

    async fn move_model(
        &self,
        x: Option<f64>,
        y: Option<f64>,
        rotation: Option<f64>,
        size: Option<f64>,
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
        if let Some(v) = size {
            data["size"] = json!(v);
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
        check_response(&resp)?;
        check_items_moved(&resp)
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

    async fn pin_item(
        &self,
        item_instance_id: &str,
        pin: bool,
        angle_relative_to: &str,
        size_relative_to: &str,
        vertex_pin_type: &str,
        model_id: &str,
        art_mesh_id: &str,
        angle: f64,
        size: f64,
    ) -> Result<(), VTubeError> {
        let mut data = json!({
            "pin": pin,
            "itemInstanceID": item_instance_id
        });
        if pin {
            data["angleRelativeTo"] = json!(angle_relative_to);
            data["sizeRelativeTo"] = json!(size_relative_to);
            data["vertexPinType"] = json!(vertex_pin_type);
            data["pinInfo"] = json!({
                "modelID": model_id,
                "artMeshID": art_mesh_id,
                "angle": angle,
                "size": size
            });
        }
        let resp = self.send_json_request("ItemPinRequest", data).await?;
        check_response(&resp)
    }

    async fn load_item(
        &self,
        file_name: &str,
        x: Option<f64>,
        y: Option<f64>,
        size: Option<f64>,
        rotation: Option<f64>,
        fade_time: Option<f64>,
        order: Option<i64>,
        unload_on_disconnect: bool,
    ) -> Result<Variant, VTubeError> {
        let data = json!({
            "fileName": file_name,
            "positionX": x.unwrap_or(0.0),
            "positionY": y.unwrap_or(0.0),
            "size": size.unwrap_or(0.32),
            "rotation": rotation.unwrap_or(0.0),
            "fadeTime": fade_time.unwrap_or(0.5),
            "order": order.unwrap_or(0),
            "failIfOrderTaken": false,
            "smoothing": 0,
            "censored": false,
            "flipped": false,
            "locked": false,
            "unloadWhenPluginDisconnects": unload_on_disconnect,
            "customDataBase64": "",
            "customDataAskUserFirst": false
        });
        let resp = self.send_json_request("ItemLoadRequest", data).await?;
        check_response(&resp)?;
        let mut fields = BTreeMap::new();
        fields.insert("instance_id".to_owned(), string_field(&resp, "instanceID"));
        fields.insert("file_name".to_owned(), string_field(&resp, "fileName"));
        Ok(Variant::Object(fields))
    }

    async fn unload_all_items(&self) -> Result<(), VTubeError> {
        let data = json!({ "unloadAllInScene": true });
        let resp = self.send_json_request("ItemUnloadRequest", data).await?;
        check_response(&resp)
    }

    async fn tint_all_art_meshes(
        &self,
        color_r: i64,
        color_g: i64,
        color_b: i64,
        color_a: i64,
        mix_with_scene_lighting: Option<f64>,
    ) -> Result<(), VTubeError> {
        let mut color_tint = json!({
            "colorR": color_r,
            "colorG": color_g,
            "colorB": color_b,
            "colorA": color_a
        });
        if let Some(v) = mix_with_scene_lighting {
            color_tint["mixWithSceneLightingColor"] = json!(v);
        }
        let data = json!({
            "colorTint": color_tint,
            "artMeshMatcher": { "tintAll": true }
        });
        let resp = self.send_json_request("ColorTintRequest", data).await?;
        check_response(&resp)
    }

    async fn set_physics_override(
        &self,
        strength: f64,
        override_seconds: f64,
    ) -> Result<(), VTubeError> {
        let data = json!({
            "strengthOverrides": [{
                "id": "",
                "value": strength,
                "setBaseValue": true,
                "overrideSeconds": override_seconds
            }],
            "windOverrides": []
        });
        let resp = self
            .send_json_request("SetCurrentModelPhysicsRequest", data)
            .await?;
        check_response(&resp)
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

fn check_items_moved(data: &Value) -> Result<(), VTubeError> {
    let failed: Vec<(String, i64)> = data["movedItems"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["success"].as_bool() == Some(false))
        .map(|entry| {
            (
                entry["itemInstanceID"].as_str().unwrap_or("").to_owned(),
                entry["errorID"].as_i64().unwrap_or(ITEM_ERROR_ID_UNKNOWN),
            )
        })
        .collect();
    match failed.first() {
        None => Ok(()),
        Some((_, error_id)) => {
            let items: Vec<&str> = failed.iter().map(|(id, _)| id.as_str()).collect();
            Err(VTubeError::Rejected {
                error_id: *error_id,
                message: format!("could not move item {}", items.join(", ")),
            })
        }
    }
}

const ITEM_ERROR_ID_UNKNOWN: i64 = -1;
const ITEM_IGNORE_SENTINEL: f64 = -1000.0;
const ITEM_IGNORE_SENTINEL_ORDER: i64 = -1000;
const EXPRESSION_ERROR_ID_UNKNOWN: i64 = -1;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use super::*;
    use crate::client::tests::{FakeVts, MockPublisher, PeerConn, stored_token_creds, wait_paused};
    use crate::request::REQUEST_TIMEOUT;

    const SETTLE: Duration = Duration::from_secs(5);
    const REJECTION_ERROR_ID: i64 = 452;
    const ITEM_ERROR_ID: i64 = 751;

    async fn connected_client(vts: &mut FakeVts) -> (VTubeClient, PeerConn) {
        let client = vts.connect(&MockPublisher::new(), &stored_token_creds());
        let conn = vts.logged_in_conn().await;
        assert!(
            wait_paused(SETTLE, || client.connection_state().is_connected()).await,
            "the session never reached connected"
        );
        (client, conn)
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_vts_rejects_fails_with_the_error_id_vts_sent() {
        let mut vts = FakeVts::bind().await;
        let (client, mut conn) = connected_client(&mut vts).await;

        let (outcome, ()) = tokio::join!(client.trigger_hotkey("hk-1"), async {
            let request = conn.expect("HotkeyTriggerRequest", SETTLE).await;
            conn.tx
                .api_error(&request, REJECTION_ERROR_ID, "Hotkey is on cooldown");
        });

        assert!(
            matches!(
                outcome,
                Err(VTubeError::Rejected { error_id, .. }) if error_id == REJECTION_ERROR_ID
            ),
            "got {outcome:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_the_peer_never_answers_times_out_at_the_request_deadline() {
        let mut vts = FakeVts::bind().await;
        let (client, conn) = connected_client(&mut vts).await;
        let _peer = conn.answer_all_except(|f| f["messageType"] == "HotkeyTriggerRequest");
        let sent_at = tokio::time::Instant::now();

        let outcome = client.trigger_hotkey("hk-1").await;

        assert!(
            matches!(outcome, Err(VTubeError::Timeout)),
            "got {outcome:?}"
        );
        assert_eq!(sent_at.elapsed(), REQUEST_TIMEOUT);
    }

    #[tokio::test(start_paused = true)]
    async fn moving_an_item_fails_only_when_vts_reports_that_item_unmoved() {
        let mut vts = FakeVts::bind().await;
        let (client, mut conn) = connected_client(&mut vts).await;

        for (moved, expect_ok) in [(true, true), (false, false)] {
            let (outcome, ()) = tokio::join!(
                client.move_item("inst-7", Some(0.5), None, None, None, None, 0.0, "linear"),
                async {
                    let request = conn.expect("ItemMoveRequest", SETTLE).await;
                    let entry = if moved {
                        json!({ "itemInstanceID": "inst-7", "success": true, "errorID": -1 })
                    } else {
                        json!({ "itemInstanceID": "inst-7", "success": false, "errorID": ITEM_ERROR_ID })
                    };
                    conn.tx.reply(&request, json!({ "movedItems": [entry] }));
                }
            );

            if expect_ok {
                assert!(outcome.is_ok(), "moved item reported {outcome:?}");
            } else {
                let Err(VTubeError::Rejected { error_id, message }) = outcome else {
                    panic!("an unmoved item must be rejected, got {outcome:?}");
                };
                assert_eq!(error_id, ITEM_ERROR_ID);
                assert!(
                    message.contains("inst-7"),
                    "message {message:?} must name the item"
                );
            }
        }
    }
}
