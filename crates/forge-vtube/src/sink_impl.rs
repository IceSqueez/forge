use async_trait::async_trait;
use serde_json::{Value, json};

use crate::client::VTubeClient;
use crate::error::VTubeError;
use crate::sink::{
    CurrentModel, ExpressionList, HotkeyList, ItemList, ItemLoad, ItemMove, ItemPin, LoadedItem,
    ModelMove, ModelTint, ParameterList, VTubeSink,
};
use crate::vts::message_type::{
    COLOR_TINT_REQUEST, CURRENT_MODEL_REQUEST, EXPRESSION_ACTIVATION_REQUEST,
    EXPRESSION_STATE_REQUEST, HOTKEY_TRIGGER_REQUEST, HOTKEYS_IN_CURRENT_MODEL_REQUEST,
    INJECT_PARAMETER_DATA_REQUEST, INPUT_PARAMETER_LIST_REQUEST, ITEM_LIST_REQUEST,
    ITEM_LOAD_REQUEST, ITEM_MOVE_REQUEST, ITEM_PIN_REQUEST, ITEM_UNLOAD_REQUEST,
    MODEL_LOAD_REQUEST, MOVE_MODEL_REQUEST, SET_CURRENT_MODEL_PHYSICS_REQUEST,
};
use crate::vts::protocol::check_response;
use forge_registry::ConfigChoice;

#[async_trait]
impl VTubeSink for VTubeClient {
    async fn trigger_hotkey(&self, hotkey_id: &str) -> Result<(), VTubeError> {
        let data = json!({ "hotkeyID": hotkey_id });
        let resp = self.send_json_request(HOTKEY_TRIGGER_REQUEST, data).await?;
        check_response(&resp)
    }

    async fn set_expression(&self, expression_file: &str, active: bool) -> Result<(), VTubeError> {
        let data = json!({ "expressionFile": expression_file, "active": active });
        let resp = self
            .send_json_request(EXPRESSION_ACTIVATION_REQUEST, data)
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
            .send_json_request(INJECT_PARAMETER_DATA_REQUEST, data)
            .await?;
        check_response(&resp)
    }

    async fn load_model(&self, model_id: &str) -> Result<(), VTubeError> {
        let data = json!({ "modelID": model_id });
        let resp = self.send_json_request(MODEL_LOAD_REQUEST, data).await?;
        check_response(&resp)?;
        self.content.notifier.notify_model_changed();
        Ok(())
    }

