use std::pin::Pin;

use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::events::{MidiEvent, MidiMonitorEvent};

pub type MidiMonitorStream =
    Pin<Box<dyn tokio_stream::Stream<Item = MidiMonitorEvent> + Send + 'static>>;

pub(crate) type MonitorTx = broadcast::Sender<MidiMonitorEvent>;

pub(crate) fn make_monitor_state() -> MonitorTx {
    let (tx, _) = broadcast::channel(256);
    tx
}

pub(crate) fn subscribe(tx: &MonitorTx) -> MidiMonitorStream {
    let rx = tx.subscribe();
    Box::pin(BroadcastStream::new(rx).filter_map(|r| r.ok()))
}

pub(crate) fn to_monitor_event(event: &MidiEvent, port_name: &str) -> MidiMonitorEvent {
    match *event {
        MidiEvent::NoteOn {
            note,
            velocity,
            channel,
        } => MidiMonitorEvent {
            kind: "note_on".to_owned(),
            port_name: port_name.to_owned(),
            channel,
            number: Some(note),
            value: Some(u16::from(velocity)),
        },
        MidiEvent::NoteOff {
            note,
            velocity,
            channel,
        } => MidiMonitorEvent {
            kind: "note_off".to_owned(),
            port_name: port_name.to_owned(),
            channel,
            number: Some(note),
            value: Some(u16::from(velocity)),
        },
        MidiEvent::ControlChange {
            controller,
            value,
            channel,
        } => MidiMonitorEvent {
            kind: "control_change".to_owned(),
            port_name: port_name.to_owned(),
            channel,
            number: Some(controller),
            value: Some(u16::from(value)),
        },
        MidiEvent::PitchBend { value, channel } => MidiMonitorEvent {
            kind: "pitch_bend".to_owned(),
            port_name: port_name.to_owned(),
            channel,
            number: None,
            value: Some(value),
        },
        MidiEvent::ProgramChange { program, channel } => MidiMonitorEvent {
            kind: "program_change".to_owned(),
            port_name: port_name.to_owned(),
            channel,
            number: Some(program),
            value: None,
        },
    }
}
