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
            primary: HealthText::Count(count as u64),
            secondary: Some(messages::health_registered_hint().into()),
        }
    } else {
        HealthValue::Text {
            primary: HealthText::Count(0),
            secondary: Some(messages::health_disabled().into()),
        }
    }
}

fn last_triggered_value(snap: &HotkeyHealthSnapshot) -> HealthValue {
    let primary = match (snap.last_triggered_at, &snap.last_triggered_combo) {
        (Some(at), Some(combo)) => messages::health_last_triggered_at(at, combo).into(),
        _ => HealthText::placeholder(),
    };
    HealthValue::Text {
        primary,
        secondary: None,
    }
}

pub(crate) fn build_trigger_delta(snap: &HotkeyHealthSnapshot) -> HealthDelta {
    HealthDelta {
        metric: LAST_TRIGGERED_METRIC,
        new_value: last_triggered_value(snap),
    }
}

impl BuiltinHealth for HotkeyClient {
    fn metrics(&self) -> [HealthMetric; 4] {
        let snap = self.health_state.lock().unwrap_or_else(|p| p.into_inner());

        let registered_value = registered_count_health_value(
            self.enabled.load(Ordering::Relaxed),
            snap.registered_count,
        );

        let conflicts_value = HealthValue::Text {
            primary: HealthText::Count(snap.conflict_count as u64),
            secondary: Some(messages::health_conflicts_hint().into()),
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
                value: last_triggered_value(&snap),
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
            label: messages::health_portal_session_lost().into(),
            active: false,
            detail: Some(HealthText::Text(
                forge_types::VerbatimText::new(detail).into(),
            )),
        },
        (Some(true), None) => HealthValue::Status {
            label: messages::health_portal_active().into(),
            active: true,
            detail: None,
        },
        (Some(false), _) => HealthValue::Status {
            label: messages::health_portal_evdev_fallback().into(),
            active: false,
            detail: None,
        },
        (None, _) => HealthValue::Status {
            label: messages::health_portal_permission_denied().into(),
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
        primary: messages::health_backend_not_applicable().into(),
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

    fn metric_value(client: &HotkeyClient, id: &HealthMetricId) -> Option<HealthValue> {
        client
            .metrics()
            .into_iter()
            .find(|metric| &metric.id == id)
            .map(|metric| metric.value)
    }

    fn registered_metric(client: &HotkeyClient) -> Option<HealthValue> {
        metric_value(client, &REGISTERED_METRIC)
    }

    fn conflicts_metric(client: &HotkeyClient) -> Option<HealthValue> {
        metric_value(client, &CONFLICTS_METRIC)
    }

    fn conflicts(count: u64) -> Option<HealthValue> {
        Some(HealthValue::Text {
            primary: HealthText::Count(count),
            secondary: Some(messages::health_conflicts_hint().into()),
        })
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
        let counted = Some(HealthValue::Text {
            primary: HealthText::Count(1),
            secondary: Some(messages::health_registered_hint().into()),
        });
        assert_eq!(registered_metric(&client), counted);

        client.disable().await.unwrap();
        assert_eq!(
            registered_metric(&client),
            Some(HealthValue::Text {
                primary: HealthText::Count(0),
                secondary: Some(messages::health_disabled().into()),
            })
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
        assert_eq!(conflicts_metric(&client), conflicts(0));

        client.disable().await.unwrap();
        refuse.lock().unwrap().insert("Alt+X".to_owned());
        client.enable().await.unwrap();

        assert_eq!(conflicts_metric(&client), conflicts(1));
    }

    #[tokio::test]
    async fn re_registering_a_combo_the_client_already_holds_counts_toward_the_conflicts_metric() {
        let (backend, _tx) = MockPortalBackend::new();
        let os_registered = Arc::clone(&backend.registered);
        let client = start_supervised(backend, noop_publisher());
        let combo = HotkeyCombo::parse("Ctrl+F1").unwrap();
        client.register(combo.clone()).await.unwrap();

        client.register(combo).await.unwrap_err();

        assert_eq!(conflicts_metric(&client), conflicts(1));
        assert_eq!(os_registered.lock().unwrap().len(), 1);
    }

    #[test]
    fn the_last_triggered_tile_carries_the_instant_and_the_combo_verbatim() {
        let mut snap = HotkeyHealthSnapshot::default();
        snap.record_trigger("Ctrl+A".to_owned());
        let at = snap.last_triggered_at.unwrap();

        let delta = build_trigger_delta(&snap);

        assert_eq!(delta.metric, LAST_TRIGGERED_METRIC);
        assert_eq!(
            delta.new_value,
            HealthValue::Text {
                primary: messages::health_last_triggered_at(at, "Ctrl+A").into(),
                secondary: None,
            }
        );
    }

    #[test]
    fn the_last_triggered_tile_reads_a_placeholder_before_any_trigger() {
        assert_eq!(
            build_trigger_delta(&HotkeyHealthSnapshot::default()).new_value,
            HealthValue::Text {
                primary: HealthText::placeholder(),
                secondary: None,
            }
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_portal_tile_names_each_backend_condition_and_lights_only_when_active() {
        let lost = "bind failed";
        for (available, session_lost, label, active, detail) in [
            (
                Some(true),
                None,
                messages::health_portal_active(),
                true,
                None,
            ),
            (
                Some(true),
                Some(lost),
                messages::health_portal_session_lost(),
                false,
                Some(HealthText::Text(
                    forge_types::VerbatimText::new(lost).into(),
                )),
            ),
            (
                Some(false),
                None,
                messages::health_portal_evdev_fallback(),
                false,
                None,
            ),
            (
                Some(false),
                Some(lost),
                messages::health_portal_evdev_fallback(),
                false,
                None,
            ),
            (
                None,
                None,
                messages::health_portal_permission_denied(),
                false,
                None,
            ),
        ] {
            assert_eq!(
                backend_health_value(available, session_lost),
                HealthValue::Status {
                    label: label.into(),
                    active,
                    detail,
                },
                "{available:?} {session_lost:?}"
            );
        }
    }
}
