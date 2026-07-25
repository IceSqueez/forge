use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use forge_platform_core::{BuiltinHealth, HealthDelta, HealthMetric, HealthStream, HealthValue};

use crate::client::ObsClient;

#[derive(Debug, Clone, Default)]
pub struct HealthSnapshot {
    pub stream_active: bool,
    pub stream_duration: Option<Duration>,
    pub record_active: bool,
    pub record_paused: bool,
    pub record_duration: Option<Duration>,
    pub cpu_percent: f64,
    pub fps: f64,
    pub render_lag: bool,
    pub dropped_frames: u64,
    pub total_frames: u64,
}

pub(crate) fn make_health_channel() -> (broadcast::Sender<HealthDelta>, Arc<RwLock<HealthSnapshot>>)
{
    let (tx, _) = broadcast::channel(16);
    (tx, Arc::new(RwLock::new(HealthSnapshot::default())))
}

/// Formats a duration as `"<h>h <m>m"`, dropping the hour segment under one hour.
pub(crate) fn format_duration_hm(d: Duration) -> String {
    let total_minutes = d.as_secs() / 60;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

pub(crate) fn stream_health_value(active: bool, duration: Option<Duration>) -> HealthValue {
    HealthValue::Status {
        label: if active {
            "Live".to_owned()
        } else {
            "Off".to_owned()
        },
        active,
        detail: Some(
            duration
                .map(format_duration_hm)
                .unwrap_or_else(|| "-".to_owned()),
        ),
    }
}

pub(crate) fn record_health_value(
    active: bool,
    paused: bool,
    duration: Option<Duration>,
) -> HealthValue {
    let label = if !active {
        "Off".to_owned()
    } else if paused {
        "Paused".to_owned()
    } else {
        "Active".to_owned()
    };
    HealthValue::Status {
        label,
        active,
        detail: Some(
            duration
                .map(format_duration_hm)
                .unwrap_or_else(|| "-".to_owned()),
        ),
    }
}

pub(crate) fn cpu_fps_value(cpu_percent: f64, fps: f64, render_lag: bool) -> HealthValue {
    HealthValue::Text {
        primary: format!("{cpu_percent:.1}% \u{00b7} {fps:.1}"),
        secondary: Some(if render_lag {
            "lagging".to_owned()
        } else {
            "smooth".to_owned()
        }),
    }
}

pub(crate) fn dropped_value(dropped: u64, total: u64) -> HealthValue {
    let pct = if total > 0 {
        dropped as f64 / total as f64 * 100.0
    } else {
        0.0
    };
    HealthValue::Text {
        primary: format!("{dropped} frames"),
        secondary: Some(format!("{pct:.2}%")),
    }
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
                value: stream_health_value(snap.stream_active, snap.stream_duration),
            },
            HealthMetric {
                label: "Recording".to_owned(),
                value: record_health_value(
                    snap.record_active,
                    snap.record_paused,
                    snap.record_duration,
                ),
            },
            HealthMetric {
                label: "CPU \u{00b7} FPS".to_owned(),
                value: cpu_fps_value(snap.cpu_percent, snap.fps, snap.render_lag),
            },
            HealthMetric {
                label: "Dropped".to_owned(),
                value: dropped_value(snap.dropped_frames, snap.total_frames),
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

    #[tokio::test]
    async fn health_stream_is_subscribable() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        let health: &dyn BuiltinHealth = &client;
        let items: Vec<_> = health.stream().take(0).collect().await;
        assert!(items.is_empty());
    }
}
