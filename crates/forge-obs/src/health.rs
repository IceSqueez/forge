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
pub(crate) const DROPPED_METRIC: HealthMetricId =
    HealthMetricId::new(messages::HEALTH_DROPPED_FRAMES);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameCounterSample {
    pub skipped: u32,
    pub total: u32,
}

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

pub(crate) fn stream_health_value(active: bool, duration: Option<Duration>) -> HealthValue {
    HealthValue::Status {
        label: HealthText::unlocalized(if active {
            "Live".to_owned()
        } else {
            "Off".to_owned()
        }),
        active,
        detail: Some(HealthText::unlocalized(
            duration
                .map(format_duration_hm)
                .unwrap_or_else(|| "-".to_owned()),
        )),
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
        label: HealthText::unlocalized(label),
        active,
        detail: Some(HealthText::unlocalized(
            duration
                .map(format_duration_hm)
                .unwrap_or_else(|| "-".to_owned()),
        )),
    }
}

pub(crate) fn cpu_fps_value(cpu_percent: f64, fps: f64, render_lag: bool) -> HealthValue {
    HealthValue::Text {
        primary: HealthText::unlocalized(format!("{cpu_percent:.1}% \u{00b7} {fps:.1}")),
        secondary: Some(HealthText::unlocalized(if render_lag {
            "lagging".to_owned()
        } else {
            "smooth".to_owned()
        })),
    }
}

pub(crate) fn dropped_value(dropped: u64, total: u64) -> HealthValue {
    let pct = if total > 0 {
        dropped as f64 / total as f64 * 100.0
    } else {
        0.0
    };
    HealthValue::Text {
        primary: HealthText::unlocalized(format!("{dropped} frames")),
        secondary: Some(HealthText::unlocalized(format!("{pct:.2}%"))),
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
                value: stream_health_value(snap.stream_active, snap.stream_duration),
            },
            HealthMetric {
                id: RECORDING_METRIC,
                value: record_health_value(
                    snap.record_active,
                    snap.record_paused,
                    snap.record_duration,
                ),
            },
            HealthMetric {
                id: CPU_FPS_METRIC,
                value: cpu_fps_value(snap.cpu_percent, snap.fps, snap.render_lag),
            },
            HealthMetric {
                id: DROPPED_METRIC,
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

    use super::*;
    use crate::client::ObsClient;

    fn text_parts(value: &HealthValue) -> (String, Option<String>) {
        match value {
            HealthValue::Text { primary, secondary } => (primary.clone(), secondary.clone()),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    fn status_parts(value: &HealthValue) -> (String, bool, Option<String>) {
        match value {
            HealthValue::Status {
                label,
                active,
                detail,
            } => (label.clone(), *active, detail.clone()),
            other => panic!("expected Status, got {other:?}"),
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
    fn stream_health_value_reports_live_or_off_with_a_duration_detail() {
        let live = stream_health_value(true, Some(Duration::from_secs(3_720)));
        assert_eq!(
            status_parts(&live),
            ("Live".to_owned(), true, Some("1h 2m".to_owned())),
        );

        let off = stream_health_value(false, None);
        assert_eq!(
            status_parts(&off),
            ("Off".to_owned(), false, Some("-".to_owned())),
        );
    }

    #[test]
    fn record_health_value_distinguishes_paused_from_active_and_off() {
        for (active, paused, expected_label) in [
            (true, false, "Active"),
            (true, true, "Paused"),
            (false, false, "Off"),
            (false, true, "Off"),
        ] {
            let (label, is_active, _) = status_parts(&record_health_value(active, paused, None));
            assert_eq!(label, expected_label, "active={active} paused={paused}");
            assert_eq!(is_active, active);
        }
    }

    #[test]
    fn cpu_fps_value_pairs_one_decimal_figures_with_a_render_lag_word() {
        let lagging = cpu_fps_value(12.34, 59.94, true);
        assert_eq!(
            text_parts(&lagging),
            ("12.3% \u{00b7} 59.9".to_owned(), Some("lagging".to_owned())),
        );

        let smooth = cpu_fps_value(0.0, 60.0, false);
        assert_eq!(
            text_parts(&smooth),
            ("0.0% \u{00b7} 60.0".to_owned(), Some("smooth".to_owned())),
        );
    }

    #[test]
    fn dropped_value_reports_the_frame_count_with_a_percentage_of_the_total() {
        let (primary, secondary) = text_parts(&dropped_value(3, 1_000));
        assert_eq!(primary, "3 frames");
        assert_eq!(secondary.as_deref(), Some("0.30%"));
    }

    #[test]
    fn dropped_value_reports_zero_percent_when_no_frames_were_rendered_yet() {
        let (primary, secondary) = text_parts(&dropped_value(0, 0));
        assert_eq!(primary, "0 frames");
        assert_eq!(secondary.as_deref(), Some("0.00%"));
    }

    #[test]
    fn metrics_declare_unique_ids() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());

        let ids: std::collections::HashSet<_> = client
            .metrics()
            .into_iter()
            .map(|metric| metric.id)
            .collect();

        assert_eq!(ids.len(), 4);
    }
}
