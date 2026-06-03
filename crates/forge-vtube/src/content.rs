use std::sync::{Arc, RwLock};

use tokio::sync::mpsc;

use forge_platform_core::{
    ActiveRow, BuiltinContent, ContentList, ContentListItem, DetailSection, SectionIcon,
};

use crate::client::VTubeClient;
use crate::protocol::new_request;
use crate::request::PendingRequest;

#[derive(Debug, Clone, Default)]
pub(crate) struct ContentSnapshot {
    pub models: Vec<ModelItem>,
    pub current_model_id: Option<String>,
    pub hotkeys: Vec<HotkeyItem>,
    pub expressions: Vec<ExpressionItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelItem {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HotkeyItem {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExpressionItem {
    pub file: String,
    pub name: String,
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
    req_tx: mpsc::UnboundedSender<PendingRequest>,
    model_changed_rx: mpsc::UnboundedReceiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run_content_task(snap, req_tx, model_changed_rx))
}

async fn run_content_task(
    snap: Arc<RwLock<ContentSnapshot>>,
    req_tx: mpsc::UnboundedSender<PendingRequest>,
    mut model_changed_rx: mpsc::UnboundedReceiver<()>,
) {
    use tokio::time::{Duration, interval};

    refresh_models_and_hotkeys(&snap, &req_tx).await;

    let mut expr_tick = interval(Duration::from_secs(5));
    expr_tick.tick().await;

    loop {
        tokio::select! {
            result = model_changed_rx.recv() => {
                match result {
                    Some(()) => refresh_models_and_hotkeys(&snap, &req_tx).await,
                    None => return,
                }
            }
            _ = expr_tick.tick() => {
                refresh_expressions(&snap, &req_tx).await;
            }
        }
    }
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
        let current_id = if data["modelLoaded"].as_bool().unwrap_or(false) {
            Some(data["modelID"].as_str().unwrap_or("").to_owned())
        } else {
            None
        };
        if let Ok(mut s) = snap.write() {
            s.current_model_id = current_id;
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
                        id: h["hotkeyID"].as_str().unwrap_or("").to_owned(),
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
                    file: e["file"].as_str().unwrap_or("").to_owned(),
                    name: e["name"].as_str().unwrap_or("").to_owned(),
                    active: e["active"].as_bool().unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default();
    if let Ok(mut s) = snap.write() {
        s.expressions = expressions;
    }
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

        let model_count = snap.models.len().to_string();
        let model_items: Vec<ContentListItem> = snap
            .models
            .iter()
            .map(|m| {
                let is_current = snap.current_model_id.as_deref() == Some(m.id.as_str());
                ContentListItem {
                    icon: SectionIcon::new(if is_current { "user-check" } else { "user" }),
                    name: m.name.clone(),
                    monospace_name: false,
                    active: is_current,
                    active_label: if is_current {
                        Some("LOADED".to_owned())
                    } else {
                        None
                    },
                    trailing: vec![],
                    enabled: true,
                }
            })
            .collect();

        let hotkey_count = snap.hotkeys.len().to_string();
        let hotkey_items: Vec<ContentListItem> = snap
            .hotkeys
            .iter()
            .map(|h| ContentListItem {
                icon: SectionIcon::new("bolt"),
                name: h.name.clone(),
                monospace_name: false,
                active: false,
                active_label: None,
                trailing: vec![],
                enabled: true,
            })
            .collect();

        let expression_items: Vec<ActiveRow> = snap
            .expressions
            .iter()
            .map(|e| ActiveRow {
                name: e.file.clone(),
                active: e.active,
                mode_label: None,
            })
            .collect();

        vec![
            DetailSection::TwoColumnLists {
                left: ContentList {
                    title: "Models".to_owned(),
                    icon: SectionIcon::new("user-square"),
                    count_label: Some(model_count),
                    items: model_items,
                    footer: None,
                },
                right: ContentList {
                    title: "Hotkeys".to_owned(),
                    icon: SectionIcon::new("bolt"),
                    count_label: Some(hotkey_count),
                    items: hotkey_items,
                    footer: None,
                },
            },
            DetailSection::ActiveItemList {
                title: "Expressions".to_owned(),
                icon: SectionIcon::new("mood-smile"),
                items: expression_items,
            },
        ]
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
