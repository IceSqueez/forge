use std::sync::{Arc, RwLock};

use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use forge_platform_core::{
    HealthDelta, HealthMetric, HealthStream, HealthValue, BuiltinHealth,
};

use crate::client::ObsClient;

#[derive(Debug, Clone, Default)]
pub struct HealthSnapshot {
    pub stream_active: bool,
    pub record_active: bool,
    pub cpu_percent: f64,
    pub fps: f64,
    pub dropped_frames: u64,
    pub total_frames: u64,
}

pub(crate) fn make_health_channel() -> (broadcast::Sender<HealthDelta>, Arc<RwLock<HealthSnapshot>>)
{
    let (tx, _) = broadcast::channel(16);
    (tx, Arc::new(RwLock::new(HealthSnapshot::default())))
}

impl BuiltinHealth for ObsClient {
    fn metrics(&self) -> [HealthMetric; 4] {
        let snap = self
            .health_state
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();

        [
            HealthMetric {
                label: "Stream".to_owned(),
                value: HealthValue::Status {
                    label: if snap.stream_active {
                        "Live".to_owned()
                    } else {
                        "Offline".to_owned()
                    },
                    active: snap.stream_active,
                    detail: None,
                },
            },
            HealthMetric {
                label: "Recording".to_owned(),
                value: HealthValue::Status {
                    label: if snap.record_active {
                        "Active".to_owned()
                    } else {
                        "Off".to_owned()
                    },
                    active: snap.record_active,
                    detail: None,
                },
            },
            HealthMetric {
                label: "CPU \u{00b7} FPS".to_owned(),
                value: HealthValue::Pair {
                    left: format!("{:.1}%", snap.cpu_percent),
                    right: format!("{:.1} fps", snap.fps),
                },
            },
            HealthMetric {
                label: "Dropped".to_owned(),
                value: HealthValue::Ratio {
                    used: snap.dropped_frames,
                    total: snap.total_frames,
                    reset_hint: None,
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
mod tests {
    use forge_platform_core::BuiltinHealth;
    use futures_util::StreamExt as _;

    use crate::client::ObsClient;

    #[test]
    fn metrics_returns_four_with_correct_labels() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        let health: &dyn BuiltinHealth = &client;
        let metrics = health.metrics();
        assert_eq!(metrics.len(), 4);
        assert_eq!(metrics[0].label, "Stream");
        assert_eq!(metrics[1].label, "Recording");
        assert_eq!(metrics[2].label, "CPU \u{00b7} FPS");
        assert_eq!(metrics[3].label, "Dropped");
    }

    #[tokio::test]
    async fn health_stream_is_subscribable() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        let health: &dyn BuiltinHealth = &client;
        let items: Vec<_> = health.stream().take(0).collect().await;
        assert!(items.is_empty());
    }
}
