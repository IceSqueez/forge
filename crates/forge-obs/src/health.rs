use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use forge_platform_core::{
    BuiltinHealth, HealthDelta, HealthMetric, HealthMetricId, HealthStream, HealthText, HealthValue,
};

use crate::client::ObsClient;
use crate::vocabulary::messages;

pub(crate) const STREAM_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_STREAM);
pub(crate) const RECORDING_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_RECORDING);
pub(crate) const CPU_FPS_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_CPU_FPS);
const PERCENT_SCALE: f64 = 100.0;

pub(crate) const DROPPED_METRIC: HealthMetricId =
    HealthMetricId::new(messages::HEALTH_DROPPED_FRAMES);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FrameCounterSample {
    pub skipped: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct HealthSnapshot {
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
    pub render_frame_sample: Option<FrameCounterSample>,
    pub output_frame_sample: Option<FrameCounterSample>,
}

pub(crate) fn make_health_channel() -> (broadcast::Sender<HealthDelta>, Arc<RwLock<HealthSnapshot>>)
{
    let (tx, _) = broadcast::channel(forge_platform_core::HEALTH_CHANNEL_CAPACITY);
    (tx, Arc::new(RwLock::new(HealthSnapshot::default())))
}

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

pub(crate) fn delta(metric: HealthMetricId, snapshot: &HealthSnapshot) -> HealthDelta {
    let new_value = if metric == STREAM_METRIC {
        stream_value(snapshot)
    } else if metric == RECORDING_METRIC {
        record_value(snapshot)
    } else if metric == CPU_FPS_METRIC {
        cpu_fps_value(snapshot)
    } else {
        dropped_value(snapshot)
    };
    HealthDelta { metric, new_value }
}

fn duration_detail(duration: Option<Duration>) -> HealthText {
    duration.map_or_else(HealthText::placeholder, HealthText::Span)
}

fn stream_value(snapshot: &HealthSnapshot) -> HealthValue {
    let label = if snapshot.stream_active {
        messages::health_live()
    } else {
        messages::health_off()
    };
    HealthValue::Status {
        label: label.into(),
        active: snapshot.stream_active,
        detail: Some(duration_detail(snapshot.stream_duration)),
    }
}

fn record_value(snapshot: &HealthSnapshot) -> HealthValue {
    let label = if !snapshot.record_active {
        messages::health_off()
    } else if snapshot.record_paused {
        messages::health_paused()
    } else {
        messages::health_active()
    };
    HealthValue::Status {
        label: label.into(),
        active: snapshot.record_active,
        detail: Some(duration_detail(snapshot.record_duration)),
    }
}

fn cpu_fps_value(snapshot: &HealthSnapshot) -> HealthValue {
    let lag = if snapshot.render_lag {
        messages::health_lagging()
    } else {
        messages::health_smooth()
    };
    HealthValue::Text {
        primary: messages::health_cpu_fps_value(snapshot.cpu_percent, snapshot.fps).into(),
        secondary: Some(lag.into()),
    }
}

fn dropped_value(snapshot: &HealthSnapshot) -> HealthValue {
    let percent = if snapshot.total_frames > 0 {
        snapshot.dropped_frames as f64 / snapshot.total_frames as f64 * PERCENT_SCALE
    } else {
        0.0
    };
    HealthValue::Text {
        primary: messages::health_dropped_count(snapshot.dropped_frames).into(),
        secondary: Some(messages::health_dropped_share(percent).into()),
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
                id: STREAM_METRIC,
                value: stream_value(&snap),
            },
            HealthMetric {
                id: RECORDING_METRIC,
                value: record_value(&snap),
            },
            HealthMetric {
                id: CPU_FPS_METRIC,
                value: cpu_fps_value(&snap),
            },
            HealthMetric {
                id: DROPPED_METRIC,
                value: dropped_value(&snap),
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
    use forge_types::MessageRef;

    use super::*;
    use crate::client::ObsClient;

    fn status(label: MessageRef, active: bool, detail: HealthText) -> HealthValue {
        HealthValue::Status {
            label: label.into(),
            active,
            detail: Some(detail),
        }
    }

    fn text(primary: MessageRef, secondary: MessageRef) -> HealthValue {
        HealthValue::Text {
            primary: primary.into(),
            secondary: Some(secondary.into()),
        }
    }

    #[test]
    fn format_duration_hm_drops_the_hour_segment_below_one_hour() {
        for (secs, expected) in [
            (0, "0m"),
            (59, "0m"),
            (60, "1m"),
            (3_599, "59m"),
            (3_600, "1h 0m"),
            (3_661, "1h 1m"),
            (90_000, "25h 0m"),
        ] {
            assert_eq!(
                format_duration_hm(Duration::from_secs(secs)),
                expected,
                "{secs}s",
            );
        }
    }

    #[test]
    fn the_stream_tile_reads_live_with_its_span_or_off_with_a_placeholder() {
        let span = Duration::from_secs(3_720);
        for (snapshot, expected) in [
            (
                HealthSnapshot {
                    stream_active: true,
                    stream_duration: Some(span),
                    ..HealthSnapshot::default()
                },
                status(messages::health_live(), true, HealthText::Span(span)),
            ),
            (
                HealthSnapshot::default(),
                status(messages::health_off(), false, HealthText::placeholder()),
            ),
        ] {
            let delta = delta(STREAM_METRIC, &snapshot);

            assert_eq!(delta.metric, STREAM_METRIC);
            assert_eq!(delta.new_value, expected);
        }
    }

    #[test]
    fn the_recording_tile_tells_paused_from_active_and_stopped_wins_over_paused() {
        let span = Duration::from_secs(45);
        for (active, paused, label) in [
            (true, false, messages::health_active()),
            (true, true, messages::health_paused()),
            (false, false, messages::health_off()),
            (false, true, messages::health_off()),
        ] {
            let snapshot = HealthSnapshot {
                record_active: active,
                record_paused: paused,
                record_duration: Some(span),
                ..HealthSnapshot::default()
            };

            assert_eq!(
                delta(RECORDING_METRIC, &snapshot).new_value,
                status(label, active, HealthText::Span(span)),
                "active={active} paused={paused}"
            );
        }
    }

    #[test]
    fn the_cpu_tile_carries_the_figures_as_numbers_with_a_render_lag_word() {
        for (render_lag, word) in [
            (true, messages::health_lagging()),
            (false, messages::health_smooth()),
        ] {
            let snapshot = HealthSnapshot {
                cpu_percent: 12.34,
                fps: 59.94,
                render_lag,
                ..HealthSnapshot::default()
            };

            assert_eq!(
                delta(CPU_FPS_METRIC, &snapshot).new_value,
                text(messages::health_cpu_fps_value(12.34, 59.94), word),
            );
        }
    }

    #[test]
    fn the_dropped_tile_shares_the_count_against_the_total_and_reads_zero_before_any_frame() {
        for (dropped_frames, total_frames, percent) in [(3, 1_000, 0.3), (0, 0, 0.0), (7, 7, 100.0)]
        {
            let snapshot = HealthSnapshot {
                dropped_frames,
                total_frames,
                ..HealthSnapshot::default()
            };

            assert_eq!(
                delta(DROPPED_METRIC, &snapshot).new_value,
                text(
                    messages::health_dropped_count(dropped_frames),
                    messages::health_dropped_share(percent),
                ),
                "{dropped_frames}/{total_frames}"
            );
        }
    }

    #[test]
    fn metrics_declare_unique_catalogued_ids() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());

        let ids: std::collections::HashSet<_> = client
            .metrics()
            .into_iter()
            .map(|metric| metric.id)
            .collect();

        assert_eq!(ids.len(), 4);
        for id in &ids {
            assert!(
                crate::vocabulary::messages::OBS_MESSAGES
                    .codes
                    .contains(id.label().code()),
                "{id} is not catalogued"
            );
        }
    }
}
