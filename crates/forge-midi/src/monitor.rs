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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn to_monitor_event_projects_number_and_value_per_midi_variant() {
        let cases = [
            (
                MidiEvent::NoteOn {
                    note: 60,
                    velocity: 100,
                    channel: 1,
                },
                "note_on",
                Some(60u8),
                Some(100u16),
            ),
            (
                MidiEvent::NoteOff {
                    note: 48,
                    velocity: 64,
                    channel: 2,
                },
                "note_off",
                Some(48),
                Some(64),
            ),
            (
                MidiEvent::ControlChange {
                    controller: 7,
                    value: 127,
                    channel: 3,
                },
                "control_change",
                Some(7),
                Some(127),
            ),
            (
                MidiEvent::PitchBend {
                    value: 16383,
                    channel: 4,
                },
                "pitch_bend",
                None,
                Some(16383),
            ),
            (
                MidiEvent::ProgramChange {
                    program: 42,
                    channel: 5,
                },
                "program_change",
                Some(42),
                None,
            ),
        ];

        for (event, kind, number, value) in cases {
            let monitor = to_monitor_event(&event, "Deck");
            assert_eq!(monitor.kind, kind, "kind for {event:?}");
            assert_eq!(monitor.number, number, "number for {event:?}");
            assert_eq!(monitor.value, value, "value for {event:?}");
            assert_eq!(monitor.port_name, "Deck", "port_name for {event:?}");
        }
    }

    #[test]
    fn to_monitor_event_carries_the_source_channel() {
        let monitor = to_monitor_event(
            &MidiEvent::ControlChange {
                controller: 7,
                value: 64,
                channel: 9,
            },
            "Deck",
        );
        assert_eq!(monitor.channel, 9);
    }
}
