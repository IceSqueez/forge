use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use time::OffsetDateTime;
use tokio::sync::mpsc;

use forge_events::{Event, EventSource};
use forge_platform_core::HealthDelta;

use crate::backend::InputHandle;
use crate::client::MidiClient;
use crate::content::{record_midi_event, record_port_added, record_port_removed};
use crate::decode::decode_midi_bytes;
use crate::events::{MidiEvent, MidiPortInfo, PortDirection};
use crate::health::{MidiHealthSnapshot, events_per_minute};

pub(crate) type RawEvent = (u64, Vec<u8>, String);

pub(crate) async fn run_supervisor(
    client: Arc<MidiClient>,
    merged_tx: mpsc::Sender<RawEvent>,
    mut merged_rx: mpsc::Receiver<RawEvent>,
) {
    let mut input_snap: Vec<MidiPortInfo> = Vec::new();
    let mut output_snap: Vec<MidiPortInfo> = Vec::new();
    let mut handles: HashMap<String, Box<dyn InputHandle>> = HashMap::new();

    do_port_discovery(
        &client,
        &merged_tx,
        &mut input_snap,
        &mut output_snap,
        &mut handles,
    );

    let start = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut poll = tokio::time::interval_at(start, Duration::from_secs(2));

    loop {
        tokio::select! {
            maybe_raw = merged_rx.recv() => {
                let Some((ts, data, port_name)) = maybe_raw else { break };
                handle_raw_event(&client, ts, &data, &port_name);
            }
            _ = poll.tick() => {
                do_port_discovery(
                    &client,
                    &merged_tx,
                    &mut input_snap,
                    &mut output_snap,
                    &mut handles,
                );
            }
        }
    }
}

fn do_port_discovery(
    client: &Arc<MidiClient>,
    merged_tx: &mpsc::Sender<RawEvent>,
    input_snap: &mut Vec<MidiPortInfo>,
    output_snap: &mut Vec<MidiPortInfo>,
    handles: &mut HashMap<String, Box<dyn InputHandle>>,
) {
    let new_inputs = client.backend.list_input_ports();
    let new_outputs = client.backend.list_output_ports();

    diff_input_ports(client, merged_tx, input_snap, handles, &new_inputs);
    diff_output_ports(client, output_snap, &new_outputs);

    *input_snap = new_inputs.clone();
    *output_snap = new_outputs.clone();

    let input_count = input_snap.len();
    let output_count = output_snap.len();

    {
        let mut snap = client
            .content_state
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        snap.input_ports = new_inputs;
        snap.output_ports = new_outputs;
    }

    let deltas = {
        let mut snap = client
            .health_state
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        build_port_count_deltas(&mut snap, input_count, output_count)
    };
    for d in deltas {
        let _ = client.health_tx.send(d);
    }

    if let Ok(mut guard) = client.input_ports.write() {
        *guard = input_snap.clone();
    }
    if let Ok(mut guard) = client.output_ports.write() {
        *guard = output_snap.clone();
    }
}

fn diff_input_ports(
    client: &Arc<MidiClient>,
    merged_tx: &mpsc::Sender<RawEvent>,
    old: &[MidiPortInfo],
    handles: &mut HashMap<String, Box<dyn InputHandle>>,
    new: &[MidiPortInfo],
) {
    let old_names: std::collections::HashSet<&str> = old.iter().map(|p| p.name.as_str()).collect();
    let new_names: std::collections::HashSet<&str> = new.iter().map(|p| p.name.as_str()).collect();

    for port in new {
        if !old_names.contains(port.name.as_str()) {
            open_input_port(client, merged_tx, handles, port);
            emit_port_event(client, &port.name, PortDirection::Input, true);
            let mut content = client
                .content_state
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            record_port_added(&mut content, port.clone());
        }
    }

    for port in old {
        if !new_names.contains(port.name.as_str()) {
            handles.remove(&port.name);
            emit_port_event(client, &port.name, PortDirection::Input, false);
            let mut content = client
                .content_state
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            record_port_removed(&mut content, &port.name, PortDirection::Input);
        }
    }
}

