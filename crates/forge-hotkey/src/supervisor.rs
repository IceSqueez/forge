use std::sync::Arc;
use std::sync::atomic::Ordering;

use forge_events::{Event, EventSource};
use forge_platform_core::HealthDelta;
use tokio::sync::{mpsc, oneshot};

use crate::backend::{HotkeyFiredEvent, HotkeyId};
use crate::client::{EnableFailure, HotkeyClient};
use crate::combo::HotkeyCombo;
use crate::health::{build_trigger_delta, registered_count_health_value};
use crate::payload_fields;

pub(crate) enum SupervisorCommand {
    Enable(oneshot::Sender<Vec<EnableFailure>>),
    Disable,
}

pub(crate) async fn run_supervisor(
    client: Arc<HotkeyClient>,
    mut fired_rx: mpsc::Receiver<HotkeyFiredEvent>,
    mut control_rx: mpsc::Receiver<SupervisorCommand>,
) {
    loop {
        tokio::select! {
            maybe_event = fired_rx.recv() => {
                let Some(event) = maybe_event else { break };
                handle_fired_event(&client, event);
            }
            maybe_cmd = control_rx.recv() => {
                let Some(cmd) = maybe_cmd else { break };
                handle_supervisor_command(&client, cmd);
            }
        }
    }
}

fn handle_fired_event(client: &Arc<HotkeyClient>, event: HotkeyFiredEvent) {
    if !client.enabled.load(Ordering::Relaxed) {
        return;
    }

    let combo_str = event.combo.as_str().to_owned();
    let id_u32 = event.id.0;

    let registered = client
        .registry
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .contains_key(&combo_str);

    if !registered {
        return;
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

fn handle_supervisor_command(client: &Arc<HotkeyClient>, cmd: SupervisorCommand) {
    match cmd {
        SupervisorCommand::Disable => {
            if !client.backend.delivery_gate_only() {
                for (id, _combo) in known_combos(client) {
                    let _ = client.backend.unregister(id);
                }
            }
            client.enabled.store(false, Ordering::Relaxed);
            emit_engine_state_change(client, false);
        }
        SupervisorCommand::Enable(reply) => {
            let mut failures = Vec::new();
            if !client.backend.delivery_gate_only() {
                for (id, combo) in known_combos(client) {
                    if let Err(error) = client.backend.register(id, &combo) {
                        failures.push(EnableFailure { id, combo, error });
                    }
                }
            }
            client.enabled.store(true, Ordering::Relaxed);
            emit_engine_state_change(client, true);
            let _ = reply.send(failures);
        }
    }
}

fn known_combos(client: &Arc<HotkeyClient>) -> Vec<(HotkeyId, HotkeyCombo)> {
    client
        .id_to_combo
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .map(|(&id, combo)| (id, combo.clone()))
        .collect()
}

fn emit_engine_state_change(client: &Arc<HotkeyClient>, enabled: bool) {
    let kind = if enabled {
        "hotkey.engine.enabled"
    } else {
        "hotkey.engine.disabled"
    };
    client
        .publisher
        .publish(Event::new(EventSource::Hotkey, kind, serde_json::json!({})));

    let delta = {
        let snap = client
            .health_state
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        HealthDelta {
            index: 0,
            new_value: registered_count_health_value(enabled, snap.registered_count),
        }
    };
    let _ = client.health_tx.send(delta);
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
        HealthDelta {
            index: 0,
            new_value: registered_count_health_value(
                client.enabled.load(Ordering::Relaxed),
                snap.registered_count,
            ),
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
        HealthDelta {
            index: 0,
            new_value: registered_count_health_value(
                client.enabled.load(Ordering::Relaxed),
                snap.registered_count,
            ),
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
