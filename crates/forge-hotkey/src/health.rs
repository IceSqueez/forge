use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use time::OffsetDateTime;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use forge_platform_core::{BuiltinHealth, HealthDelta, HealthMetric, HealthStream, HealthValue};

use crate::client::HotkeyClient;

pub(crate) type HealthTx = broadcast::Sender<HealthDelta>;

#[derive(Debug, Clone)]
pub(crate) struct TriggerRecord {
    pub(crate) combo: String,
    pub(crate) at: OffsetDateTime,
}

#[derive(Default)]
pub(crate) struct HotkeyHealthSnapshot {
    pub(crate) registered_count: usize,
    pub(crate) last_triggered_at: Option<OffsetDateTime>,
    pub(crate) last_triggered_combo: Option<String>,
    pub(crate) conflict_count: usize,
    pub(crate) recent_triggers: VecDeque<TriggerRecord>,
    pub(crate) last_synthesized_release_at: Option<OffsetDateTime>,
}

impl HotkeyHealthSnapshot {
    pub(crate) fn record_synthesized_release(&mut self) {
        self.last_synthesized_release_at = Some(OffsetDateTime::now_utc());
    }

    pub(crate) fn record_trigger(&mut self, combo: String) {
        let now = OffsetDateTime::now_utc();
        self.last_triggered_at = Some(now);
        self.last_triggered_combo = Some(combo.clone());
        if self.recent_triggers.len() >= 20 {
            self.recent_triggers.pop_front();
        }
        self.recent_triggers
            .push_back(TriggerRecord { combo, at: now });
    }
}

pub(crate) fn make_health_state() -> (HealthTx, Arc<Mutex<HotkeyHealthSnapshot>>) {
    let (tx, _) = broadcast::channel(16);
    (tx, Arc::new(Mutex::new(HotkeyHealthSnapshot::default())))
}

pub(crate) fn registered_count_health_value(enabled: bool, count: usize) -> HealthValue {
    if enabled {
        HealthValue::Text {
            primary: count.to_string(),
            secondary: Some("hotkeys".to_owned()),
        }
    } else {
        HealthValue::Text {
            primary: "0".to_owned(),
            secondary: Some("disabled".to_owned()),
        }
    }
}

pub(crate) fn build_trigger_delta(snap: &HotkeyHealthSnapshot) -> HealthDelta {
    let primary = match (&snap.last_triggered_at, &snap.last_triggered_combo) {
        (Some(t), Some(c)) => {
            let formatted = t
                .format(
                    &time::format_description::parse_borrowed::<2>("[hour]:[minute]:[second]")
                        .unwrap_or_default(),
                )
                .unwrap_or_else(|_| "--:--:--".to_owned());
            format!("{formatted} {c}")
        }
        _ => "\u{2014}".to_owned(),
    };
    HealthDelta {
        index: 1,
        new_value: HealthValue::Text {
            primary,
            secondary: None,
        },
    }
}

impl BuiltinHealth for HotkeyClient {
    fn metrics(&self) -> [HealthMetric; 4] {
        let snap = self.health_state.lock().unwrap_or_else(|p| p.into_inner());

        let registered_value = registered_count_health_value(
            self.enabled.load(Ordering::Relaxed),
            snap.registered_count,
        );

        let last_triggered_value = match (&snap.last_triggered_at, &snap.last_triggered_combo) {
            (Some(t), Some(c)) => {
                let formatted = t
                    .format(
                        &time::format_description::parse_borrowed::<2>("[hour]:[minute]:[second]")
                            .unwrap_or_default(),
                    )
                    .unwrap_or_else(|_| "--:--:--".to_owned());
                HealthValue::Text {
                    primary: format!("{formatted} {c}"),
                    secondary: None,
                }
            }
            _ => HealthValue::Text {
                primary: "\u{2014}".to_owned(),
                secondary: None,
            },
        };

        let conflicts_value = HealthValue::Text {
            primary: snap.conflict_count.to_string(),
            secondary: Some("since startup".to_owned()),
        };

        let portal_value = portal_health_value(self.portal_available);

        [
            HealthMetric {
                label: "REGISTERED".to_owned(),
                value: registered_value,
            },
            HealthMetric {
                label: "LAST TRIGGERED".to_owned(),
                value: last_triggered_value,
            },
            HealthMetric {
                label: "CONFLICTS".to_owned(),
                value: conflicts_value,
            },
            HealthMetric {
                label: backend_label(self.portal_available),
                value: portal_value,
            },
        ]
    }

