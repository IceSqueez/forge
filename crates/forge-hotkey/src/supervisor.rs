use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_platform_core::HealthDelta;
use tokio::sync::{mpsc, oneshot};
use tokio::time::MissedTickBehavior;

use crate::backend::{HotkeyEdge, HotkeyFiredEvent, HotkeyId};
use crate::client::{EnableFailure, HotkeyClient};
use crate::combo::HotkeyCombo;
use crate::health::{build_trigger_delta, registered_count_health_value};
use crate::hold;
use crate::payload_fields;

const CEILING_SWEEP_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) enum SupervisorCommand {
    Enable(oneshot::Sender<Vec<EnableFailure>>),
    Disable(oneshot::Sender<()>),
}

pub(crate) async fn run_supervisor(
    client: Arc<HotkeyClient>,
    mut fired_rx: mpsc::Receiver<HotkeyFiredEvent>,
    restart_rx: Option<mpsc::Receiver<()>>,
    mut control_rx: mpsc::Receiver<SupervisorCommand>,
) {
    let mut restart_rx = restart_rx;
    let mut ceiling_sweep = tokio::time::interval(CEILING_SWEEP_INTERVAL);
    ceiling_sweep.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            biased;
            maybe_event = fired_rx.recv() => {
                let Some(event) = maybe_event else { break };
                handle_fired_event(&client, event);
            }
            maybe_cmd = control_rx.recv() => {
                let Some(cmd) = maybe_cmd else { break };
                handle_supervisor_command(&client, cmd);
            }
            restart = maybe_restart(restart_rx.as_mut()) => {
                match restart {
                    Some(()) => hold::close_all_synthesized(&client),
                    None => restart_rx = None,
                }
            }
            _ = ceiling_sweep.tick() => {
                hold::close_expired(&client);
            }
        }
    }

    hold::close_all_synthesized(&client);
}

async fn maybe_restart(restart_rx: Option<&mut mpsc::Receiver<()>>) -> Option<()> {
    match restart_rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

fn handle_fired_event(client: &Arc<HotkeyClient>, event: HotkeyFiredEvent) {
    match event.edge {
        HotkeyEdge::Press => handle_press(client, event),
        HotkeyEdge::Release => hold::close_observed(client, event.id, event.timestamp_us),
    }
}

fn handle_press(client: &Arc<HotkeyClient>, event: HotkeyFiredEvent) {
    if !client.enabled.load(Ordering::Relaxed) {
        return;
    }

    let combo_str = event.combo.as_str().to_owned();

    let registered = client
        .registry
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .contains_key(&combo_str);

    if !registered {
        return;
    }

    if !hold::open(client, event.id, combo_str.clone(), event.timestamp_us) {
        return;
    }

    let delta = {
        let mut snap = client
            .health_state
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        snap.record_trigger(combo_str);
        build_trigger_delta(&snap)
    };
    let _ = client.health_tx.send(delta);
}

fn handle_supervisor_command(client: &Arc<HotkeyClient>, cmd: SupervisorCommand) {
    match cmd {
        SupervisorCommand::Disable(reply) => {
            if !client.enabled.load(Ordering::Relaxed) {
                let _ = reply.send(());
                return;
            }
            hold::close_all_synthesized(client);
            if !client.backend.delivery_gate_only() {
                for (id, _combo) in known_combos(client) {
                    let _ = client.backend.unregister(id);
                }
            }
            client.enabled.store(false, Ordering::Relaxed);
            emit_engine_state_change(client, false);
            let _ = reply.send(());
        }
        SupervisorCommand::Enable(reply) => {
            if client.enabled.load(Ordering::Relaxed) {
                let _ = reply.send(Vec::new());
                return;
            }
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
            if !failures.is_empty() {
                record_enable_failures(client, &failures);
                emit_enable_failed(client, &failures);
            }
            let _ = reply.send(failures);
        }
    }
}

fn record_enable_failures(client: &Arc<HotkeyClient>, failures: &[EnableFailure]) {
    let conflicts = failures
        .iter()
        .filter(|f| matches!(f.error, crate::error::HotkeyError::AlreadyRegistered { .. }))
        .count();
    if conflicts == 0 {
        return;
    }
    let mut snap = client
        .health_state
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    snap.conflict_count = snap.conflict_count.saturating_add(conflicts);
}

fn emit_enable_failed(client: &Arc<HotkeyClient>, failures: &[EnableFailure]) {
    let combos: Vec<String> = failures
        .iter()
        .map(|f| f.combo.as_str().to_owned())
        .collect();
    client.publisher.publish(Event::new(
        EventSource::Hotkey,
        "hotkey.engine.enable_failed",
        serde_json::json!({ (payload_fields::COMBOS): combos }),
    ));
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