    async fn deactivate_all_expressions(&self) -> Result<(), VTubeError> {
        let resp = self
            .send_json_request(EXPRESSION_STATE_REQUEST, json!({}))
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
                    _ => UNKNOWN_ERROR_ID,
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

    async fn move_model(&self, request: &ModelMove) -> Result<(), VTubeError> {
        let mut data = json!({
            "timeInSeconds": request.time_in_seconds,
            "valuesAreRelativeToModel": false
        });
        if let Some(v) = request.x {
            data["positionX"] = json!(v);
        }
        if let Some(v) = request.y {
            data["positionY"] = json!(v);
        }
        if let Some(v) = request.rotation {
            data["rotation"] = json!(v);
        }
        if let Some(v) = request.size {
            data["size"] = json!(v);
        }
        let resp = self.send_json_request(MOVE_MODEL_REQUEST, data).await?;
        check_response(&resp)
    }

    async fn move_item(&self, request: &ItemMove<'_>) -> Result<(), VTubeError> {
        let placement = request.placement;
        let data = json!({
            "itemsToMove": [{
                "itemInstanceID": request.item_instance_id,
                "timeInSeconds": request.time_in_seconds,
                "fadeMode": request.fade_mode.wire_name(),
                "positionX": placement.x.unwrap_or(ITEM_IGNORE_SENTINEL),
                "positionY": placement.y.unwrap_or(ITEM_IGNORE_SENTINEL),
                "size": placement.size.unwrap_or(ITEM_IGNORE_SENTINEL),
                "rotation": placement.rotation.unwrap_or(ITEM_IGNORE_SENTINEL),
                "order": placement.order.unwrap_or(ITEM_IGNORE_SENTINEL_ORDER),
                "setFlip": false,
                "flip": false,
                "userCanStop": false
            }]
        });
        let resp = self.send_json_request(ITEM_MOVE_REQUEST, data).await?;
        check_response(&resp)?;
        check_items_moved(&resp)
    }

    async fn get_current_model(&self) -> Result<CurrentModel, VTubeError> {
        let resp = self
            .send_json_request(CURRENT_MODEL_REQUEST, json!({}))
            .await?;
        check_response(&resp)?;
        Ok(CurrentModel {
            name: string_field(&resp, "modelName"),
            id: string_field(&resp, "modelID"),
            loaded: resp["modelLoaded"].as_bool().unwrap_or(false),
        })
    }

    async fn get_hotkeys(&self) -> Result<HotkeyList, VTubeError> {
        let resp = self
            .send_json_request(HOTKEYS_IN_CURRENT_MODEL_REQUEST, json!({}))
            .await?;
        check_response(&resp)?;
        let entries = resp["availableHotkeys"].as_array();
        Ok(HotkeyList {
            names: string_array(entries, "name"),
            ids: string_array(entries, "hotkeyID"),
        })
    }

    async fn get_expressions(&self) -> Result<ExpressionList, VTubeError> {
        let resp = self
            .send_json_request(EXPRESSION_STATE_REQUEST, json!({}))
            .await?;
        check_response(&resp)?;
        let entries = resp["expressions"].as_array();
        Ok(ExpressionList {
            names: string_array(entries, "name"),
            active: bool_array(entries, "active"),
        })
    }

    async fn get_parameters(&self) -> Result<ParameterList, VTubeError> {
        let resp = self
            .send_json_request(INPUT_PARAMETER_LIST_REQUEST, json!({}))
            .await?;
        check_response(&resp)?;
        let mut names = string_array(resp["defaultParameters"].as_array(), "name");
        names.extend(string_array(resp["customParameters"].as_array(), "name"));
        Ok(ParameterList { names })
    }

    async fn get_items(&self) -> Result<ItemList, VTubeError> {
        let data = json!({
            "includeAvailableSpots": false,
            "includeItemInstancesInScene": true,
            "includeAvailableItemFiles": false
        });
        let resp = self.send_json_request(ITEM_LIST_REQUEST, data).await?;
        check_response(&resp)?;
        let entries = resp["itemInstancesInScene"].as_array();
        Ok(ItemList {
            instance_ids: string_array(entries, "instanceID"),
            file_names: string_array(entries, "fileName"),
        })
    }

    async fn pin_item(&self, request: &ItemPin<'_>) -> Result<(), VTubeError> {
        let mut data = json!({
            "pin": request.pin,
            "itemInstanceID": request.item_instance_id
        });
        if request.pin {
            data["angleRelativeTo"] = json!(request.angle_relative_to.wire_name());
            data["sizeRelativeTo"] = json!(request.size_relative_to.wire_name());
            data["vertexPinType"] = json!(request.vertex_pin_type.wire_name());
            data["pinInfo"] = json!({
                "modelID": request.model_id,
                "artMeshID": request.art_mesh_id,
                "angle": request.angle,
                "size": request.size
            });
        }
        let resp = self.send_json_request(ITEM_PIN_REQUEST, data).await?;
        check_response(&resp)
    }

    async fn load_item(&self, request: &ItemLoad<'_>) -> Result<LoadedItem, VTubeError> {
        let placement = request.placement;
        let data = json!({
            "fileName": request.file_name,
            "positionX": placement.x.unwrap_or(0.0),
            "positionY": placement.y.unwrap_or(0.0),
            "size": placement.size.unwrap_or(DEFAULT_ITEM_SIZE),
            "rotation": placement.rotation.unwrap_or(0.0),
            "fadeTime": request.fade_time.unwrap_or(DEFAULT_ITEM_FADE_SECS),
            "order": placement.order.unwrap_or(DEFAULT_ITEM_ORDER),
            "failIfOrderTaken": false,
            "smoothing": 0,
            "censored": false,
            "flipped": false,
            "locked": false,
            "unloadWhenPluginDisconnects": request.unload_on_disconnect,
            "customDataBase64": "",
            "customDataAskUserFirst": false
        });
        let resp = self.send_json_request(ITEM_LOAD_REQUEST, data).await?;
        check_response(&resp)?;
        Ok(LoadedItem {
            instance_id: string_field(&resp, "instanceID"),
            file_name: string_field(&resp, "fileName"),
        })
    }

    async fn unload_all_items(&self) -> Result<(), VTubeError> {
        let data = json!({ "unloadAllInScene": true });
        let resp = self.send_json_request(ITEM_UNLOAD_REQUEST, data).await?;
        check_response(&resp)
    }

    async fn tint_all_art_meshes(&self, request: &ModelTint) -> Result<(), VTubeError> {
        let mut color_tint = json!({
            "colorR": request.color_r,
            "colorG": request.color_g,
            "colorB": request.color_b,
            "colorA": request.color_a
        });
        if let Some(v) = request.mix_with_scene_lighting {
            color_tint["mixWithSceneLightingColor"] = json!(v);
        }
        let data = json!({
            "colorTint": color_tint,
            "artMeshMatcher": { "tintAll": true }
        });
        let resp = self.send_json_request(COLOR_TINT_REQUEST, data).await?;
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
            .send_json_request(SET_CURRENT_MODEL_PHYSICS_REQUEST, data)
            .await?;
        check_response(&resp)
    }
}

fn string_field(data: &Value, key: &str) -> String {
    data[key].as_str().unwrap_or("").to_owned()
}

fn string_array(entries: Option<&Vec<Value>>, key: &str) -> Vec<String> {
    entries
        .map(|arr| {
            arr.iter()
                .map(|e| e[key].as_str().unwrap_or("").to_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn bool_array(entries: Option<&Vec<Value>>, key: &str) -> Vec<bool> {
    entries
        .map(|arr| {
            arr.iter()
                .map(|e| e[key].as_bool().unwrap_or(false))
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
                entry["errorID"].as_i64().unwrap_or(UNKNOWN_ERROR_ID),
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

const UNKNOWN_ERROR_ID: i64 = -1;
const DEFAULT_ITEM_SIZE: f64 = 0.32;
const DEFAULT_ITEM_FADE_SECS: f64 = 0.5;
const DEFAULT_ITEM_ORDER: i64 = 0;
const ITEM_IGNORE_SENTINEL: f64 = -1000.0;
const ITEM_IGNORE_SENTINEL_ORDER: i64 = -1000;

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use super::*;
    use crate::runners::choices::FadeMode;
    use crate::sink::ItemPlacement;
    use std::sync::atomic::Ordering;

    use crate::client::tests::{
        CapturingPublisher, FakeVts, PeerConn, stored_token_creds, wait_until,
    };
    use crate::timing::VTubeTiming;

    const SETTLE: Duration = forge_test_support::HANG_GUARD;
    const SHORT_REQUEST_TIMEOUT: Duration = Duration::from_millis(100);
    const REJECTION_ERROR_ID: i64 = 452;
    const ITEM_ERROR_ID: i64 = 751;
    const NO_MODEL_ERROR_ID: i64 = 50;
    const EXPRESSION_ERROR_ID: i64 = 400;

    async fn connected_client(vts: &mut FakeVts) -> (VTubeClient, PeerConn) {
        connected_client_with_timing(vts, VTubeTiming::default()).await
    }

    async fn connected_client_with_timing(
        vts: &mut FakeVts,
        timing: VTubeTiming,
    ) -> (VTubeClient, PeerConn) {
        let client =
            vts.connect_with_timing(&CapturingPublisher::shared(), &stored_token_creds(), timing);
        let conn = vts.logged_in_conn().await;
        assert!(
            wait_until(SETTLE, || client.connection_state().is_connected()).await,
            "the session never reached connected"
        );
        (client, conn)
    }

    #[tokio::test]
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

    #[tokio::test]
    async fn a_request_the_peer_never_answers_times_out_at_the_request_deadline() {
        let mut vts = FakeVts::bind().await;
        let timing = VTubeTiming {
            request_timeout: SHORT_REQUEST_TIMEOUT,
            ..VTubeTiming::default()
        };
        let (client, conn) = connected_client_with_timing(&mut vts, timing).await;
        let (_peer, ignored) =
            conn.answer_all_except(|f| f["messageType"] == "HotkeyTriggerRequest");
        let started = std::time::Instant::now();

        let (outcome, ()) = tokio::join!(client.trigger_hotkey("hk-1"), async {
            assert!(
                wait_until(SETTLE, || ignored.load(Ordering::SeqCst) == 1).await,
                "the hotkey request never reached VTS"
            );
        });

        assert!(
            matches!(outcome, Err(VTubeError::Timeout)),
            "got {outcome:?}"
        );
        assert!(
            started.elapsed() >= SHORT_REQUEST_TIMEOUT,
            "the request gave up before its deadline"
        );
    }

    #[tokio::test]
    async fn moving_an_item_fails_only_when_vts_reports_that_item_unmoved() {
        let mut vts = FakeVts::bind().await;
        let (client, mut conn) = connected_client(&mut vts).await;

        let request = ItemMove {
            item_instance_id: "inst-7",
            placement: ItemPlacement {
                x: Some(0.5),
                ..ItemPlacement::default()
            },
            time_in_seconds: 0.0,
            fade_mode: FadeMode::Linear,
        };
        for (moved, expect_ok) in [(true, true), (false, false)] {
            let (outcome, ()) = tokio::join!(client.move_item(&request), async {
                let request = conn.expect("ItemMoveRequest", SETTLE).await;
                let entry = if moved {
                    json!({ "itemInstanceID": "inst-7", "success": true, "errorID": -1 })
                } else {
                    json!({ "itemInstanceID": "inst-7", "success": false, "errorID": ITEM_ERROR_ID })
                };
                conn.tx.reply(&request, json!({ "movedItems": [entry] }));
            });

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

    struct ExpressionScript {
        state: Result<serde_json::Value, i64>,
        reject_deactivation_of: &'static [&'static str],
    }

    async fn reset_against(
        client: &VTubeClient,
        conn: &mut PeerConn,
        script: ExpressionScript,
    ) -> (Result<(), VTubeError>, Vec<(String, serde_json::Value)>) {
        let reset = client.deactivate_all_expressions();
        tokio::pin!(reset);
        let mut activations = Vec::new();
        let outcome = loop {
            tokio::select! {
                outcome = &mut reset => break outcome,
                Some(frame) = conn.next_frame() => match frame["messageType"].as_str() {
                    Some("ExpressionStateRequest") => match &script.state {
                        Ok(expressions) => conn
                            .tx
                            .reply(&frame, json!({ "expressions": expressions })),
                        Err(error_id) => conn.tx.api_error(&frame, *error_id, "No model loaded"),
                    },
                    Some("ExpressionActivationRequest") => {
                        let file = frame["data"]["expressionFile"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned();
                        if script.reject_deactivation_of.contains(&file.as_str()) {
                            conn.tx
                                .api_error(&frame, EXPRESSION_ERROR_ID, "Expression not found");
                        } else {
                            conn.tx.reply(&frame, json!({}));
                        }
                        activations.push((file, frame["data"]["active"].clone()));
                    }
                    _ => conn.tx.reply(&frame, json!({})),
                },
            }
        };
        (outcome, activations)
    }

    #[tokio::test]
    async fn resetting_deactivates_exactly_the_expressions_vts_reports_active() {
        let cases: [(&str, serde_json::Value, &[&str]); 5] = [
            (
                "none active",
                json!([{ "file": "smile.exp3.json", "active": false }]),
                &[],
            ),
            ("no expression list", serde_json::Value::Null, &[]),
            (
                "one active",
                json!([
                    { "file": "smile.exp3.json", "active": false },
                    { "file": "blush.exp3.json", "active": true },
                ]),
                &["blush.exp3.json"],
            ),
            (
                "several active",
                json!([
                    { "file": "smile.exp3.json", "active": true },
                    { "file": "blush.exp3.json", "active": false },
                    { "file": "tears.exp3.json", "active": true },
                    { "file": "angry.exp3.json", "active": true },
                ]),
                &["smile.exp3.json", "tears.exp3.json", "angry.exp3.json"],
            ),
            (
                "entries without a file or an active flag",
                json!([
                    { "active": true },
                    { "file": "smile.exp3.json" },
                    { "file": "tears.exp3.json", "active": true },
                ]),
                &["tears.exp3.json"],
            ),
        ];

        for (case, expressions, expected) in cases {
            let mut vts = FakeVts::bind().await;
            let (client, mut conn) = connected_client(&mut vts).await;

            let (outcome, activations) = reset_against(
                &client,
                &mut conn,
                ExpressionScript {
                    state: Ok(expressions),
                    reject_deactivation_of: &[],
                },
            )
            .await;

            assert!(outcome.is_ok(), "{case}: reset failed with {outcome:?}");
            let expected: Vec<_> = expected
                .iter()
                .map(|file| ((*file).to_owned(), json!(false)))
                .collect();
            assert_eq!(activations, expected, "{case}");
        }
    }

    #[tokio::test]
    async fn a_rejected_state_query_fails_the_reset() {
        let mut vts = FakeVts::bind().await;
        let (client, mut conn) = connected_client(&mut vts).await;

        let (outcome, _) = reset_against(
            &client,
            &mut conn,
            ExpressionScript {
                state: Err(NO_MODEL_ERROR_ID),
                reject_deactivation_of: &[],
            },
        )
        .await;

        assert!(
            matches!(
                outcome,
                Err(VTubeError::Rejected { error_id, .. }) if error_id == NO_MODEL_ERROR_ID
            ),
            "got {outcome:?}"
        );
    }

    async fn reset_rejecting(
        rejected: &'static [&'static str],
    ) -> (Result<(), VTubeError>, Vec<(String, serde_json::Value)>) {
        let mut vts = FakeVts::bind().await;
        let (client, mut conn) = connected_client(&mut vts).await;
        reset_against(
            &client,
            &mut conn,
            ExpressionScript {
                state: Ok(json!([
                    { "file": "smile.exp3.json", "active": true },
                    { "file": "tears.exp3.json", "active": true },
                    { "file": "angry.exp3.json", "active": true },
                    { "file": "blush.exp3.json", "active": true },
                ])),
                reject_deactivation_of: rejected,
            },
        )
        .await
    }

    #[tokio::test]
    async fn a_rejected_deactivation_does_not_stop_the_remaining_ones() {
        let (_, activations) = reset_rejecting(&["smile.exp3.json"]).await;

        let files: Vec<&str> = activations.iter().map(|(file, _)| file.as_str()).collect();
        assert_eq!(
            files,
            [
                "smile.exp3.json",
                "tears.exp3.json",
                "angry.exp3.json",
                "blush.exp3.json"
            ]
        );
    }

    #[tokio::test]
    async fn a_reset_with_rejected_deactivations_fails_naming_only_those_expressions() {
        let (outcome, _) = reset_rejecting(&["tears.exp3.json", "blush.exp3.json"]).await;

        let Err(VTubeError::Rejected { error_id, message }) = outcome else {
            panic!("got {outcome:?}");
        };
        assert_eq!(error_id, EXPRESSION_ERROR_ID);
        let named: Vec<&str> = ["smile", "tears", "angry", "blush"]
            .into_iter()
            .filter(|name| message.contains(&format!("{name}.exp3.json")))
            .collect();
        assert_eq!(named, ["tears", "blush"], "message: {message}");
    }
}
