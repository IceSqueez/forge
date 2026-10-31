use super::{Capture, MidiScreenView};
use crate::async_bridge::{BridgeFlow, drain_events};
use crate::integrations::BuiltinObject;
use crate::midi_signal::{MidiSignal, note_name};
use crate::runtime_event;
use forge_midi::{MidiClient, MidiMonitorEvent, MidiSignalKind};
use forge_runtime::{EventBus, IntegrationSlot, LifecycleState};
use futures_util::StreamExt;
use gpui::{Context, Task};
use std::sync::Arc;
use tokio::sync::mpsc::error::TrySendError;

const MONITOR_BRIDGE_CAPACITY: usize = 256;
const MONITOR_ROWS: usize = 4;

pub(super) fn monitor_value_label(event: &MidiMonitorEvent) -> String {
    match event.kind {
        MidiSignalKind::NoteOn => match (event.number, event.value) {
            (Some(note), Some(velocity)) => {
                format!("{} vel {velocity}", note_name(i64::from(note)))
            }
            (Some(note), None) => note_name(i64::from(note)),
            _ => String::new(),
        },
        MidiSignalKind::NoteOff => event
            .number
            .map(|note| note_name(i64::from(note)))
            .unwrap_or_default(),
        MidiSignalKind::ControlChange => match (event.number, event.value) {
            (Some(controller), Some(value)) => format!("{controller} \u{2192} {value}"),
            _ => String::new(),
        },
        MidiSignalKind::PitchBend => event.value.map(|v| v.to_string()).unwrap_or_default(),
        MidiSignalKind::ProgramChange => event.number.map(|n| n.to_string()).unwrap_or_default(),
    }
}

impl MidiScreenView {
    pub(super) fn spawn_monitor_bridge(
        client: &Arc<MidiClient>,
        rt_handle: &tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let (tx, mut rx) = tokio::sync::mpsc::channel(MONITOR_BRIDGE_CAPACITY);
        let mut stream = client.monitor_stream();
        rt_handle.spawn(async move {
            loop {
                let event = tokio::select! {
                    () = tx.closed() => break,
                    event = stream.next() => event,
                };
                let Some(event) = event else {
                    break;
                };
                if matches!(tx.try_send(event), Err(TrySendError::Closed(_))) {
                    break;
                }
            }
        });
        cx.spawn(async move |this, cx| {
            while let Some(event) = rx.recv().await {
                if this
                    .update(cx, |this, cx| this.push_monitor(event, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
    }

    pub(super) fn spawn_lifecycle_bridge(
        integration: &IntegrationSlot<BuiltinObject>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let id = integration.id().clone();
        let mut lifecycle = integration.watch();
        let mut last = lifecycle.current().remove(&id);
        cx.spawn(async move |this, cx| {
            while let Some(mut states) = lifecycle.changed().await {
                let state = states.remove(&id);
                if state == last {
                    continue;
                }
                last = state;
                if this.update(cx, |this, cx| this.follow_engine(cx)).is_err() {
                    break;
                }
            }
        })
    }

    fn follow_engine(&mut self, cx: &mut Context<Self>) {
        match self.integration.as_ref().map(IntegrationSlot::state) {
            Some(LifecycleState::Running) => self.enabled = true,
            Some(LifecycleState::Disabled | LifecycleState::Failed(_)) => self.enabled = false,
            Some(LifecycleState::Starting | LifecycleState::Stopping) | None => {}
        }
        let live = self.sink.live();
        let unchanged = match (&self.client, &live) {
            (Some(current), Some(live)) => Arc::ptr_eq(current, live),
            (None, None) => true,
            _ => false,
        };
        if !unchanged {
            self._monitor_bridge = live
                .as_ref()
                .map(|client| Self::spawn_monitor_bridge(client, &self.rt_handle, cx));
            self.client = live;
            self.refresh_ports(cx);
        }
        cx.notify();
    }

    pub(super) fn spawn_port_bridge(bus: Arc<EventBus>, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            drain_events(&bus, cx, move |batch, cx| {
                let touched = batch.iter().any(runtime_event::is_midi_port_event);
                if !touched {
                    return BridgeFlow::Continue;
                }
                match this.update(cx, |this, cx| this.refresh_ports(cx)) {
                    Ok(()) => BridgeFlow::Continue,
                    Err(_) => BridgeFlow::Stop,
                }
            })
            .await;
        })
    }

    fn push_monitor(&mut self, event: MidiMonitorEvent, cx: &mut Context<Self>) {
        if self.capture != Capture::Off {
            self.on_capture(MidiSignal::from_monitor(&event), cx);
        }
        self.monitor.insert(0, event);
        self.monitor.truncate(MONITOR_ROWS);
        cx.notify();
    }

    fn on_capture(&mut self, signal: MidiSignal, cx: &mut Context<Self>) {
        let capture = std::mem::replace(&mut self.capture, Capture::Off);
        self.escape_sub = None;
        match capture {
            Capture::Off => {}
            Capture::Learn => self.open_modal(None, signal, None, cx),
            Capture::Relearn => {
                if let Some(open) = &self.modal {
                    open.view
                        .update(cx, |modal, cx| modal.apply_capture(signal, cx));
                }
            }
        }
    }
}