fn diff_output_ports(client: &Arc<MidiClient>, old: &[MidiPortInfo], new: &[MidiPortInfo]) {
    let old_names: std::collections::HashSet<&str> = old.iter().map(|p| p.name.as_str()).collect();
    let new_names: std::collections::HashSet<&str> = new.iter().map(|p| p.name.as_str()).collect();

    for port in new {
        if !old_names.contains(port.name.as_str()) {
            emit_port_event(client, &port.name, PortDirection::Output, true);
            let mut content = client
                .content_state
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            record_port_added(&mut content, port.clone());
        }
    }

    for port in old {
        if !new_names.contains(port.name.as_str()) {
            emit_port_event(client, &port.name, PortDirection::Output, false);
            let mut content = client
                .content_state
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            record_port_removed(&mut content, &port.name, PortDirection::Output);
        }
    }
}

fn open_input_port(
    client: &Arc<MidiClient>,
    merged_tx: &mpsc::Sender<RawEvent>,
    handles: &mut HashMap<String, Box<dyn InputHandle>>,
    port: &MidiPortInfo,
) {
    let (port_tx, mut port_rx) = mpsc::channel::<(u64, Vec<u8>)>(64);
    match client.backend.open_input(&port.name, port_tx) {
        Ok(handle) => {
            handles.insert(port.name.clone(), handle);
            let merged = merged_tx.clone();
            let pname = port.name.clone();
            tokio::spawn(async move {
                while let Some((ts, data)) = port_rx.recv().await {
                    let _ = merged.send((ts, data, pname.clone())).await;
                }
            });
        }
        Err(e) => {
            tracing::warn!(port = %port.name, error = %e, "failed to open MIDI input");
        }
    }
}

fn emit_port_event(client: &Arc<MidiClient>, name: &str, direction: PortDirection, added: bool) {
    let kind = if added {
        "midi.port.added"
    } else {
        "midi.port.removed"
    };
    let dir_str = match direction {
        PortDirection::Input => "input",
        PortDirection::Output => "output",
    };
    let event = Event::new(
        EventSource::Midi,
        kind,
        serde_json::json!({ "name": name, "direction": dir_str }),
    );
    client.publisher.publish(event);
}

fn handle_raw_event(client: &Arc<MidiClient>, _ts: u64, data: &[u8], port_name: &str) {
    match decode_midi_bytes(data) {
        Ok(Some(event)) => emit_midi_event(client, port_name, event),
        Ok(None) => {}
        Err(e) => {
            tracing::debug!(port = %port_name, error = %e, "ignoring malformed MIDI bytes");
        }
    }
}

fn emit_midi_event(client: &Arc<MidiClient>, port_name: &str, event: MidiEvent) {
    let (kind, payload) = match &event {
        MidiEvent::NoteOn {
            note,
            velocity,
            channel,
        } => (
            "midi.input.note_on",
            serde_json::json!({
                "note": note,
                "velocity": velocity,
                "channel": channel,
                "port": port_name,
            }),
        ),
        MidiEvent::NoteOff {
            note,
            velocity,
            channel,
        } => (
            "midi.input.note_off",
            serde_json::json!({
                "note": note,
                "velocity": velocity,
                "channel": channel,
                "port": port_name,
            }),
        ),
        MidiEvent::ControlChange {
            controller,
            value,
            channel,
        } => (
            "midi.input.control_change",
            serde_json::json!({
                "controller": controller,
                "value": value,
                "channel": channel,
                "port": port_name,
            }),
        ),
        MidiEvent::PitchBend { value, channel } => (
            "midi.input.pitch_bend",
            serde_json::json!({
                "value": value,
                "channel": channel,
                "port": port_name,
            }),
        ),
        MidiEvent::ProgramChange { program, channel } => (
            "midi.input.program_change",
            serde_json::json!({
                "program": program,
                "channel": channel,
                "port": port_name,
            }),
        ),
    };

    client
        .publisher
        .publish(Event::new(EventSource::Midi, kind, payload));

    let now = Instant::now();
    let is_note_on = matches!(event, MidiEvent::NoteOn { .. });

    let deltas: Vec<HealthDelta> = {
        let mut snap = client
            .health_state
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        snap.event_timestamps.push_back(now);
        if is_note_on {
            snap.last_note_on_at = Some(OffsetDateTime::now_utc());
        }
        build_event_deltas(&mut snap, is_note_on)
    };
    for d in deltas {
        let _ = client.health_tx.send(d);
    }

    {
        let mut snap = client
            .content_state
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        record_midi_event(&mut snap, port_name);
    }
}

