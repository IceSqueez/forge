use std::sync::{Arc, RwLock};

use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use forge_types::VerbatimText;

use forge_platform_core::{
    BuiltinHealth, HealthDelta, HealthMetric, HealthMetricId, HealthStream, HealthText, HealthValue,
};

use crate::client::VTubeClient;
use crate::vocabulary::messages;
use crate::vts::event::VtsEvent;

#[derive(Debug, Clone, Default)]
pub(crate) struct HealthSnapshot {
    pub model_name: String,
    pub model_loaded: bool,
    pub tracking_active: bool,
    pub dialing: bool,
}

pub(crate) fn make_health_channel() -> (broadcast::Sender<HealthDelta>, Arc<RwLock<HealthSnapshot>>)
{
    let (tx, _) = broadcast::channel(forge_platform_core::HEALTH_CHANNEL_CAPACITY);
    (tx, Arc::new(RwLock::new(HealthSnapshot::default())))
}

pub(crate) const MODEL_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_MODEL);
pub(crate) const EXPRESSIONS_METRIC: HealthMetricId =
    HealthMetricId::new(messages::HEALTH_EXPRESSIONS);
pub(crate) const ITEMS_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_ITEMS);
pub(crate) const TRACKING_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_TRACKING);

pub(crate) fn update_from_event(
    event: &VtsEvent,
    snap: &Arc<RwLock<HealthSnapshot>>,
    tx: &broadcast::Sender<HealthDelta>,
) {
    match event {
        VtsEvent::TrackingStatusChanged { face_found, .. } => {
            apply_tracking(*face_found, snap, tx);
        }
        VtsEvent::ModelLoaded {
            loaded, model_name, ..
        } => {
            apply_model(*loaded, model_name, None, snap, tx);
        }
        _ => {}
    }
}

pub(crate) fn apply_tracking(
    face_found: bool,
    snap: &Arc<RwLock<HealthSnapshot>>,
    tx: &broadcast::Sender<HealthDelta>,
) {
    let mut changed = false;
    if let Ok(mut s) = snap.write()
        && s.tracking_active != face_found
    {
        s.tracking_active = face_found;
        changed = true;
    }
    if changed {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "having no subscribers is a normal state"
        )]
        let _ = tx.send(HealthDelta {
            metric: TRACKING_METRIC,
            new_value: tracking_value(face_found),
        });
    }
}

pub(crate) fn apply_model(
    loaded: bool,
    name: &str,
    param_count: Option<u64>,
    snap: &Arc<RwLock<HealthSnapshot>>,
    tx: &broadcast::Sender<HealthDelta>,
) {
    let new_name = if loaded {
        name.to_owned()
    } else {
        String::new()
    };
    let mut changed = false;
    if let Ok(mut s) = snap.write()
        && (s.model_loaded != loaded || s.model_name != new_name)
    {
        s.model_loaded = loaded;
        s.model_name = new_name.clone();
        changed = true;
    }
    if changed {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "having no subscribers is a normal state"
        )]
        let _ = tx.send(HealthDelta {
            metric: MODEL_METRIC,
            new_value: model_value(&new_name, param_count),
        });
    }
}

pub(crate) fn clear_session_state(
    snap: &Arc<RwLock<HealthSnapshot>>,
    tx: &broadcast::Sender<HealthDelta>,
) {
    apply_model(false, "", None, snap, tx);
    apply_tracking(false, snap, tx);
}

pub(crate) fn model_value(name: &str, param_count: Option<u64>) -> HealthValue {
    if name.is_empty() {
        HealthValue::Text {
            primary: HealthText::placeholder(),
            secondary: Some(messages::health_model_not_loaded().into()),
        }
    } else {
        HealthValue::Text {
            primary: HealthText::Text(VerbatimText::new(name).into()),
            secondary: param_count.map(|n| messages::health_model_parameters(n).into()),
        }
    }
}

pub(crate) fn expressions_value(count: usize) -> HealthValue {
    HealthValue::Text {
        primary: HealthText::Count(count as u64),
        secondary: Some(messages::health_expressions_hint().into()),
    }
}

