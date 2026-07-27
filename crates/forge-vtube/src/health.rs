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
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::mpsc;
    use tokio_stream::StreamExt as _;

    use forge_platform_core::{BuiltinHealth, ConnectionState, HealthValue};

    use super::*;
    use crate::client::VTubeClient;
    use crate::events::RawEnvelope;

    fn make_envelope(message_type: &str, data: serde_json::Value) -> RawEnvelope {
        RawEnvelope {
            message_type: message_type.to_owned(),
            data,
        }
    }

    #[tokio::test]
    async fn health_stream_is_subscribable() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let health: &dyn BuiltinHealth = &c;
        let items: Vec<_> = health.stream().take(0).collect().await;
        assert!(items.is_empty());
    }

    #[test]
    fn model_loaded_event_emits_delta_at_index_zero() {
        let (tx, snap) = make_health_channel();
        let mut rx = tx.subscribe();

        let env = make_envelope(
            "ModelLoadedEvent",
            serde_json::json!({
                "modelLoaded": true,
                "modelID": "m1",
                "modelName": "MyAvatar"
            }),
        );
        update_from_event(&env, &snap, &tx);

        let delta = rx.try_recv().unwrap();
        assert_eq!(delta.index, 0);
        assert!(matches!(
            delta.new_value,
            HealthValue::Text { ref primary, .. } if primary == "MyAvatar"
        ));
    }

    #[test]
    fn face_found_event_emits_delta_at_index_one() {
        let (tx, snap) = make_health_channel();
        let mut rx = tx.subscribe();

        let env = make_envelope(
            "TrackingStatusChangedEvent",
            serde_json::json!({ "faceFound": true }),
        );
        update_from_event(&env, &snap, &tx);

        let delta = rx.try_recv().unwrap();
        assert_eq!(delta.index, 1);
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

    #[tokio::test]
    async fn health_task_stops_when_api_sender_dropped() {
        let (health_tx, health_snap) = make_health_channel();
        let (req_tx, _req_rx) = mpsc::unbounded_channel();
        let (api_call_tx, api_call_rx) = mpsc::unbounded_channel::<()>();
        let connection_state = Arc::new(AtomicConnectionState::new(ConnectionState::Disconnected));

        let handle = spawn_health_task(
            health_snap,
            health_tx,
            req_tx,
            api_call_rx,
            connection_state,
        );

        drop(api_call_tx);
        let result = tokio::time::timeout(tokio::time::Duration::from_secs(2), handle).await;
        assert!(
            result.is_ok(),
            "health task should exit when api sender drops"
        );
    }

    #[tokio::test]
    async fn health_task_emits_api_calls_delta_on_signal() {
        let (health_tx, health_snap) = make_health_channel();
        let mut delta_rx = health_tx.subscribe();
        let (req_tx, _req_rx) = mpsc::unbounded_channel();
        let (api_call_tx, api_call_rx) = mpsc::unbounded_channel::<()>();
        let connection_state = Arc::new(AtomicConnectionState::new(ConnectionState::Connected));

        let handle = spawn_health_task(
            Arc::clone(&health_snap),
            health_tx,
            req_tx,
            api_call_rx,
            connection_state,
        );

        api_call_tx.send(()).unwrap();

        let delta = tokio::time::timeout(tokio::time::Duration::from_secs(2), async {
            loop {
                if let Ok(d) = delta_rx.try_recv()
                    && d.index == 3
                {
                    return d;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();

        assert_eq!(delta.index, 3);
        assert!(matches!(
            delta.new_value,
            HealthValue::Text { ref primary, .. } if primary == "1"
        ));

        handle.abort();
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
