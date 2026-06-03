use std::collections::VecDeque;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, RwLock};

use tokio::sync::{broadcast, mpsc};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use forge_platform_core::{BuiltinHealth, HealthDelta, HealthMetric, HealthStream, HealthValue};

use crate::client::{STATE_CONNECTED, VTubeClient};
use crate::events::RawEnvelope;
use crate::protocol::new_request;
use crate::request::PendingRequest;

#[derive(Debug, Clone, Default)]
pub(crate) struct HealthSnapshot {
    pub model_name: String,
    pub model_loaded: bool,
    pub tracking_active: bool,
    pub fps: f64,
    pub api_calls_60s: u32,
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
        "FaceFoundEvent" => {
            let found = env.data["found"].as_bool().unwrap_or(false);
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
                    index: 1,
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
                let primary = if loaded && !new_name.is_empty() {
                    new_name
                } else {
                    "\u{2014}".to_owned()
                };
                let _ = tx.send(HealthDelta {
                    index: 0,
                    new_value: HealthValue::Text {
                        primary,
                        secondary: None,
                    },
                });
            }
        }
        _ => {}
    }
}

pub(crate) fn spawn_health_task(
    snap: Arc<RwLock<HealthSnapshot>>,
    tx: broadcast::Sender<HealthDelta>,
    req_tx: mpsc::UnboundedSender<PendingRequest>,
    api_call_rx: mpsc::UnboundedReceiver<()>,
    connection_state: Arc<AtomicU8>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run_health_task(
        snap,
        tx,
        req_tx,
        api_call_rx,
        connection_state,
    ))
}

async fn run_health_task(
    snap: Arc<RwLock<HealthSnapshot>>,
    tx: broadcast::Sender<HealthDelta>,
    req_tx: mpsc::UnboundedSender<PendingRequest>,
    mut api_call_rx: mpsc::UnboundedReceiver<()>,
    connection_state: Arc<AtomicU8>,
) {
    use tokio::time::{Duration, interval};

    let mut stats_tick = interval(Duration::from_secs(2));
    let mut api_timestamps: VecDeque<tokio::time::Instant> = VecDeque::new();

    loop {
        tokio::select! {
            result = api_call_rx.recv() => {
                match result {
                    Some(()) => {
                        api_timestamps.push_back(tokio::time::Instant::now());
                        let cutoff =
                            tokio::time::Instant::now() - Duration::from_secs(60);
                        while api_timestamps
                            .front()
                            .is_some_and(|t| *t < cutoff)
                        {
                            api_timestamps.pop_front();
                        }
                        let count = api_timestamps.len() as u32;
                        let mut changed = false;
                        if let Ok(mut s) = snap.write()
                            && s.api_calls_60s != count
                        {
                            s.api_calls_60s = count;
                            changed = true;
                        }
                        if changed {
                            let _ = tx.send(HealthDelta {
                                index: 3,
                                new_value: HealthValue::Text {
                                    primary: count.to_string(),
                                    secondary: Some("last 60 s".to_owned()),
                                },
                            });
                        }
                    }
                    None => return,
                }
            }
            _ = stats_tick.tick() => {
                if connection_state.load(Ordering::Acquire) == STATE_CONNECTED {
                    poll_stats(&snap, &tx, &req_tx).await;
                }
            }
        }
    }
}

async fn poll_stats(
    snap: &Arc<RwLock<HealthSnapshot>>,
    tx: &broadcast::Sender<HealthDelta>,
    req_tx: &mpsc::UnboundedSender<PendingRequest>,
) {
    let req = new_request("StatisticsRequest", serde_json::json!({}));
    let request_id = req.request_id.clone();
    let Ok(payload) = serde_json::to_string(&req) else {
        return;
    };
    let (respond_to, rx) = tokio::sync::oneshot::channel();
    if req_tx
        .send(PendingRequest {
            request_id,
            payload,
            respond_to,
        })
        .is_err()
    {
        return;
    }
    let data = match tokio::time::timeout(tokio::time::Duration::from_secs(5), rx).await {
        Ok(Ok(d)) => d,
        _ => return,
    };
    let fps = data["framerate"].as_f64().unwrap_or(0.0);
    let mut changed = false;
    if let Ok(mut s) = snap.write()
        && (s.fps - fps).abs() > f64::EPSILON
    {
        s.fps = fps;
        changed = true;
    }
    if changed {
        let _ = tx.send(HealthDelta {
            index: 2,
            new_value: HealthValue::Text {
                primary: format!("{fps:.1} fps"),
                secondary: None,
            },
        });
    }
}

impl BuiltinHealth for VTubeClient {
    fn metrics(&self) -> [HealthMetric; 4] {
        let snap = self
            .health_state
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();

        let model_primary = if snap.model_loaded && !snap.model_name.is_empty() {
            snap.model_name.clone()
        } else {
            "\u{2014}".to_owned()
        };
        let tracking_label = if snap.tracking_active { "Face" } else { "Off" };

        [
            HealthMetric {
                label: "CURRENT MODEL".to_owned(),
                value: HealthValue::Text {
                    primary: model_primary,
                    secondary: None,
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
            HealthMetric {
                label: "FPS".to_owned(),
                value: HealthValue::Text {
                    primary: format!("{:.1} fps", snap.fps),
                    secondary: None,
                },
            },
            HealthMetric {
                label: "API CALLS".to_owned(),
                value: HealthValue::Text {
                    primary: snap.api_calls_60s.to_string(),
                    secondary: Some("last 60 s".to_owned()),
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
    use std::sync::atomic::AtomicU8;

    use tokio::sync::mpsc;
    use tokio_stream::StreamExt as _;

    use forge_platform_core::{BuiltinHealth, HealthValue};

    use super::*;
    use crate::client::{STATE_CONNECTED, STATE_DISCONNECTED, VTubeClient};
    use crate::events::RawEnvelope;

    fn make_envelope(message_type: &str, data: serde_json::Value) -> RawEnvelope {
        RawEnvelope {
            message_type: message_type.to_owned(),
            data,
        }
    }

    #[test]
    fn metrics_returns_four_with_correct_labels() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let health: &dyn BuiltinHealth = &c;
        let m = health.metrics();
        assert_eq!(m.len(), 4);
        assert_eq!(m[0].label, "CURRENT MODEL");
        assert_eq!(m[1].label, "TRACKING");
        assert_eq!(m[2].label, "FPS");
        assert_eq!(m[3].label, "API CALLS");
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

        let env = make_envelope("FaceFoundEvent", serde_json::json!({ "found": true }));
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

        let env = make_envelope("FaceFoundEvent", serde_json::json!({ "found": false }));
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
        let connection_state = Arc::new(AtomicU8::new(STATE_DISCONNECTED));

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
        let connection_state = Arc::new(AtomicU8::new(STATE_CONNECTED));

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