pub(crate) fn items_value(count: u64) -> HealthValue {
    HealthValue::Text {
        primary: HealthText::Count(count),
        secondary: Some(messages::health_items_hint().into()),
    }
}

fn tracking_value(face_found: bool) -> HealthValue {
    HealthValue::Status {
        label: messages::health_tracking(face_found).into(),
        active: face_found,
        detail: None,
    }
}

impl BuiltinHealth for VTubeClient {
    fn metrics(&self) -> [HealthMetric; 4] {
        let snap = self
            .health_state
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let content = self
            .content_state
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();

        let model_name = if snap.model_loaded {
            snap.model_name.as_str()
        } else {
            ""
        };

        [
            HealthMetric {
                id: MODEL_METRIC,
                value: model_value(model_name, content.current_model_param_count.map(u64::from)),
            },
            HealthMetric {
                id: EXPRESSIONS_METRIC,
                value: expressions_value(content.expressions.len()),
            },
            HealthMetric {
                id: ITEMS_METRIC,
                value: items_value(content.item_count.map_or(0, u64::from)),
            },
            HealthMetric {
                id: TRACKING_METRIC,
                value: tracking_value(snap.tracking_active),
            },
        ]
    }

    fn stream(&self) -> HealthStream {
        let rx = self.health_tx.subscribe();
        Box::pin(BroadcastStream::new(rx).filter_map(|r| r.ok()))
    }
}

#[cfg(test)]
mod tests {

    use forge_platform_core::{BuiltinHealth, HealthValue};

    use super::*;
    use crate::client::VTubeClient;
    use crate::content::ExpressionItem;

    fn model_name(name: &str) -> HealthText {
        HealthText::Text(VerbatimText::new(name).into())
    }

    fn no_model() -> HealthValue {
        HealthValue::Text {
            primary: HealthText::placeholder(),
            secondary: Some(messages::health_model_not_loaded().into()),
        }
    }

    fn model_loaded(model_name: &str) -> VtsEvent {
        VtsEvent::ModelLoaded {
            loaded: true,
            model_id: "m1".to_owned(),
            model_name: model_name.to_owned(),
        }
    }

    fn tracking(face_found: bool) -> VtsEvent {
        VtsEvent::TrackingStatusChanged {
            face_found,
            left_hand_found: false,
            right_hand_found: false,
        }
    }

    fn expression_item() -> ExpressionItem {
        ExpressionItem {
            name: "Blush".to_owned(),
            file: "blush.exp3.json".to_owned(),
            active: false,
        }
    }

    #[test]
    fn the_model_loaded_delta_addresses_the_model_metric() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let mut rx = c.health_tx.subscribe();

        let event = model_loaded("MyAvatar");
        update_from_event(&event, &c.health_state, &c.health_tx);

