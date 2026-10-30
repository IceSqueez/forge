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
            primary: HealthText::unlocalized(input_count.to_string()),
            secondary: Some(HealthText::unlocalized("connected")),
        }
    } else {
        HealthValue::Text {
            primary: HealthText::unlocalized("0"),
            secondary: Some(HealthText::unlocalized("disabled")),
        }
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
        let output_value = HealthValue::Text {
            primary: HealthText::unlocalized(snap.output_count.to_string()),
            secondary: Some(HealthText::unlocalized("available")),
        };

        let last_note_value = match snap.last_note_on_at {
            None => HealthValue::Text {
                primary: HealthText::unlocalized("\u{2014}"),
                secondary: None,
            },
            Some(t) => {
                let formatted = t
                    .format(
                        &time::format_description::parse_borrowed::<2>("[hour]:[minute]:[second]")
                            .unwrap_or_default(),
                    )
                    .unwrap_or_else(|_| "--:--:--".to_owned());
                HealthValue::Text {
                    primary: HealthText::unlocalized(formatted),
                    secondary: None,
                }
            }
        };

        let epm = events_per_minute(&mut snap.event_timestamps);
        let events_value = HealthValue::Text {
            primary: HealthText::unlocalized(epm.to_string()),
            secondary: Some(HealthText::unlocalized("last 60 s")),
        };

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
        for (enabled, count, expected_primary, expected_secondary) in [
            (true, 3usize, "3", "connected"),
            (false, 3, "0", "disabled"),
        ] {
            let value = input_port_health_value(enabled, count);
            assert!(
                matches!(
                    &value,
                    HealthValue::Text { primary, secondary }
                        if *primary == HealthText::unlocalized(expected_primary)
                            && *secondary == Some(HealthText::unlocalized(expected_secondary))
                ),
                "enabled={enabled} count={count} produced {value:?}"
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
                    if *secondary == Some(HealthText::unlocalized("disabled"))
            ),
            "input metric was {:?}",
            input_metric.value
        );
    }
}