fn build_port_count_deltas(
    snap: &mut MidiHealthSnapshot,
    input_count: usize,
    output_count: usize,
) -> Vec<HealthDelta> {
    let mut deltas = Vec::new();
    if snap.input_count != input_count {
        snap.input_count = input_count;
        deltas.push(HealthDelta {
            index: 0,
            new_value: forge_platform_core::HealthValue::Text {
                primary: input_count.to_string(),
                secondary: Some("connected".to_owned()),
            },
        });
    }
    if snap.output_count != output_count {
        snap.output_count = output_count;
        deltas.push(HealthDelta {
            index: 1,
            new_value: forge_platform_core::HealthValue::Text {
                primary: output_count.to_string(),
                secondary: Some("available".to_owned()),
            },
        });
    }
    deltas
}

fn build_event_deltas(snap: &mut MidiHealthSnapshot, is_note_on: bool) -> Vec<HealthDelta> {
    let mut deltas = Vec::new();

    if is_note_on && let Some(t) = snap.last_note_on_at {
        let formatted = t
            .format(
                &time::format_description::parse_borrowed::<2>("[hour]:[minute]:[second]")
                    .unwrap_or_default(),
            )
            .unwrap_or_else(|_| "--:--:--".to_owned());
        deltas.push(HealthDelta {
            index: 2,
            new_value: forge_platform_core::HealthValue::Text {
                primary: formatted,
                secondary: None,
            },
        });
    }

    let epm = events_per_minute(&mut snap.event_timestamps);
    deltas.push(HealthDelta {
        index: 3,
        new_value: forge_platform_core::HealthValue::Text {
            primary: epm.to_string(),
            secondary: Some("last 60 s".to_owned()),
        },
    });

    deltas
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use forge_events::{Event, EventPublisher};

    use crate::backend::MidiBackend;
    use crate::backend::tests::MockMidiBackend;
    use crate::client::MidiClient;
    use crate::config::MidiConfig;
    use crate::events::{MidiPortInfo, PortDirection};

    struct RecordingPublisher {
        events: std::sync::Mutex<Vec<Event>>,
    }

    impl RecordingPublisher {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                events: std::sync::Mutex::new(Vec::new()),
            })
        }

        fn has_kind(&self, kind: &str) -> bool {
            self.events.lock().unwrap().iter().any(|e| e.kind == kind)
        }

        fn find_all_kind(&self, kind: &str) -> Vec<Event> {
            self.events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| e.kind == kind)
                .cloned()
                .collect()
        }

        fn find_kind(&self, kind: &str) -> Option<Event> {
            self.events
                .lock()
                .unwrap()
                .iter()
                .find(|e| e.kind == kind)
                .cloned()
        }
    }

    impl EventPublisher for RecordingPublisher {
        fn publish(&self, event: Event) {
            self.events.lock().unwrap().push(event);
        }
    }

    fn input_port(name: &str) -> MidiPortInfo {
        MidiPortInfo {
            name: name.to_owned(),
            direction: PortDirection::Input,
        }
    }

    fn output_port(name: &str) -> MidiPortInfo {
        MidiPortInfo {
            name: name.to_owned(),
            direction: PortDirection::Output,
        }
    }

    async fn start_client(
        backend: Arc<MockMidiBackend>,
        publisher: Arc<RecordingPublisher>,
    ) -> Arc<MidiClient> {
        let client = MidiClient::start(
            MidiConfig::default(),
            Arc::clone(&publisher) as Arc<dyn EventPublisher>,
            Arc::clone(&backend) as Arc<dyn MidiBackend>,
        );
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }
        client
    }

    #[tokio::test(start_paused = true)]
    async fn hot_plug_add_emits_port_added_event() {
        let backend = Arc::new(MockMidiBackend::new(vec![input_port("A")], vec![]));
        let publisher = RecordingPublisher::new();
        let _client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        backend.set_input_ports(vec![input_port("A"), input_port("B")]);
        tokio::time::advance(Duration::from_secs(2)).await;
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }

        let added = publisher.find_all_kind("midi.port.added");
        assert!(
            added.iter().any(|e| e.payload["name"] == "B"),
            "expected midi.port.added for port B"
        );
        let b_ev = added.iter().find(|e| e.payload["name"] == "B").unwrap();
        assert_eq!(b_ev.payload["direction"], "input");
    }

    #[tokio::test(start_paused = true)]
    async fn hot_plug_remove_emits_port_removed_event() {
        let backend = Arc::new(MockMidiBackend::new(
            vec![input_port("A"), input_port("B")],
            vec![],
        ));
        let publisher = RecordingPublisher::new();
        let _client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        backend.set_input_ports(vec![input_port("A")]);
        tokio::time::advance(Duration::from_secs(2)).await;
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }

        let removed = publisher.find_all_kind("midi.port.removed");
        assert!(
            removed.iter().any(|e| e.payload["name"] == "B"),
            "expected midi.port.removed for port B"
        );
    }

    #[tokio::test]
    async fn subscribe_inject_note_on_emits_bus_event() {
        let backend = Arc::new(MockMidiBackend::new(vec![input_port("Piano")], vec![]));
        let publisher = RecordingPublisher::new();
        let _client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        backend.inject_all(0, vec![0x90, 60, 127]).await;
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }

        assert!(publisher.has_kind("midi.input.note_on"));
        let ev = publisher.find_kind("midi.input.note_on").unwrap();
        assert_eq!(ev.payload["note"], 60);
        assert_eq!(ev.payload["velocity"], 127);
        assert_eq!(ev.payload["channel"], 0);
        assert_eq!(ev.payload["port"], "Piano");
    }

    #[tokio::test]
    async fn subscribe_inject_note_off_emits_bus_event() {
        let backend = Arc::new(MockMidiBackend::new(vec![input_port("Piano")], vec![]));
        let publisher = RecordingPublisher::new();
        let _client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        backend.inject_all(0, vec![0x80, 48, 64]).await;
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }

        assert!(publisher.has_kind("midi.input.note_off"));
        let ev = publisher.find_kind("midi.input.note_off").unwrap();
        assert_eq!(ev.payload["note"], 48);
        assert_eq!(ev.payload["port"], "Piano");
    }

    #[tokio::test]
    async fn subscribe_inject_cc_emits_bus_event() {
        let backend = Arc::new(MockMidiBackend::new(vec![input_port("Pad")], vec![]));
        let publisher = RecordingPublisher::new();
        let _client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        backend.inject_all(0, vec![0xB1, 7, 100]).await;
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }

        assert!(publisher.has_kind("midi.input.control_change"));
        let ev = publisher.find_kind("midi.input.control_change").unwrap();
        assert_eq!(ev.payload["controller"], 7);
        assert_eq!(ev.payload["value"], 100);
        assert_eq!(ev.payload["channel"], 1);
        assert_eq!(ev.payload["port"], "Pad");
    }

    #[tokio::test]
    async fn unsupported_status_byte_emits_no_event() {
        let backend = Arc::new(MockMidiBackend::new(vec![input_port("Piano")], vec![]));
        let publisher = RecordingPublisher::new();
        let _client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        backend.inject_all(0, vec![0xC0, 10]).await;
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }

        assert!(!publisher.has_kind("midi.input.note_on"));
        assert!(!publisher.has_kind("midi.input.note_off"));
        assert!(!publisher.has_kind("midi.input.control_change"));
    }

    #[tokio::test]
    async fn note_on_velocity_zero_emits_note_off_event() {
        let backend = Arc::new(MockMidiBackend::new(vec![input_port("Piano")], vec![]));
        let publisher = RecordingPublisher::new();
        let _client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        backend.inject_all(0, vec![0x90, 60, 0]).await;
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }

        assert!(
            !publisher.has_kind("midi.input.note_on"),
            "velocity-0 must become note_off"
        );
        assert!(publisher.has_kind("midi.input.note_off"));
        let ev = publisher.find_kind("midi.input.note_off").unwrap();
        assert_eq!(ev.payload["note"], 60);
    }

    #[tokio::test(start_paused = true)]
    async fn output_port_added_emits_event_with_output_direction() {
        let backend = Arc::new(MockMidiBackend::new(vec![], vec![]));
        let publisher = RecordingPublisher::new();
        let _client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        backend.set_output_ports(vec![output_port("Synth")]);
        tokio::time::advance(Duration::from_secs(2)).await;
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }

        let added = publisher.find_all_kind("midi.port.added");
        assert!(
            added.iter().any(|e| e.payload["direction"] == "output"),
            "expected output port added event"
        );
    }
}
