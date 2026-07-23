use std::sync::Arc;

use forge_events::{Event, EventSource};
use forge_platform_core::HealthDelta;
use tokio::sync::mpsc;

use crate::backend::HotkeyFiredEvent;
use crate::client::HotkeyClient;
use crate::health::build_trigger_delta;
use crate::payload_fields;

pub(crate) async fn run_supervisor(
    client: Arc<HotkeyClient>,
    mut fired_rx: mpsc::Receiver<HotkeyFiredEvent>,
) {
    while let Some(event) = fired_rx.recv().await {
        let combo_str = event.combo.as_str().to_owned();
        let id_u32 = event.id.0;

        let registered = client
            .registry
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .contains_key(&combo_str);

        if !registered {
            continue;
        }

        client.publisher.publish(Event::new(
            EventSource::Hotkey,
            "hotkey.global.pressed",
            serde_json::json!({
                (payload_fields::COMBO): combo_str,
                (payload_fields::ID): id_u32,
                (payload_fields::TIMESTAMP_US): event.timestamp_us,
            }),
        ));

        let delta = {
            let mut snap = client
                .health_state
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            snap.record_trigger(combo_str.clone());
            build_trigger_delta(&snap)
        };
        let _ = client.health_tx.send(delta);
    }
}

pub(crate) fn emit_registered(client: &HotkeyClient, combo_str: &str, id_u32: u32) {
    client.publisher.publish(Event::new(
        EventSource::Hotkey,
        "hotkey.registered",
        serde_json::json!({ (payload_fields::COMBO): combo_str, (payload_fields::ID): id_u32 }),
    ));

    let delta: HealthDelta = {
        let mut snap = client
            .health_state
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        snap.registered_count = snap.registered_count.saturating_add(1);
        forge_platform_core::HealthDelta {
            index: 0,
            new_value: forge_platform_core::HealthValue::Text {
                primary: snap.registered_count.to_string(),
                secondary: Some("hotkeys".to_owned()),
            },
        }
    };
    let _ = client.health_tx.send(delta);
}

pub(crate) fn emit_unregistered(client: &HotkeyClient, combo_str: &str, id_u32: u32) {
    client.publisher.publish(Event::new(
        EventSource::Hotkey,
        "hotkey.unregistered",
        serde_json::json!({ (payload_fields::COMBO): combo_str, (payload_fields::ID): id_u32 }),
    ));

    let delta = {
        let mut snap = client
            .health_state
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        snap.registered_count = snap.registered_count.saturating_sub(1);
        forge_platform_core::HealthDelta {
            index: 0,
            new_value: forge_platform_core::HealthValue::Text {
                primary: snap.registered_count.to_string(),
                secondary: Some("hotkeys".to_owned()),
            },
        }
    };
    let _ = client.health_tx.send(delta);
}

#[cfg(target_os = "linux")]
pub(crate) fn emit_portal_unavailable(client: &HotkeyClient, detail: &str) {
    client.publisher.publish(Event::new(
        EventSource::Hotkey,
        "hotkey.portal.unavailable",
        serde_json::json!({
            (payload_fields::portal::REASON): payload_fields::portal::reason::NO_BACKEND_AVAILABLE,
            (payload_fields::portal::DETAIL): detail,
        }),
    ));
}
