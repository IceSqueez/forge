use std::sync::{Arc, OnceLock, RwLock};

use tokio::sync::{broadcast, mpsc};

use forge_platform_core::{BuiltinContent, DetailSection, HealthDelta, HealthValue, SectionIcon};

use crate::client::VTubeClient;
use crate::health::HealthSnapshot;
use crate::protocol::new_request;
use crate::request::{PendingRequest, ReqTxHandle};

#[derive(Debug, Clone, Default)]
pub(crate) struct ContentSnapshot {
    pub models: Vec<ModelItem>,
    pub current_model_id: Option<String>,
    pub current_model_param_count: Option<u32>,
    pub hotkeys: Vec<HotkeyItem>,
    pub expressions: Vec<ExpressionItem>,
    pub item_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelItem {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HotkeyItem {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExpressionItem {
    pub name: String,
    pub file: String,
    pub active: bool,
}

#[derive(Clone)]
pub(crate) struct ContentNotifier {
    model_changed_tx: mpsc::UnboundedSender<()>,
}

impl ContentNotifier {
    pub(crate) fn new() -> (Self, mpsc::UnboundedReceiver<()>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                model_changed_tx: tx,
            },
            rx,
        )
    }

    #[cfg(test)]
    pub(crate) fn noop() -> Self {
        let (tx, _) = mpsc::unbounded_channel();
        Self {
            model_changed_tx: tx,
        }
    }

    pub(crate) fn notify_model_changed(&self) {
        let _ = self.model_changed_tx.send(());
    }
}

pub(crate) fn spawn_content_task(
    snap: Arc<RwLock<ContentSnapshot>>,
    req_tx: impl Into<ReqTxHandle>,
    model_changed_rx: mpsc::UnboundedReceiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run_content_task(snap, req_tx.into(), model_changed_rx))
}

async fn run_content_task(
    snap: Arc<RwLock<ContentSnapshot>>,
    req_tx: ReqTxHandle,
    mut model_changed_rx: mpsc::UnboundedReceiver<()>,
) {
    refresh_models_and_hotkeys(&snap, &req_tx.current().await).await;

    while let Some(()) = model_changed_rx.recv().await {
        refresh_models_and_hotkeys(&snap, &req_tx.current().await).await;
    }
}

pub(crate) fn spawn_catalog_metrics_task(
    content_state: Arc<RwLock<ContentSnapshot>>,
    health_state: Arc<RwLock<HealthSnapshot>>,
    req_tx: impl Into<ReqTxHandle>,
    health_tx: broadcast::Sender<HealthDelta>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run_catalog_metrics_task(
        content_state,
        health_state,
        req_tx.into(),
        health_tx,
    ))
}

async fn run_catalog_metrics_task(
    content_state: Arc<RwLock<ContentSnapshot>>,
    health_state: Arc<RwLock<HealthSnapshot>>,
    req_tx: ReqTxHandle,
    health_tx: broadcast::Sender<HealthDelta>,
) {
    use tokio::time::{Duration, MissedTickBehavior, interval};

    let mut tick = interval(Duration::from_secs(5));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut last_param_count: Option<Option<u32>> = None;
    let mut last_model_id: Option<Option<String>> = None;

    loop {
        tick.tick().await;

        let dialing = health_state.read().map(|s| s.dialing).unwrap_or(false);
        if !dialing {
            let tx = req_tx.current().await;
            report_expressions(&content_state, &tx, &health_tx).await;
            report_items(&content_state, &tx, &health_tx).await;
        }

        let current_model_id = content_state
            .read()
            .map(|s| s.current_model_id.clone())
            .unwrap_or(None);
        if last_model_id != Some(current_model_id.clone()) {
            last_param_count = None;
        }
        last_model_id = Some(current_model_id);

        report_model_secondary(
            &content_state,
            &health_state,
            &health_tx,
            &mut last_param_count,
        );
    }
}

