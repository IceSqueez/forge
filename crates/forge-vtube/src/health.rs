use std::sync::{Arc, RwLock};

use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use forge_platform_core::{BuiltinHealth, HealthDelta, HealthMetric, HealthStream, HealthValue};

use crate::client::VTubeClient;
use crate::events::RawEnvelope;

#[derive(Debug, Clone, Default)]
pub(crate) struct HealthSnapshot {
    pub model_name: String,
    pub model_loaded: bool,
    pub tracking_active: bool,
    /// True while the supervisor is dialing, authenticating or backing off; gates the catalog
    /// sweep so it does not enqueue requests nobody will drain until this clears.
    pub dialing: bool,
}

pub(crate) fn make_health_channel() -> (broadcast::Sender<HealthDelta>, Arc<RwLock<HealthSnapshot>>)
{
    let (tx, _) = broadcast::channel(16);
    (tx, Arc::new(RwLock::new(HealthSnapshot::default())))
}

pub(crate) fn update_from_event(
    env: &RawEnvelope,
    snap: &Arc<RwLock<HealthSnapshot>>,
    tx: &broadcast::Sender<HealthDelta>,
) {
    match env.message_type.as_str() {
        "TrackingStatusChangedEvent" => {
            let found = env.data["faceFound"].as_bool().unwrap_or(false);
            let mut changed = false;
            if let Ok(mut s) = snap.write()
                && s.tracking_active != found
            {
                s.tracking_active = found;
                changed = true;
            }
            if changed {
                let label = if found {
                    "Face".to_owned()
                } else {
                    "Off".to_owned()
                };
                let _ = tx.send(HealthDelta {
                    index: 3,
                    new_value: HealthValue::Status {
                        label,
                        active: found,
                        detail: None,
                    },
                });
            }
        }
        "ModelLoadedEvent" => {
            let loaded = env.data["modelLoaded"].as_bool().unwrap_or(false);
            let new_name = if loaded {
                env.data["modelName"].as_str().unwrap_or("").to_owned()
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
                let (primary, secondary) = if loaded && !new_name.is_empty() {
                    (new_name, None)
                } else {
                    ("\u{2014}".to_owned(), Some("not loaded".to_owned()))
                };
                let _ = tx.send(HealthDelta {
                    index: 0,
                    new_value: HealthValue::Text { primary, secondary },
                });
            }
        }
        _ => {}
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

        let model_loaded = snap.model_loaded && !snap.model_name.is_empty();
        let (model_primary, model_secondary) = if model_loaded {
            (
                snap.model_name.clone(),
                content
                    .current_model_param_count
                    .map(|n| format!("{n} parameters")),
            )
        } else {
            ("\u{2014}".to_owned(), Some("not loaded".to_owned()))
        };
        let tracking_label = if snap.tracking_active { "Face" } else { "Off" };

        [
            HealthMetric {
                label: "MODEL".to_owned(),
                value: HealthValue::Text {
                    primary: model_primary,
                    secondary: model_secondary,
                },
            },
            HealthMetric {
                label: "EXPRESSIONS".to_owned(),
                value: HealthValue::Text {
                    primary: content.expressions.len().to_string(),
                    secondary: Some("hotkey-bound".to_owned()),
                },
            },
            HealthMetric {
                label: "ITEMS".to_owned(),
                value: HealthValue::Text {
                    primary: content.item_count.unwrap_or(0).to_string(),
                    secondary: Some("throwable / pinned".to_owned()),
                },
            },
            HealthMetric {
                label: "TRACKING".to_owned(),
                value: HealthValue::Status {
                    label: tracking_label.to_owned(),
                    active: snap.tracking_active,
                    detail: None,
                },
            },
        ]
    }

    fn stream(&self) -> HealthStream {
        let rx = self.health_tx.subscribe();
        Box::pin(BroadcastStream::new(rx).filter_map(|r| r.ok()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use tokio_stream::StreamExt as _;

    use forge_platform_core::{BuiltinHealth, HealthValue};

    use super::*;
    use crate::client::VTubeClient;
    use crate::content::ExpressionItem;
    use crate::events::RawEnvelope;

    fn make_envelope(message_type: &str, data: serde_json::Value) -> RawEnvelope {
        RawEnvelope {
            message_type: message_type.to_owned(),
            data,
        }
    }

    fn expression_item() -> ExpressionItem {
        ExpressionItem {
            name: "Blush".to_owned(),
            file: "blush.exp3.json".to_owned(),
            active: false,
        }
    }

    #[tokio::test]
    async fn health_stream_is_subscribable() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let health: &dyn BuiltinHealth = &c;
        let items: Vec<_> = health.stream().take(0).collect().await;
        assert!(items.is_empty());
    }

    // Why: the delta index is a hardcoded slot number into the array `metrics()` builds. If the
    // two ever drift, the UI writes the model name into whichever metric now sits at that slot.
    #[test]
    fn the_model_loaded_delta_addresses_the_metric_labelled_model() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let mut rx = c.health_tx.subscribe();

        let env = make_envelope(
            "ModelLoadedEvent",
            serde_json::json!({
                "modelLoaded": true,
                "modelID": "m1",
                "modelName": "MyAvatar"
            }),
        );
        update_from_event(&env, &c.health_state, &c.health_tx);

        let delta = rx.try_recv().unwrap();
        assert_eq!(c.metrics()[usize::from(delta.index)].label, "MODEL");
        assert!(matches!(
            delta.new_value,
            HealthValue::Text { ref primary, .. } if primary == "MyAvatar"
        ));
    }

    #[test]
    fn the_tracking_delta_addresses_the_metric_labelled_tracking() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let mut rx = c.health_tx.subscribe();

        let env = make_envelope(
            "TrackingStatusChangedEvent",
            serde_json::json!({ "faceFound": true }),
        );
        update_from_event(&env, &c.health_state, &c.health_tx);

        let delta = rx.try_recv().unwrap();
        assert_eq!(c.metrics()[usize::from(delta.index)].label, "TRACKING");
        assert!(matches!(
            delta.new_value,
            HealthValue::Status { active: true, .. }
        ));
    }

    #[test]
    fn no_delta_when_model_state_unchanged() {
        let (tx, snap) = make_health_channel();
        let mut rx = tx.subscribe();

        let env = make_envelope(
            "ModelLoadedEvent",
            serde_json::json!({
                "modelLoaded": true,
                "modelID": "m1",
                "modelName": "SameAvatar"
            }),
        );
        update_from_event(&env, &snap, &tx);
        assert!(rx.try_recv().is_ok());

        update_from_event(&env, &snap, &tx);
        assert!(
            rx.try_recv().is_err(),
            "second call with identical state must not emit a delta"
        );
    }

    #[test]
    fn no_delta_when_tracking_unchanged() {
        let (tx, snap) = make_health_channel();
        let mut rx = tx.subscribe();

        let env = make_envelope(
            "TrackingStatusChangedEvent",
            serde_json::json!({ "faceFound": false }),
        );
        update_from_event(&env, &snap, &tx);
        assert!(
            rx.try_recv().is_err(),
            "default state is false; identical update must not emit"
        );
    }

    fn text_metric(metrics: &[HealthMetric], label: &str) -> (String, Option<String>) {
        let metric = metrics
            .iter()
            .find(|m| m.label == label)
            .unwrap_or_else(|| panic!("no metric labelled {label}"));
        match &metric.value {
            HealthValue::Text { primary, secondary } => (primary.clone(), secondary.clone()),
            _ => panic!("metric {label} is not a text value"),
        }
    }

    // Why: both counters live in the CONTENT snapshot, not the health snapshot; wiring them to
    // the health snapshot's own fields would peg them at zero forever.
    #[test]
    fn metrics_report_expression_and_item_counts_from_the_content_snapshot() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        if let Ok(mut s) = c.content_state.write() {
            s.expressions = vec![expression_item(), expression_item(), expression_item()];
            s.item_count = Some(7);
        }

        let metrics = c.metrics();
        assert_eq!(text_metric(&metrics, "EXPRESSIONS").0, "3");
        assert_eq!(text_metric(&metrics, "ITEMS").0, "7");
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
            text_metric(&c.metrics(), "MODEL"),
            ("MyAvatar".to_owned(), Some("42 parameters".to_owned()))
        );
    }

    #[test]
    fn an_empty_snapshot_reports_no_model_and_no_items_rather_than_a_blank_slot() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let metrics = c.metrics();

        assert_eq!(
            text_metric(&metrics, "MODEL"),
            ("\u{2014}".to_owned(), Some("not loaded".to_owned()))
        );
        assert_eq!(text_metric(&metrics, "ITEMS").0, "0");
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