        let delta = rx.try_recv().unwrap();
        assert_eq!(delta.metric, MODEL_METRIC);
        assert!(matches!(
            delta.new_value,
            HealthValue::Text { ref primary, .. } if *primary == model_name("MyAvatar")
        ));
    }

    #[test]
    fn the_tracking_delta_addresses_the_tracking_metric() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let mut rx = c.health_tx.subscribe();

        let event = tracking(true);
        update_from_event(&event, &c.health_state, &c.health_tx);

        let delta = rx.try_recv().unwrap();
        assert_eq!(delta.metric, TRACKING_METRIC);
        assert!(matches!(
            delta.new_value,
            HealthValue::Status { active: true, .. }
        ));
    }

    #[test]
    fn metrics_declare_unique_catalogued_ids() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");

        let ids: std::collections::HashSet<_> =
            c.metrics().into_iter().map(|metric| metric.id).collect();

        assert_eq!(ids.len(), 4);
        for id in &ids {
            assert!(
                crate::vocabulary::messages::VTUBE_MESSAGES
                    .codes
                    .contains(id.label().code()),
                "{id} is not catalogued"
            );
        }
    }

    #[test]
    fn no_delta_when_model_state_unchanged() {
        let (tx, snap) = make_health_channel();
        let mut rx = tx.subscribe();

        let event = model_loaded("SameAvatar");
        update_from_event(&event, &snap, &tx);
        assert!(rx.try_recv().is_ok());

        update_from_event(&event, &snap, &tx);
        assert!(
            rx.try_recv().is_err(),
            "second call with identical state must not emit a delta"
        );
    }

    #[test]
    fn no_delta_when_tracking_unchanged() {
        let (tx, snap) = make_health_channel();
        let mut rx = tx.subscribe();

        let event = tracking(false);
        update_from_event(&event, &snap, &tx);
        assert!(
            rx.try_recv().is_err(),
            "default state is false; identical update must not emit"
        );
    }

    fn metric_value(metrics: &[HealthMetric], id: &HealthMetricId) -> HealthValue {
        metrics
            .iter()
            .find(|m| &m.id == id)
            .unwrap_or_else(|| panic!("no metric {id}"))
            .value
            .clone()
    }

    fn primary(value: HealthValue) -> HealthText {
        match value {
            HealthValue::Text { primary, .. } => primary,
            other => panic!("not a text value: {other:?}"),
        }
    }

    #[test]
    fn metrics_report_expression_and_item_counts_from_the_content_snapshot() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        if let Ok(mut s) = c.content_state.write() {
            s.expressions = vec![expression_item(), expression_item(), expression_item()];
            s.item_count = Some(7);
        }

        let metrics = c.metrics();
        assert_eq!(
            primary(metric_value(&metrics, &EXPRESSIONS_METRIC)),
            HealthText::Count(3)
        );
        assert_eq!(
            primary(metric_value(&metrics, &ITEMS_METRIC)),
            HealthText::Count(7)
        );
    }

    #[test]
    fn a_loaded_model_is_annotated_with_its_live2d_parameter_count() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        if let Ok(mut s) = c.health_state.write() {
            s.model_loaded = true;
            s.model_name = "MyAvatar".to_owned();
        }
        if let Ok(mut s) = c.content_state.write() {
            s.current_model_param_count = Some(42);
        }

        assert_eq!(
            metric_value(&c.metrics(), &MODEL_METRIC),
            HealthValue::Text {
                primary: model_name("MyAvatar"),
                secondary: Some(messages::health_model_parameters(42).into()),
            }
        );
    }

    #[test]
    fn a_loaded_model_without_a_known_parameter_count_shows_the_name_alone() {
        assert_eq!(
            model_value("MyAvatar", None),
            HealthValue::Text {
                primary: model_name("MyAvatar"),
                secondary: None,
            }
        );
    }

    #[test]
    fn an_empty_snapshot_reports_no_model_and_no_items_rather_than_a_blank_slot() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let metrics = c.metrics();

        assert_eq!(metric_value(&metrics, &MODEL_METRIC), no_model());
        assert_eq!(
            primary(metric_value(&metrics, &ITEMS_METRIC)),
            HealthText::Count(0)
        );
    }

    #[test]
    fn a_stale_name_of_an_unloaded_model_is_not_shown_on_the_tile() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        if let Ok(mut s) = c.health_state.write() {
            s.model_loaded = false;
            s.model_name = "GoneAvatar".to_owned();
        }

        assert_eq!(metric_value(&c.metrics(), &MODEL_METRIC), no_model());
    }

    #[test]
    fn a_loaded_model_without_a_name_reads_as_not_loaded() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        if let Ok(mut s) = c.health_state.write() {
            s.model_loaded = true;
            s.model_name = String::new();
        }

        assert_eq!(metric_value(&c.metrics(), &MODEL_METRIC), no_model());
    }

    #[test]
    fn unloading_a_model_publishes_the_not_loaded_tile() {
        let (tx, snap) = make_health_channel();
        apply_model(true, "MyAvatar", Some(42), &snap, &tx);
        let mut rx = tx.subscribe();

        apply_model(false, "MyAvatar", Some(42), &snap, &tx);

        assert_eq!(rx.try_recv().unwrap().new_value, no_model());
    }

    #[tokio::test]
    async fn send_json_request_returns_not_connected_when_disconnected() {
        use crate::error::VTubeError;
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let result = c
            .send_json_request("TestRequest", serde_json::json!({}))
            .await;
        assert!(
            matches!(result, Err(VTubeError::NotConnected)),
            "disconnected client must return NotConnected"
        );
    }
}