async fn report_expressions(
    snap: &Arc<RwLock<ContentSnapshot>>,
    req_tx: &mpsc::UnboundedSender<PendingRequest>,
    health_tx: &broadcast::Sender<HealthDelta>,
) {
    let before = snap.read().map(|s| s.expressions.len()).unwrap_or(0);
    refresh_expressions(snap, req_tx).await;
    let after = snap.read().map(|s| s.expressions.len()).unwrap_or(before);
    if after != before {
        let _ = health_tx.send(HealthDelta {
            index: 1,
            new_value: HealthValue::Text {
                primary: after.to_string(),
                secondary: Some("hotkey-bound".to_owned()),
            },
        });
    }
}

async fn report_items(
    snap: &Arc<RwLock<ContentSnapshot>>,
    req_tx: &mpsc::UnboundedSender<PendingRequest>,
    health_tx: &broadcast::Sender<HealthDelta>,
) {
    let before = snap.read().map(|s| s.item_count).unwrap_or(None);
    refresh_items(snap, req_tx).await;
    let after = snap.read().map(|s| s.item_count).unwrap_or(before);
    if after != before {
        let _ = health_tx.send(HealthDelta {
            index: 2,
            new_value: HealthValue::Text {
                primary: after.unwrap_or(0).to_string(),
                secondary: Some("throwable / pinned".to_owned()),
            },
        });
    }
}

fn report_model_secondary(
    content_state: &Arc<RwLock<ContentSnapshot>>,
    health_state: &Arc<RwLock<HealthSnapshot>>,
    health_tx: &broadcast::Sender<HealthDelta>,
    last_param_count: &mut Option<Option<u32>>,
) {
    let param_count = content_state
        .read()
        .map(|s| s.current_model_param_count)
        .unwrap_or(None);
    if *last_param_count == Some(param_count) {
        return;
    }
    *last_param_count = Some(param_count);

    let Ok(health) = health_state.read() else {
        return;
    };
    if !health.model_loaded || health.model_name.is_empty() {
        return;
    }
    let primary = health.model_name.clone();
    drop(health);

    let _ = health_tx.send(HealthDelta {
        index: 0,
        new_value: HealthValue::Text {
            primary,
            secondary: param_count.map(|n| format!("{n} parameters")),
        },
    });
}

