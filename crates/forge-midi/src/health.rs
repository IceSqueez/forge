use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const EVENT_RATE_WINDOW: Duration = Duration::from_secs(60);

use time::OffsetDateTime;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use forge_platform_core::{
    BuiltinHealth, HealthDelta, HealthMetric, HealthMetricId, HealthStream, HealthText, HealthValue,
};

use crate::client::MidiClient;
use crate::vocabulary::messages;

pub(crate) const INPUT_PORTS_METRIC: HealthMetricId =
    HealthMetricId::new(messages::HEALTH_INPUT_PORTS);
pub(crate) const OUTPUT_PORTS_METRIC: HealthMetricId =
    HealthMetricId::new(messages::HEALTH_OUTPUT_PORTS);
pub(crate) const LAST_NOTE_ON_METRIC: HealthMetricId =
    HealthMetricId::new(messages::HEALTH_LAST_NOTE_ON);
pub(crate) const EVENTS_PER_MINUTE_METRIC: HealthMetricId =
    HealthMetricId::new(messages::HEALTH_EVENTS_PER_MINUTE);

pub(crate) type HealthTx = broadcast::Sender<HealthDelta>;

#[derive(Default)]
pub(crate) struct MidiHealthSnapshot {
    pub input_count: usize,
    pub output_count: usize,
    pub last_note_on_at: Option<OffsetDateTime>,
    pub event_timestamps: VecDeque<Instant>,
}

pub(crate) fn make_health_state() -> (HealthTx, Arc<Mutex<MidiHealthSnapshot>>) {
    let (tx, _) = broadcast::channel(forge_platform_core::HEALTH_CHANNEL_CAPACITY);
    (tx, Arc::new(Mutex::new(MidiHealthSnapshot::default())))
}

pub(crate) fn input_port_health_value(enabled: bool, input_count: usize) -> HealthValue {
    if enabled {
        HealthValue::Text {
            primary: HealthText::Count(input_count as u64),
            secondary: Some(messages::health_input_connected().into()),
        }
    } else {
        HealthValue::Text {
            primary: HealthText::Count(0),
            secondary: Some(messages::health_disabled().into()),
        }
    }
}

pub(crate) fn output_port_health_value(output_count: usize) -> HealthValue {
    HealthValue::Text {
        primary: HealthText::Count(output_count as u64),
        secondary: Some(messages::health_output_available().into()),
    }
}

pub(crate) fn last_note_health_value(at: Option<OffsetDateTime>) -> HealthValue {
    HealthValue::Text {
        primary: at.map_or_else(HealthText::placeholder, |at| {
            messages::health_note_time(at).into()
        }),
        secondary: None,
    }
}

pub(crate) fn events_per_minute_health_value(events: usize) -> HealthValue {
    HealthValue::Text {
        primary: HealthText::Count(events as u64),
        secondary: Some(messages::health_last_minute().into()),
    }
}

pub(crate) fn events_per_minute(timestamps: &mut VecDeque<Instant>) -> usize {
    let cutoff = Instant::now().checked_sub(EVENT_RATE_WINDOW);
    if let Some(cutoff) = cutoff {
        while timestamps.front().is_some_and(|t| *t < cutoff) {
            timestamps.pop_front();
        }
    }
    timestamps.len()
}

impl BuiltinHealth for MidiClient {
    fn metrics(&self) -> [HealthMetric; 4] {
        let mut snap = self.health_state.lock().unwrap_or_else(|p| p.into_inner());

        let input_value =
            input_port_health_value(self.enabled.load(Ordering::Relaxed), snap.input_count);
        let output_value = output_port_health_value(snap.output_count);
        let last_note_value = last_note_health_value(snap.last_note_on_at);
        let events_value =
            events_per_minute_health_value(events_per_minute(&mut snap.event_timestamps));

        [
            HealthMetric {
                id: INPUT_PORTS_METRIC,
                value: input_value,
            },
            HealthMetric {
                id: OUTPUT_PORTS_METRIC,
                value: output_value,
            },
            HealthMetric {
                id: LAST_NOTE_ON_METRIC,
                value: last_note_value,
            },
            HealthMetric {
                id: EVENTS_PER_MINUTE_METRIC,
                value: events_value,
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
    use crate::client::MidiClient;

    #[test]
    fn metrics_declare_unique_catalogued_ids() {
        let c = MidiClient::new_for_test();

        let ids: std::collections::HashSet<_> =
            c.metrics().into_iter().map(|metric| metric.id).collect();

        assert_eq!(ids.len(), 4);
        for id in &ids {
            assert!(
                crate::vocabulary::messages::MIDI_MESSAGES
                    .codes
                    .contains(id.label().code()),
                "{id} is not catalogued"
            );
        }
    }

    #[test]
    fn input_port_health_value_masks_the_port_count_while_disabled() {
        for (enabled, count, shown, hint) in [
            (true, 3usize, 3, messages::health_input_connected()),
            (false, 3, 0, messages::health_disabled()),
        ] {
            assert_eq!(
                input_port_health_value(enabled, count),
                HealthValue::Text {
                    primary: HealthText::Count(shown),
                    secondary: Some(hint.into()),
                },
                "enabled={enabled} count={count}"
            );
        }
    }

    #[test]
    fn metrics_report_the_input_port_slot_as_disabled_when_input_is_off() {
        let c = MidiClient::new_for_test();
        c.health_state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .input_count = 2;
        c.enabled.store(false, Ordering::Relaxed);

        let h: &dyn BuiltinHealth = &*c;
        let metrics = h.metrics();
        let input_metric = metrics
            .iter()
            .find(|metric| metric.id == INPUT_PORTS_METRIC)
            .unwrap();
        assert!(
            matches!(
                &input_metric.value,
                HealthValue::Text { secondary, .. }
                    if *secondary == Some(messages::health_disabled().into())
            ),
            "input metric was {:?}",
            input_metric.value
        );
    }

    #[test]
    fn the_last_note_tile_reads_a_placeholder_until_a_note_arrives() {
        let at = OffsetDateTime::UNIX_EPOCH;

        for (seen, primary) in [
            (None, HealthText::placeholder()),
            (Some(at), messages::health_note_time(at).into()),
        ] {
            assert_eq!(
                last_note_health_value(seen),
                HealthValue::Text {
                    primary,
                    secondary: None,
                },
                "{seen:?}"
            );
        }
    }

    #[test]
    fn the_port_and_event_tiles_carry_their_counts_with_a_hint() {
        assert_eq!(
            output_port_health_value(2),
            HealthValue::Text {
                primary: HealthText::Count(2),
                secondary: Some(messages::health_output_available().into()),
            }
        );
        assert_eq!(
            events_per_minute_health_value(120),
            HealthValue::Text {
                primary: HealthText::Count(120),
                secondary: Some(messages::health_last_minute().into()),
            }
        );
    }
}