    fn stream(&self) -> HealthStream {
        let rx = self.health_tx.subscribe();
        Box::pin(BroadcastStream::new(rx).filter_map(|r| r.ok()))
    }
}

#[cfg(target_os = "linux")]
fn portal_health_value(portal_available: Option<bool>) -> HealthValue {
    match portal_available {
        Some(true) => HealthValue::Status {
            label: "Portal active".to_owned(),
            active: true,
            detail: None,
        },
        Some(false) => HealthValue::Status {
            label: "Portal unavailable - evdev fallback".to_owned(),
            active: false,
            detail: None,
        },
        None => HealthValue::Status {
            label: "Permission denied - add user to 'input' group".to_owned(),
            active: false,
            detail: None,
        },
    }
}

#[cfg(target_os = "linux")]
fn backend_label(_portal_available: Option<bool>) -> String {
    "PORTAL".to_owned()
}

#[cfg(not(target_os = "linux"))]
fn portal_health_value(_portal_available: Option<bool>) -> HealthValue {
    HealthValue::Text {
        primary: "N/A".to_owned(),
        secondary: None,
    }
}

#[cfg(not(target_os = "linux"))]
fn backend_label(_portal_available: Option<bool>) -> String {
    "BACKEND".to_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_platform_core::BuiltinHealth;

    use super::*;
    use crate::backend::tests::MockPortalBackend;
    use crate::client::tests::{noop_publisher, start_supervised};
    use crate::combo::HotkeyCombo;

    fn text_metric(client: &HotkeyClient, index: usize) -> Option<(String, Option<String>)> {
        match client.metrics().into_iter().nth(index)?.value {
            HealthValue::Text { primary, secondary } => Some((primary, secondary)),
            _ => None,
        }
    }

    fn registered_metric(client: &HotkeyClient) -> Option<(String, Option<String>)> {
        text_metric(client, 0)
    }

    fn conflicts_metric(client: &HotkeyClient) -> Option<(String, Option<String>)> {
        text_metric(client, 2)
    }

    #[tokio::test]
    async fn the_registered_metric_masks_the_count_while_the_engine_is_disabled() {
        let (backend, _tx) = MockPortalBackend::new();
        let client = start_supervised(backend, noop_publisher());
        client
            .register(HotkeyCombo::parse("Ctrl+F1").unwrap())
            .await
            .unwrap();
        let counted = Some(("1".to_owned(), Some("hotkeys".to_owned())));
        assert_eq!(registered_metric(&client), counted);

        client.disable().await.unwrap();
        assert_eq!(
            registered_metric(&client),
            Some(("0".to_owned(), Some("disabled".to_owned())))
        );

        client.enable().await.unwrap();
        assert_eq!(registered_metric(&client), counted);
    }

    #[tokio::test]
    async fn a_combo_refused_at_enable_counts_toward_the_conflicts_metric() {
        let (backend, _tx) = MockPortalBackend::new();
        let refuse = Arc::clone(&backend.fail_on);
        let client = start_supervised(backend, noop_publisher());
        client
            .register(HotkeyCombo::parse("Alt+X").unwrap())
            .await
            .unwrap();
        assert_eq!(
            conflicts_metric(&client),
            Some(("0".to_owned(), Some("since startup".to_owned())))
        );

        client.disable().await.unwrap();
        refuse.lock().unwrap().insert("Alt+X".to_owned());
        client.enable().await.unwrap();

        assert_eq!(
            conflicts_metric(&client),
            Some(("1".to_owned(), Some("since startup".to_owned())))
        );
    }

    #[tokio::test]
    async fn re_registering_a_combo_the_client_already_holds_counts_toward_the_conflicts_metric() {
        let (backend, _tx) = MockPortalBackend::new();
        let os_registered = Arc::clone(&backend.registered);
        let client = start_supervised(backend, noop_publisher());
        let combo = HotkeyCombo::parse("Ctrl+F1").unwrap();
        client.register(combo.clone()).await.unwrap();

        client.register(combo).await.unwrap_err();

        assert_eq!(
            conflicts_metric(&client),
            Some(("1".to_owned(), Some("since startup".to_owned())))
        );
        assert_eq!(os_registered.lock().unwrap().len(), 1);
    }

    #[test]
    fn build_trigger_delta_with_values() {
        let mut snap = HotkeyHealthSnapshot::default();
        snap.record_trigger("Ctrl+A".to_owned());
        let delta = build_trigger_delta(&snap);
        assert_eq!(delta.index, 1);
        assert!(matches!(
            delta.new_value,
            HealthValue::Text { ref primary, .. } if primary.contains("Ctrl+A")
        ));
    }
}