async fn refresh_models_and_hotkeys(
    snap: &Arc<RwLock<ContentSnapshot>>,
    req_tx: &mpsc::UnboundedSender<PendingRequest>,
) {
    if let Ok(data) = send_internal(req_tx, "AvailableModelsRequest", serde_json::json!({})).await {
        let models: Vec<ModelItem> = data["availableModels"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|m| ModelItem {
                        id: m["modelID"].as_str().unwrap_or("").to_owned(),
                        name: m["modelName"].as_str().unwrap_or("").to_owned(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        if let Ok(mut s) = snap.write() {
            s.models = models;
        }
    }

    if let Ok(data) = send_internal(req_tx, "CurrentModelRequest", serde_json::json!({})).await {
        let loaded = data["modelLoaded"].as_bool().unwrap_or(false);
        let current_id = loaded.then(|| data["modelID"].as_str().unwrap_or("").to_owned());
        let param_count = loaded
            .then(|| data["numberOfLive2DParameters"].as_u64())
            .flatten()
            .map(|n| n as u32);
        if let Ok(mut s) = snap.write() {
            s.current_model_id = current_id;
            s.current_model_param_count = param_count;
        }
    }

    if let Ok(data) = send_internal(
        req_tx,
        "HotkeysInCurrentModelRequest",
        serde_json::json!({}),
    )
    .await
    {
        let hotkeys: Vec<HotkeyItem> = data["availableHotkeys"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|h| HotkeyItem {
                        name: h["name"].as_str().unwrap_or("").to_owned(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        if let Ok(mut s) = snap.write() {
            s.hotkeys = hotkeys;
        }
    }
}

async fn refresh_expressions(
    snap: &Arc<RwLock<ContentSnapshot>>,
    req_tx: &mpsc::UnboundedSender<PendingRequest>,
) {
    let Ok(data) = send_internal(req_tx, "ExpressionStateRequest", serde_json::json!({})).await
    else {
        return;
    };
    let expressions: Vec<ExpressionItem> = data["expressions"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|e| ExpressionItem {
                    name: e["name"].as_str().unwrap_or("").to_owned(),
                    file: e["file"].as_str().unwrap_or("").to_owned(),
                    active: e["active"].as_bool().unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default();
    if let Ok(mut s) = snap.write() {
        s.expressions = expressions;
    }
}

async fn refresh_items(
    snap: &Arc<RwLock<ContentSnapshot>>,
    req_tx: &mpsc::UnboundedSender<PendingRequest>,
) {
    let Ok(data) = send_internal(
        req_tx,
        "ItemListRequest",
        serde_json::json!({
            "includeAvailableSpots": false,
            "includeItemInstancesInScene": false,
            "includeAvailableItemFiles": false,
        }),
    )
    .await
    else {
        return;
    };
    let item_count = data["itemsInSceneCount"].as_u64().map(|n| n as u32);
    if let Ok(mut s) = snap.write() {
        s.item_count = item_count;
    }
}

pub(crate) fn spawn_version_fetch(
    req_tx: impl Into<ReqTxHandle>,
    vtube_version: Arc<OnceLock<String>>,
    mut connected_rx: mpsc::UnboundedReceiver<()>,
) -> tokio::task::JoinHandle<()> {
    let req_tx: ReqTxHandle = req_tx.into();
    tokio::spawn(async move {
        while connected_rx.recv().await.is_some() {
            if vtube_version.get().is_some() {
                continue;
            }
            let tx = req_tx.current().await;
            if let Ok(data) = send_internal(&tx, "APIStateRequest", serde_json::json!({})).await
                && let Some(version) = data["vTubeStudioVersion"].as_str()
            {
                let _ = vtube_version.set(version.to_owned());
            }
        }
    })
}

async fn send_internal(
    req_tx: &mpsc::UnboundedSender<PendingRequest>,
    msg_type: &str,
    data: serde_json::Value,
) -> Result<serde_json::Value, ()> {
    let req = new_request(msg_type, data);
    let request_id = req.request_id.clone();
    let payload = serde_json::to_string(&req).map_err(|_| ())?;
    let (respond_to, rx) = tokio::sync::oneshot::channel();
    req_tx
        .send(PendingRequest {
            request_id,
            payload,
            respond_to,
        })
        .map_err(|_| ())?;
    tokio::time::timeout(tokio::time::Duration::from_secs(5), rx)
        .await
        .map_err(|_| ())?
        .map_err(|_| ())
}

impl BuiltinContent for VTubeClient {
    fn sections(&self) -> Vec<DetailSection> {
        let snap = self
            .content_state
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();

        let expression_names: Vec<String> = snap
            .expressions
            .iter()
            .map(|e| {
                if e.name.is_empty() {
                    e.file.clone()
                } else {
                    e.name.clone()
                }
            })
            .collect();

        vec![DetailSection::ChipGrid {
            title: "Available Expressions".to_owned(),
            icon: SectionIcon::new("mood-smile"),
            chip_icon: SectionIcon::new("mood-smile"),
            items: expression_names,
        }]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::mpsc;

    use forge_platform_core::{BuiltinContent, DetailSection};

    use super::*;
    use crate::client::VTubeClient;

    async fn mock_sequential_handler(
        mut rx: mpsc::UnboundedReceiver<PendingRequest>,
        responses: Vec<serde_json::Value>,
    ) {
        for data in responses {
            if let Some(req) = rx.recv().await {
                let _ = req.respond_to.send(data);
            }
        }
    }

    #[tokio::test]
    async fn available_models_response_populates_models_list() {
        let snap = Arc::new(RwLock::new(ContentSnapshot::default()));
        let (req_tx, req_rx) = mpsc::unbounded_channel::<PendingRequest>();
        let (_, model_changed_rx) = mpsc::unbounded_channel::<()>();

        let responses = vec![
            serde_json::json!({
                "availableModels": [
                    { "modelID": "m1", "modelName": "Avatar1" },
                    { "modelID": "m2", "modelName": "Avatar2" }
                ]
            }),
            serde_json::json!({ "modelLoaded": true, "modelID": "m1" }),
            serde_json::json!({ "availableHotkeys": [] }),
        ];

        tokio::spawn(mock_sequential_handler(req_rx, responses));

        let handle = spawn_content_task(Arc::clone(&snap), req_tx, model_changed_rx);

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        handle.abort();

        let s = snap.read().unwrap();
        assert_eq!(s.models.len(), 2);
        assert_eq!(s.models[0].name, "Avatar1");
        assert_eq!(s.models[1].name, "Avatar2");
        assert_eq!(s.current_model_id.as_deref(), Some("m1"));
    }

    #[tokio::test]
    async fn model_loaded_notification_triggers_catalog_refresh() {
        let snap = Arc::new(RwLock::new(ContentSnapshot::default()));
        let (req_tx, req_rx) = mpsc::unbounded_channel::<PendingRequest>();
        let (notifier, model_changed_rx) = ContentNotifier::new();

        let responses = vec![
            serde_json::json!({ "availableModels": [] }),
            serde_json::json!({ "modelLoaded": false }),
            serde_json::json!({ "availableHotkeys": [] }),
            serde_json::json!({
                "availableModels": [{ "modelID": "m3", "modelName": "NewAvatar" }]
            }),
            serde_json::json!({ "modelLoaded": true, "modelID": "m3" }),
            serde_json::json!({ "availableHotkeys": [] }),
        ];

        tokio::spawn(mock_sequential_handler(req_rx, responses));

        let handle = spawn_content_task(Arc::clone(&snap), req_tx, model_changed_rx);
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        notifier.notify_model_changed();
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        handle.abort();

        let s = snap.read().unwrap();
        assert_eq!(s.models.len(), 1);
        assert_eq!(s.models[0].name, "NewAvatar");
        assert_eq!(s.current_model_id.as_deref(), Some("m3"));
    }

    #[tokio::test]
    async fn expression_state_response_populates_expressions() {
        let snap = Arc::new(RwLock::new(ContentSnapshot::default()));
        let (req_tx, req_rx) = mpsc::unbounded_channel::<PendingRequest>();

        let data = serde_json::json!({
            "expressions": [
                { "file": "blush.exp3.json", "name": "Blush", "active": true },
                { "file": "wink.exp3.json", "name": "Wink", "active": false },
                { "file": "cry.exp3.json", "name": "Cry", "active": false }
            ]
        });

        tokio::spawn(async move {
            let mut rx = req_rx;
            if let Some(req) = rx.recv().await {
                let _ = req.respond_to.send(data);
            }
        });

        refresh_expressions(&snap, &req_tx).await;

        let s = snap.read().unwrap();
        assert_eq!(s.expressions.len(), 3);
        assert!(s.expressions[0].active);
        assert_eq!(s.expressions[0].file, "blush.exp3.json");
        assert!(!s.expressions[1].active);
        assert!(!s.expressions[2].active);
    }

    #[test]
    fn sections_returns_two_column_and_active_list() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let content: &dyn BuiltinContent = &c;
        let sections = content.sections();
        assert_eq!(sections.len(), 2);
        assert!(matches!(sections[0], DetailSection::TwoColumnLists { .. }));
        assert!(matches!(sections[1], DetailSection::ActiveItemList { .. }));
    }
}
