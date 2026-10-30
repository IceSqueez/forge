use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use time::OffsetDateTime;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use forge_platform_core::{
    BuiltinHealth, HealthDelta, HealthMetric, HealthMetricId, HealthStream, HealthText, HealthValue,
};

use crate::client::HotkeyClient;
use crate::vocabulary::messages;

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
    pub(crate) backend_session_lost: Option<String>,
}

pub(crate) const REGISTERED_METRIC: HealthMetricId =
    HealthMetricId::new(messages::HEALTH_REGISTERED);
pub(crate) const LAST_TRIGGERED_METRIC: HealthMetricId =
    HealthMetricId::new(messages::HEALTH_LAST_TRIGGERED);
pub(crate) const CONFLICTS_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_CONFLICTS);
#[cfg(target_os = "linux")]
pub(crate) const BACKEND_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_PORTAL);
#[cfg(not(target_os = "linux"))]
pub(crate) const BACKEND_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_BACKEND);

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
    let (tx, _) = broadcast::channel(forge_platform_core::HEALTH_CHANNEL_CAPACITY);
    (tx, Arc::new(Mutex::new(HotkeyHealthSnapshot::default())))
}

pub(crate) fn registered_count_health_value(enabled: bool, count: usize) -> HealthValue {
    if enabled {
        HealthValue::Text {
            primary: HealthText::unlocalized(count.to_string()),
            secondary: Some(HealthText::unlocalized("hotkeys")),
        }
    } else {
        HealthValue::Text {
            primary: HealthText::unlocalized("0"),
            secondary: Some(HealthText::unlocalized("disabled")),
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
        metric: LAST_TRIGGERED_METRIC,
        new_value: HealthValue::Text {
            primary: HealthText::unlocalized(primary),
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
                    primary: HealthText::unlocalized(format!("{formatted} {c}")),
                    secondary: None,
                }
            }
            _ => HealthValue::Text {
                primary: HealthText::unlocalized("\u{2014}"),
                secondary: None,
            },
        };

        let conflicts_value = HealthValue::Text {
            primary: HealthText::unlocalized(snap.conflict_count.to_string()),
            secondary: Some(HealthText::unlocalized("since startup")),
        };

        let portal_value =
            backend_health_value(self.portal_available, snap.backend_session_lost.as_deref());

        [
            HealthMetric {
                id: REGISTERED_METRIC,
                value: registered_value,
            },
            HealthMetric {
                id: LAST_TRIGGERED_METRIC,
                value: last_triggered_value,
            },
            HealthMetric {
                id: CONFLICTS_METRIC,
                value: conflicts_value,
            },
            HealthMetric {
                id: BACKEND_METRIC,
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
pub(crate) fn backend_health_value(
    portal_available: Option<bool>,
    session_lost: Option<&str>,
) -> HealthValue {
    match (portal_available, session_lost) {
        (Some(true), Some(detail)) => HealthValue::Status {
            label: HealthText::unlocalized("Portal session lost - hotkeys will not fire"),
            active: false,
            detail: Some(HealthText::unlocalized(detail)),
        },
        (Some(true), None) => HealthValue::Status {
            label: HealthText::unlocalized("Portal active"),
            active: true,
            detail: None,
        },
        (Some(false), _) => HealthValue::Status {
            label: HealthText::unlocalized("Portal unavailable - evdev fallback"),
            active: false,
            detail: None,
        },
        (None, _) => HealthValue::Status {
            label: HealthText::unlocalized("Permission denied - add user to 'input' group"),
            active: false,
            detail: None,
        },
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn backend_health_value(
    _portal_available: Option<bool>,
    _session_lost: Option<&str>,
) -> HealthValue {
    HealthValue::Text {
        primary: HealthText::unlocalized("N/A"),
        secondary: None,
    }
}

#[cfg(test)]
mod tests {
    use forge_platform_core::BuiltinHealth;

    use super::*;
    use crate::backend::tests::MockPortalBackend;
    use crate::client::tests::{noop_publisher, start_supervised};
    use crate::combo::HotkeyCombo;

    fn plain(text: &HealthText) -> String {
        match text {
            HealthText::Text(text) => text.code_or_text(),
            other => panic!("expected text, got {other:?}"),
        }
    }

    fn text_metric(client: &HotkeyClient, id: &HealthMetricId) -> Option<(String, Option<String>)> {
        match client
            .metrics()
            .into_iter()
            .find(|metric| &metric.id == id)?
            .value
        {
            HealthValue::Text { primary, secondary } => {
                Some((plain(&primary), secondary.as_ref().map(plain)))
            }
            _ => None,
        }
    }

    fn registered_metric(client: &HotkeyClient) -> Option<(String, Option<String>)> {
        text_metric(client, &REGISTERED_METRIC)
    }

    fn conflicts_metric(client: &HotkeyClient) -> Option<(String, Option<String>)> {
        text_metric(client, &CONFLICTS_METRIC)
    }

    #[tokio::test]
    async fn metrics_declare_unique_catalogued_ids() {
        let (backend, _tx) = MockPortalBackend::new();
        let client = start_supervised(backend, noop_publisher());

        let ids: std::collections::HashSet<_> = client
            .metrics()
            .into_iter()
            .map(|metric| metric.id)
            .collect();

        assert_eq!(ids.len(), 4);
        for id in &ids {
            assert!(
                crate::vocabulary::messages::HOTKEY_MESSAGES
                    .codes
                    .contains(id.label().code()),
                "{id} is not catalogued"
            );
        }
    }

    #[tokio::test]
    async fn every_streamed_health_delta_targets_a_declared_metric() {
        use tokio_stream::StreamExt;

        let (backend, _tx) = MockPortalBackend::new();
        let client = start_supervised(backend, noop_publisher());
        let declared: Vec<_> = client
            .metrics()
            .into_iter()
            .map(|metric| metric.id)
            .collect();
        let mut stream = client.stream();
        let combo = HotkeyCombo::parse("Ctrl+F2").unwrap();
        let id = client.register(combo).await.unwrap();
        client.unregister(id).await.unwrap();
        client.disable().await.unwrap();

        let mut seen = Vec::new();
        for _ in 0..3 {
            seen.push(stream.next().await.unwrap().metric);
        }
        seen.push(build_trigger_delta(&HotkeyHealthSnapshot::default()).metric);

        for id in &seen {
            assert!(declared.contains(id), "undeclared {id}");
        }
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
        assert_eq!(delta.metric, LAST_TRIGGERED_METRIC);
        assert!(matches!(
            delta.new_value,
            HealthValue::Text { ref primary, .. } if plain(primary).contains("Ctrl+A")
        ));
    }
}
