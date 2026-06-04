use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use time::OffsetDateTime;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use forge_platform_core::{BuiltinHealth, HealthDelta, HealthMetric, HealthStream, HealthValue};

use crate::client::MidiClient;

pub(crate) type HealthTx = broadcast::Sender<HealthDelta>;

#[derive(Default)]
pub(crate) struct MidiHealthSnapshot {
    pub input_count: usize,
    pub output_count: usize,
    pub last_note_on_at: Option<OffsetDateTime>,
    pub event_timestamps: VecDeque<Instant>,
}

pub(crate) fn make_health_state() -> (HealthTx, Arc<Mutex<MidiHealthSnapshot>>) {
    let (tx, _) = broadcast::channel(16);
    (tx, Arc::new(Mutex::new(MidiHealthSnapshot::default())))
}

pub(crate) fn events_per_minute(timestamps: &mut VecDeque<Instant>) -> usize {
    let cutoff = Instant::now().checked_sub(std::time::Duration::from_secs(60));
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

        let input_value = HealthValue::Text {
            primary: snap.input_count.to_string(),
            secondary: Some("connected".to_owned()),
        };
        let output_value = HealthValue::Text {
            primary: snap.output_count.to_string(),
            secondary: Some("available".to_owned()),
        };

        let last_note_value = match snap.last_note_on_at {
            None => HealthValue::Text {
                primary: "\u{2014}".to_owned(),
                secondary: None,
            },
            Some(t) => {
                let formatted = t
                    .format(
                        &time::format_description::parse("[hour]:[minute]:[second]")
                            .unwrap_or_default(),
                    )
                    .unwrap_or_else(|_| "--:--:--".to_owned());
                HealthValue::Text {
                    primary: formatted,
                    secondary: None,
                }
            }
        };

        let epm = events_per_minute(&mut snap.event_timestamps);
        let events_value = HealthValue::Text {
            primary: epm.to_string(),
            secondary: Some("last 60 s".to_owned()),
        };

        [
            HealthMetric {
                label: "INPUT PORTS".to_owned(),
                value: input_value,
            },
            HealthMetric {
                label: "OUTPUT PORTS".to_owned(),
                value: output_value,
            },
            HealthMetric {
                label: "LAST NOTE ON".to_owned(),
                value: last_note_value,
            },
            HealthMetric {
                label: "EVENTS / MIN".to_owned(),
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
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_platform_core::{BuiltinHealth, HealthValue};

    use super::*;
    use crate::client::MidiClient;

    #[test]
    fn metrics_returns_four_with_correct_labels() {
        let c = MidiClient::new_for_test();
        let h: &dyn BuiltinHealth = &*c;
        let m = h.metrics();
        assert_eq!(m.len(), 4);
        assert_eq!(m[0].label, "INPUT PORTS");
        assert_eq!(m[1].label, "OUTPUT PORTS");
        assert_eq!(m[2].label, "LAST NOTE ON");
        assert_eq!(m[3].label, "EVENTS / MIN");
    }

    #[test]
    fn initial_last_note_on_shows_dash() {
        let c = MidiClient::new_for_test();
        let h: &dyn BuiltinHealth = &*c;
        let m = h.metrics();
        assert!(matches!(
            m[2].value,
            HealthValue::Text { ref primary, .. } if primary == "\u{2014}"
        ));
    }

    #[test]
    fn initial_events_per_min_is_zero() {
        let c = MidiClient::new_for_test();
        let h: &dyn BuiltinHealth = &*c;
        let m = h.metrics();
        assert!(matches!(
            m[3].value,
            HealthValue::Text { ref primary, .. } if primary == "0"
        ));
    }

    #[tokio::test]
    async fn stream_is_subscribable() {
        let c = MidiClient::new_for_test();
        let h: &dyn BuiltinHealth = &*c;
        let items: Vec<_> = h.stream().take(0).collect().await;
        assert!(items.is_empty());
    }
}
