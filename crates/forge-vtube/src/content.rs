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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::mpsc;

    use forge_platform_core::{BuiltinContent, BuiltinHealth, DetailSection, HealthValue};

    use super::*;
    use crate::client::VTubeClient;
    use crate::client::tests::wait_for;
    use crate::request::ReqTxSlot;

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

    /// Serves `responses` in order, then signals so the test can assert without sleeping.
    fn serve(
        rx: mpsc::UnboundedReceiver<PendingRequest>,
        responses: Vec<serde_json::Value>,
    ) -> tokio::sync::oneshot::Receiver<()> {
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            mock_sequential_handler(rx, responses).await;
            let _ = done_tx.send(());
        });
        done_rx
    }

    async fn await_served(done_rx: tokio::sync::oneshot::Receiver<()>) {
        tokio::time::timeout(tokio::time::Duration::from_secs(5), done_rx)
            .await
            .expect("mock VTS handler must serve every queued response")
            .expect("mock VTS handler must not be dropped");
    }

    #[tokio::test]
    async fn available_models_response_populates_models_list() {
        let snap = Arc::new(RwLock::new(ContentSnapshot::default()));
        let (req_tx, req_rx) = mpsc::unbounded_channel::<PendingRequest>();

        let done = serve(
            req_rx,
            vec![
                serde_json::json!({
                    "availableModels": [
                        { "modelID": "m1", "modelName": "Avatar1" },
                        { "modelID": "m2", "modelName": "Avatar2" }
                    ]
                }),
                serde_json::json!({ "modelLoaded": true, "modelID": "m1" }),
                serde_json::json!({ "availableHotkeys": [] }),
            ],
        );

        refresh_models_and_hotkeys(&snap, &req_tx).await;
        await_served(done).await;

        let s = snap.read().unwrap();
        assert_eq!(s.models.len(), 2);
        assert_eq!(s.models[0].name, "Avatar1");
        assert_eq!(s.models[1].name, "Avatar2");
        assert_eq!(s.current_model_id.as_deref(), Some("m1"));
    }

    // Why: `numberOfLive2DParameters` is only meaningful while a model is loaded; a stale count
    // left behind after an unload would be rendered under the "not loaded" model slot.
    #[tokio::test]
    async fn parameter_count_is_captured_only_while_a_model_is_loaded() {
        for (current_model, expected) in [
            (
                serde_json::json!({
                    "modelLoaded": true,
                    "modelID": "m1",
                    "numberOfLive2DParameters": 37
                }),
                Some(37),
            ),
            (
                serde_json::json!({ "modelLoaded": false, "numberOfLive2DParameters": 37 }),
                None,
            ),
        ] {
            let snap = Arc::new(RwLock::new(ContentSnapshot::default()));
            let (req_tx, req_rx) = mpsc::unbounded_channel::<PendingRequest>();
            let done = serve(
                req_rx,
                vec![
                    serde_json::json!({ "availableModels": [] }),
                    current_model,
                    serde_json::json!({ "availableHotkeys": [] }),
                ],
            );

            refresh_models_and_hotkeys(&snap, &req_tx).await;
            await_served(done).await;

            assert_eq!(snap.read().unwrap().current_model_param_count, expected);
        }
    }

    #[tokio::test]
    async fn model_loaded_notification_triggers_catalog_refresh() {
        let snap = Arc::new(RwLock::new(ContentSnapshot::default()));
        let (req_tx, req_rx) = mpsc::unbounded_channel::<PendingRequest>();
        let (notifier, model_changed_rx) = ContentNotifier::new();

        let done = serve(
            req_rx,
            vec![
                serde_json::json!({ "availableModels": [] }),
                serde_json::json!({ "modelLoaded": false }),
                serde_json::json!({ "availableHotkeys": [] }),
                serde_json::json!({
                    "availableModels": [{ "modelID": "m3", "modelName": "NewAvatar" }]
                }),
                serde_json::json!({ "modelLoaded": true, "modelID": "m3" }),
                serde_json::json!({ "availableHotkeys": [] }),
            ],
        );

        let handle = spawn_content_task(Arc::clone(&snap), req_tx, model_changed_rx);
        notifier.notify_model_changed();
        await_served(done).await;
        handle.abort();

        let s = snap.read().unwrap();
        assert_eq!(s.models.len(), 1);
        assert_eq!(s.models[0].name, "NewAvatar");
        assert_eq!(s.current_model_id.as_deref(), Some("m3"));
    }

    // Why: `itemsInSceneCount` is the only field the ITEMS metric reads; a renamed key would
    // pin the counter at zero with no error anywhere.
    #[tokio::test]
    async fn item_list_response_populates_the_scene_item_count() {
        let snap = Arc::new(RwLock::new(ContentSnapshot::default()));
        let (req_tx, req_rx) = mpsc::unbounded_channel::<PendingRequest>();
        let done = serve(req_rx, vec![serde_json::json!({ "itemsInSceneCount": 4 })]);

        refresh_items(&snap, &req_tx).await;
        await_served(done).await;

        assert_eq!(snap.read().unwrap().item_count, Some(4));
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

    // Why: VTS leaves `name` empty for expressions the user never titled; the raw `.exp3.json`
    // file name is the only label left, and a blank chip is unclickable in the picker.
    #[test]
    fn expression_chips_use_the_display_name_and_fall_back_to_the_file_name() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        if let Ok(mut s) = c.content_state.write() {
            s.expressions = vec![
                ExpressionItem {
                    name: "Blush".to_owned(),
                    file: "blush.exp3.json".to_owned(),
                    active: true,
                },
                ExpressionItem {
                    name: String::new(),
                    file: "wink.exp3.json".to_owned(),
                    active: false,
                },
            ];
        }

        let content: &dyn BuiltinContent = &c;
        let section = content
            .sections()
            .into_iter()
            .next()
            .expect("the connected VTube surface must expose a section");
        let DetailSection::ChipGrid { items, .. } = section else {
            panic!("the connected VTube surface must expose the expressions as a chip grid");
        };

        assert_eq!(items, ["Blush".to_owned(), "wink.exp3.json".to_owned()]);
    }

    // Why: the detail screen snapshots `metrics()` once and afterwards only patches the slot a
    // delta addresses, so a count that never emits a delta stays frozen at its open-time value.
    // The first sweep must also land at startup - a screen opened right after connect would
    // otherwise show zero expressions and zero items for a full interval.
    #[tokio::test]
    async fn the_catalog_sweep_publishes_both_counts_as_soon_as_it_starts() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let mut health_rx = c.health_tx.subscribe();
        let (req_tx, req_rx) = mpsc::unbounded_channel::<PendingRequest>();
        let done = serve(
            req_rx,
            vec![
                serde_json::json!({
                    "expressions": [
                        { "file": "a.exp3.json", "name": "A", "active": false },
                        { "file": "b.exp3.json", "name": "B", "active": false }
                    ]
                }),
                serde_json::json!({ "itemsInSceneCount": 3 }),
            ],
        );

        let handle = spawn_catalog_metrics_task(
            Arc::clone(&c.content_state),
            Arc::clone(&c.health_state),
            req_tx,
            c.health_tx.clone(),
        );
        await_served(done).await;

        let mut reported = Vec::new();
        for _ in 0..2 {
            let delta = tokio::time::timeout(tokio::time::Duration::from_secs(2), health_rx.recv())
                .await
                .expect("the first catalog sweep must run at startup, not one interval later")
                .expect("the health channel must stay open");
            let label = c.metrics()[usize::from(delta.index)].label.clone();
            let HealthValue::Text { primary, .. } = delta.new_value else {
                panic!("{label} must be published as a text metric");
            };
            reported.push((label, primary));
        }
        handle.abort();

        reported.sort();
        assert_eq!(
            reported,
            vec![
                ("EXPRESSIONS".to_owned(), "2".to_owned()),
                ("ITEMS".to_owned(), "3".to_owned()),
            ]
        );
    }

    // Why: the ModelLoadedEvent delta cannot carry the parameter count - when the event lands the
    // content snapshot still holds the outgoing model's count - so it ships a bare name and this
    // sweep is the only thing that restores the count afterwards.
    #[test]
    fn the_model_metric_regains_its_parameter_count_after_a_switch() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        if let Ok(mut s) = c.health_state.write() {
            s.model_loaded = true;
            s.model_name = "NewAvatar".to_owned();
        }
        if let Ok(mut s) = c.content_state.write() {
            s.current_model_param_count = Some(55);
        }
        let mut health_rx = c.health_tx.subscribe();
        let mut last_param_count = Some(Some(42));

        report_model_secondary(
            &c.content_state,
            &c.health_state,
            &c.health_tx,
            &mut last_param_count,
        );

        let delta = health_rx
            .try_recv()
            .expect("a freshly landed parameter count must be published");
        assert_eq!(c.metrics()[usize::from(delta.index)].label, "MODEL");
        assert!(matches!(
            delta.new_value,
            HealthValue::Text { ref primary, ref secondary }
                if primary == "NewAvatar" && secondary.as_deref() == Some("55 parameters")
        ));
    }

    // Why: `update_from_event` already published the em-dash + "not loaded" pair for an unloaded
    // model. A sweep that skipped the loaded-model guard would overwrite it with a blank name.
    #[test]
    fn the_sweep_leaves_an_unloaded_model_slot_alone() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let mut health_rx = c.health_tx.subscribe();
        let mut last_param_count = Some(Some(42));

        report_model_secondary(
            &c.content_state,
            &c.health_state,
            &c.health_tx,
            &mut last_param_count,
        );

        assert!(
            health_rx.try_recv().is_err(),
            "an unloaded model must not be re-announced under a blank name"
        );
    }

    // Why: the fetch used to fire at construction time, racing a handshake that can block on the
    // user physically approving the VTS popup. Its deadline lapsed and the cell stayed empty for
    // the rest of the session.
    #[tokio::test]
    async fn the_version_fetch_waits_until_a_connection_announces_itself() {
        let (req_tx, mut req_rx) = mpsc::unbounded_channel::<PendingRequest>();
        let (connected_tx, connected_rx) = mpsc::unbounded_channel::<()>();
        let version = Arc::new(OnceLock::<String>::new());
        let handle = spawn_version_fetch(req_tx, Arc::clone(&version), connected_rx);

        assert!(
            tokio::time::timeout(tokio::time::Duration::from_millis(50), req_rx.recv())
                .await
                .is_err(),
            "no request may be queued before a connection announces itself"
        );

        let _ = connected_tx.send(());
        let req = tokio::time::timeout(tokio::time::Duration::from_secs(2), req_rx.recv())
            .await
            .expect("a connection announcement must trigger the version fetch")
            .expect("the request channel must stay open");
        let _ = req
            .respond_to
            .send(serde_json::json!({ "vTubeStudioVersion": "1.28.0" }));

        assert!(
            wait_for(|| version.get().is_some()).await,
            "the answered version must reach the shared cell"
        );
        handle.abort();
        assert_eq!(version.get().map(String::as_str), Some("1.28.0"));
    }

    // Why: a fetch whose connection died before answering has no other trigger. Without a retry
    // on the next announcement the version badge stays empty until the app restarts.
    #[tokio::test]
    async fn a_later_connection_retries_a_version_fetch_that_was_never_answered() {
        let (req_tx, mut req_rx) = mpsc::unbounded_channel::<PendingRequest>();
        let (connected_tx, connected_rx) = mpsc::unbounded_channel::<()>();
        let version = Arc::new(OnceLock::<String>::new());
        let handle = spawn_version_fetch(req_tx, Arc::clone(&version), connected_rx);

        let _ = connected_tx.send(());
        let unanswered = tokio::time::timeout(tokio::time::Duration::from_secs(2), req_rx.recv())
            .await
            .expect("the first announcement must trigger a version fetch")
            .expect("the request channel must stay open");
        drop(unanswered);

        let _ = connected_tx.send(());
        let retried = tokio::time::timeout(tokio::time::Duration::from_secs(2), req_rx.recv())
            .await
            .expect("a later connection must retry the unanswered version fetch")
            .expect("the request channel must stay open");
        let _ = retried
            .respond_to
            .send(serde_json::json!({ "vTubeStudioVersion": "1.29.0" }));

        assert!(
            wait_for(|| version.get().is_some()).await,
            "the retried version must reach the shared cell"
        );
        handle.abort();
        assert_eq!(version.get().map(String::as_str), Some("1.29.0"));
    }

    async fn next_text_delta(
        rx: &mut broadcast::Receiver<HealthDelta>,
        c: &VTubeClient,
    ) -> (String, String, Option<String>) {
        let delta = tokio::time::timeout(tokio::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("the catalog sweep must publish a health delta")
            .expect("the health channel must stay open");
        let label = c.metrics()[usize::from(delta.index)].label.clone();
        let HealthValue::Text { primary, secondary } = delta.new_value else {
            panic!("{label} must be published as a text metric");
        };
        (label, primary, secondary)
    }

    // Why: the heal dedups so it does not re-announce an unchanged model every 5 s. Keying that
    // dedup on the parameter count alone silently loses the switch between two models that
    // export the same number of Live2D parameters - ordinary for variants exported from one
    // source model - and the MODEL slot keeps the bare name ModelLoadedEvent shipped.
    #[tokio::test(start_paused = true)]
    async fn a_model_switch_between_equal_parameter_counts_still_republishes_the_count() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        // Dialing keeps the sweep's two requests out of this test, so the 5 s interval is the
        // only timer the paused clock has to step over. The model slot is maintained either way.
        if let Ok(mut s) = c.health_state.write() {
            s.dialing = true;
            s.model_loaded = true;
            s.model_name = "Avatar A".to_owned();
        }
        if let Ok(mut s) = c.content_state.write() {
            s.current_model_id = Some("m-a".to_owned());
            s.current_model_param_count = Some(42);
        }
        let mut health_rx = c.health_tx.subscribe();
        let (req_tx, _req_rx) = mpsc::unbounded_channel::<PendingRequest>();

        let handle = spawn_catalog_metrics_task(
            Arc::clone(&c.content_state),
            Arc::clone(&c.health_state),
            req_tx,
            c.health_tx.clone(),
        );

        assert_eq!(
            next_text_delta(&mut health_rx, &c).await,
            (
                "MODEL".to_owned(),
                "Avatar A".to_owned(),
                Some("42 parameters".to_owned())
            )
        );

        if let Ok(mut s) = c.health_state.write() {
            s.model_name = "Avatar B".to_owned();
        }
        if let Ok(mut s) = c.content_state.write() {
            s.current_model_id = Some("m-b".to_owned());
        }
        tokio::time::advance(tokio::time::Duration::from_secs(6)).await;

        let healed = next_text_delta(&mut health_rx, &c).await;
        handle.abort();
        assert_eq!(
            healed,
            (
                "MODEL".to_owned(),
                "Avatar B".to_owned(),
                Some("42 parameters".to_owned())
            )
        );
    }

    // Why: `req_rx` is drained only inside the supervisor's connected loop. A sweep that fires
    // while the supervisor is dialing, authenticating or backing off queues requests nobody
    // reads, and with auto-reconnect on that backlog grows for as long as VTS stays down.
    #[tokio::test]
    async fn the_catalog_sweep_enqueues_nothing_while_the_supervisor_is_dialing() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        if let Ok(mut s) = c.health_state.write() {
            s.dialing = true;
            s.model_loaded = true;
            s.model_name = "Avatar A".to_owned();
        }
        if let Ok(mut s) = c.content_state.write() {
            s.current_model_param_count = Some(42);
        }
        let mut health_rx = c.health_tx.subscribe();
        let (req_tx, mut req_rx) = mpsc::unbounded_channel::<PendingRequest>();

        let handle = spawn_catalog_metrics_task(
            Arc::clone(&c.content_state),
            Arc::clone(&c.health_state),
            req_tx,
            c.health_tx.clone(),
        );

        // The model slot is published after the sweep in the same pass, so receiving its delta
        // proves the first sweep is already behind us - nothing has to be slept on.
        assert_eq!(next_text_delta(&mut health_rx, &c).await.0, "MODEL");
        let queued = req_rx.try_recv();
        handle.abort();

        assert!(
            matches!(queued, Err(mpsc::error::TryRecvError::Empty)),
            "a sweep that runs while dialing queues requests the supervisor will not drain"
        );
    }

    async fn next_request(rx: &mut mpsc::UnboundedReceiver<PendingRequest>) -> PendingRequest {
        tokio::time::timeout(tokio::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("the sweep must reach the channel the slot currently holds")
            .expect("the request channel must stay open")
    }

    // Why: `reconnect` swaps the sender inside the shared slot instead of respawning the
    // background tasks. A task that resolved its sender once at spawn keeps sending into the
    // channel the retired supervisor dropped, so from the first Reconnect onward the expression
    // and item counts freeze and the model parameter count never refreshes again.
    #[tokio::test(start_paused = true)]
    async fn a_catalog_sweep_reaches_the_request_channel_a_reconnect_swapped_in() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let (retired_tx, mut retired_rx) = mpsc::unbounded_channel::<PendingRequest>();
        let (fresh_tx, mut fresh_rx) = mpsc::unbounded_channel::<PendingRequest>();
        let slot: ReqTxSlot = Arc::new(tokio::sync::Mutex::new(retired_tx));

        let handle = spawn_catalog_metrics_task(
            Arc::clone(&c.content_state),
            Arc::clone(&c.health_state),
            Arc::clone(&slot),
            c.health_tx.clone(),
        );

        // Dropping each request unanswered closes its oneshot, which `send_internal` observes
        // immediately - the first sweep finishes on the original channel with no deadline to
        // step the paused clock over.
        for _ in 0..2 {
            drop(next_request(&mut retired_rx).await);
        }

        // The swap `BuiltinControl::reconnect` performs after the old supervisor drops `req_rx`.
        *slot.lock().await = fresh_tx;
        tokio::time::advance(tokio::time::Duration::from_secs(6)).await;

        let redirected = next_request(&mut fresh_rx).await;
        handle.abort();

        assert!(
            redirected.payload.contains("ExpressionStateRequest"),
            "the first sweep after a reconnect must carry the catalog refresh, got {}",
            redirected.payload
        );
    }
}
