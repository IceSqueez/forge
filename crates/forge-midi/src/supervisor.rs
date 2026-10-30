use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

const DEVICE_POLL_INTERVAL: Duration = Duration::from_secs(2);

use time::OffsetDateTime;
use tokio::sync::mpsc;

use forge_events::{Event, EventSource};
use forge_platform_core::HealthDelta;

use crate::backend::InputHandle;
use crate::client::MidiClient;
use crate::content::{record_midi_event, record_port_added, record_port_removed};
use crate::decode::decode_midi_bytes;
use crate::events::{MidiEvent, MidiPortInfo, PortDirection};
use crate::health::{
    EVENTS_PER_MINUTE_METRIC, INPUT_PORTS_METRIC, LAST_NOTE_ON_METRIC, MidiHealthSnapshot,
    OUTPUT_PORTS_METRIC, events_per_minute, events_per_minute_health_value,
    input_port_health_value, last_note_health_value, output_port_health_value,
};
use crate::monitor::{bus_payload, to_monitor_event};
use crate::vocabulary::event_kinds;
use crate::vocabulary::fields::port as port_fields;

pub(crate) type RawEvent = (u64, Vec<u8>, String);

pub(crate) enum SupervisorCommand {
    Enable,
    Disable,
    Rescan,
    Shutdown,
}

pub(crate) async fn run_supervisor(
    client: Weak<MidiClient>,
    merged_tx: mpsc::Sender<RawEvent>,
    mut merged_rx: mpsc::Receiver<RawEvent>,
    mut control_rx: mpsc::Receiver<SupervisorCommand>,
) {
    let mut input_snap: Vec<MidiPortInfo> = Vec::new();
    let mut output_snap: Vec<MidiPortInfo> = Vec::new();
    let mut handles: HashMap<String, Box<dyn InputHandle>> = HashMap::new();

    let Some(first) = client.upgrade() else {
        return;
    };
    do_port_discovery(
        &first,
        &merged_tx,
        &mut input_snap,
        &mut output_snap,
        &mut handles,
    )
    .await;
    drop(first);

    let start = tokio::time::Instant::now() + DEVICE_POLL_INTERVAL;
    let mut poll = tokio::time::interval_at(start, DEVICE_POLL_INTERVAL);

    loop {
        tokio::select! {
            maybe_raw = merged_rx.recv() => {
                let Some((ts, data, port_name)) = maybe_raw else { break };
                let Some(client) = client.upgrade() else { break };
                handle_raw_event(&client, ts, &data, &port_name);
            }
            maybe_cmd = control_rx.recv() => {
                let Some(cmd) = maybe_cmd else { break };
                let Some(client) = client.upgrade() else { break };
                if handle_supervisor_command(
                    &client,
                    cmd,
                    &merged_tx,
                    &mut input_snap,
                    &mut output_snap,
                    &mut handles,
                )
                .await
                .is_break()
                {
                    break;
                }
            }
            _ = poll.tick() => {
                let Some(client) = client.upgrade() else { break };
                do_port_discovery(
                    &client,
                    &merged_tx,
                    &mut input_snap,
                    &mut output_snap,
                    &mut handles,
                )
                .await;
                decay_events_per_minute(&client);
            }
        }
    }
}

async fn handle_supervisor_command(
    client: &Arc<MidiClient>,
    cmd: SupervisorCommand,
    merged_tx: &mpsc::Sender<RawEvent>,
    input_snap: &mut Vec<MidiPortInfo>,
    output_snap: &mut Vec<MidiPortInfo>,
    handles: &mut HashMap<String, Box<dyn InputHandle>>,
) -> ControlFlow<()> {
    match cmd {
        SupervisorCommand::Rescan => {
            do_port_discovery(client, merged_tx, input_snap, output_snap, handles).await;
        }
        SupervisorCommand::Shutdown => {
            handles.clear();
            return ControlFlow::Break(());
        }
        SupervisorCommand::Disable => {
            handles.clear();
            sync_subscribed_inputs(client, handles);
            client.enabled.store(false, Ordering::Relaxed);
            emit_engine_state_change(client, false);
        }
        SupervisorCommand::Enable => {
            client.enabled.store(true, Ordering::Relaxed);
            for port in input_snap.iter() {
                if !handles.contains_key(&port.name) {
                    open_input_port(client, merged_tx, handles, port);
                }
            }
            sync_subscribed_inputs(client, handles);
            emit_engine_state_change(client, true);
        }
    }
    ControlFlow::Continue(())
}

fn emit_engine_state_change(client: &Arc<MidiClient>, enabled: bool) {
    let kind = if enabled {
        event_kinds::ENGINE_ENABLED
    } else {
        event_kinds::ENGINE_DISABLED
    };
    client
        .publisher
        .publish(Event::new(EventSource::Midi, kind, serde_json::json!({})));

    let input_count = {
        let snap = client
            .health_state
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        snap.input_count
    };
    let delta = HealthDelta {
        metric: INPUT_PORTS_METRIC,
        new_value: input_port_health_value(enabled, input_count),
    };
    #[expect(
        clippy::let_underscore_must_use,
        reason = "having no subscribers is a normal state"
    )]
    let _ = client.health_tx.send(delta);
}

fn sync_subscribed_inputs(
    client: &Arc<MidiClient>,
    handles: &HashMap<String, Box<dyn InputHandle>>,
) {
    let mut content = client
        .content_state
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    content.subscribed_inputs = handles.keys().cloned().collect();
}

fn decay_events_per_minute(client: &Arc<MidiClient>) {
    let delta = {
        let mut snap = client
            .health_state
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let before = snap.event_timestamps.len();
        let epm = events_per_minute(&mut snap.event_timestamps);
        (snap.event_timestamps.len() != before).then(|| HealthDelta {
            metric: EVENTS_PER_MINUTE_METRIC,
            new_value: events_per_minute_health_value(epm),
        })
    };
    if let Some(delta) = delta {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "having no subscribers is a normal state"
        )]
        let _ = client.health_tx.send(delta);
    }
}

async fn list_ports(client: &Arc<MidiClient>) -> (Vec<MidiPortInfo>, Vec<MidiPortInfo>) {
    let backend = Arc::clone(&client.backend);
    tokio::task::spawn_blocking(move || (backend.list_input_ports(), backend.list_output_ports()))
        .await
        .unwrap_or_default()
}

async fn do_port_discovery(
    client: &Arc<MidiClient>,
    merged_tx: &mpsc::Sender<RawEvent>,
    input_snap: &mut Vec<MidiPortInfo>,
    output_snap: &mut Vec<MidiPortInfo>,
    handles: &mut HashMap<String, Box<dyn InputHandle>>,
) {
    let (new_inputs, new_outputs) = list_ports(client).await;

    diff_input_ports(client, merged_tx, input_snap, handles, &new_inputs);
    diff_output_ports(client, output_snap, &new_outputs);
    sync_subscribed_inputs(client, handles);

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
        build_port_count_deltas(client, &mut snap, input_count, output_count)
    };
    for d in deltas {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "having no subscribers is a normal state"
        )]
        let _ = client.health_tx.send(d);
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
        if client.enabled.load(Ordering::Relaxed) && !handles.contains_key(&port.name) {
            open_input_port(client, merged_tx, handles, port);
        }
        if !old_names.contains(port.name.as_str()) {
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
            client.backend.release_output(&port.name);
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
                    if merged.send((ts, data, pname.clone())).await.is_err() {
                        break;
                    }
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
        event_kinds::PORT_ADDED
    } else {
        event_kinds::PORT_REMOVED
    };
    let event = Event::new(
        EventSource::Midi,
        kind,
        serde_json::json!({ (port_fields::PORT_NAME): name, (port_fields::DIRECTION): direction.as_str() }),
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
    let signal = to_monitor_event(&event, port_name);
    client.publisher.publish(Event::new(
        EventSource::Midi,
        signal.kind.event_kind(),
        bus_payload(&signal),
    ));

    #[expect(
        clippy::let_underscore_must_use,
        reason = "having no subscribers is a normal state"
    )]
    let _ = client.monitor_tx.send(signal);

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
        #[expect(
            clippy::let_underscore_must_use,
            reason = "having no subscribers is a normal state"
        )]
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
    client: &Arc<MidiClient>,
    snap: &mut MidiHealthSnapshot,
    input_count: usize,
    output_count: usize,
) -> Vec<HealthDelta> {
    let mut deltas = Vec::new();
    if snap.input_count != input_count {
        snap.input_count = input_count;
        let enabled = client.enabled.load(Ordering::Relaxed);
        deltas.push(HealthDelta {
            metric: INPUT_PORTS_METRIC,
            new_value: input_port_health_value(enabled, input_count),
        });
    }
    if snap.output_count != output_count {
        snap.output_count = output_count;
        deltas.push(HealthDelta {
            metric: OUTPUT_PORTS_METRIC,
            new_value: output_port_health_value(output_count),
        });
    }
    deltas
}

fn build_event_deltas(snap: &mut MidiHealthSnapshot, is_note_on: bool) -> Vec<HealthDelta> {
    let mut deltas = Vec::new();

    if is_note_on && snap.last_note_on_at.is_some() {
        deltas.push(HealthDelta {
            metric: LAST_NOTE_ON_METRIC,
            new_value: last_note_health_value(snap.last_note_on_at),
        });
    }

    let epm = events_per_minute(&mut snap.event_timestamps);
    deltas.push(HealthDelta {
        metric: EVENTS_PER_MINUTE_METRIC,
        new_value: events_per_minute_health_value(epm),
    });

    deltas
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use tokio::sync::mpsc;
    use tokio_stream::StreamExt;

    use forge_events::{Event, EventPublisher};

    use super::run_supervisor;
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

    const HANG_GUARD: Duration = Duration::from_secs(30);

    async fn wait_until(what: &str, done: impl Fn() -> bool) {
        let waited = tokio::time::timeout(HANG_GUARD, async {
            while !done() {
                tokio::task::yield_now().await;
            }
        })
        .await;
        assert!(waited.is_ok(), "{what} never happened");
    }

    fn open_ports(backend: &MockMidiBackend) -> usize {
        backend.state.lock().unwrap().senders.len()
    }

    fn launch_client(
        backend: &Arc<MockMidiBackend>,
        publisher: &Arc<RecordingPublisher>,
    ) -> Arc<MidiClient> {
        MidiClient::start(
            MidiConfig::default(),
            Arc::clone(publisher) as Arc<dyn EventPublisher>,
            Arc::clone(backend) as Arc<dyn MidiBackend>,
        )
    }

    async fn start_client(
        backend: Arc<MockMidiBackend>,
        publisher: Arc<RecordingPublisher>,
    ) -> Arc<MidiClient> {
        let client = launch_client(&backend, &publisher);
        let expected = backend.state.lock().unwrap().input_ports.len();
        wait_until("the first discovery", || open_ports(&backend) == expected).await;
        client
    }

    async fn wait_for_port_event(publisher: &RecordingPublisher, kind: &str, port: &str) {
        wait_until(kind, || {
            publisher
                .find_all_kind(kind)
                .iter()
                .any(|e| e.payload["port_name"] == port)
        })
        .await;
    }

    #[tokio::test]
    async fn the_poll_tick_reports_a_hot_plugged_port_without_a_rescan() {
        let backend = Arc::new(MockMidiBackend::new(vec![input_port("A")], vec![]));
        let publisher = RecordingPublisher::new();
        let _client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        backend.set_input_ports(vec![input_port("A"), input_port("B")]);
        wait_for_port_event(&publisher, "midi.port.added", "B").await;

        let added = publisher.find_all_kind("midi.port.added");
        let b_ev = added
            .iter()
            .find(|e| e.payload["port_name"] == "B")
            .unwrap();
        assert_eq!(b_ev.payload["direction"], "input");
    }

    #[tokio::test]
    async fn discovery_reports_an_unplugged_port() {
        let backend = Arc::new(MockMidiBackend::new(
            vec![input_port("A"), input_port("B")],
            vec![],
        ));
        let publisher = RecordingPublisher::new();
        let client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        backend.set_input_ports(vec![input_port("A")]);
        client.rescan_ports().await.unwrap();

        wait_for_port_event(&publisher, "midi.port.removed", "B").await;
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
        assert_eq!(ev.payload["port_name"], "Piano");
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
        assert_eq!(ev.payload["port_name"], "Piano");
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
        assert_eq!(ev.payload["port_name"], "Pad");
    }

    #[tokio::test]
    async fn messages_without_a_decoded_kind_publish_nothing() {
        const CHANNEL_PRESSURE: [u8; 2] = [0xD0, 64];
        const POLY_AFTERTOUCH: [u8; 3] = [0xA0, 60, 64];
        const TIMING_CLOCK: [u8; 1] = [0xF8];
        const NOTE_ON_MARKER: [u8; 3] = [0x90, 60, 100];
        let backend = Arc::new(MockMidiBackend::new(vec![input_port("Piano")], vec![]));
        let publisher = RecordingPublisher::new();
        let _client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        backend.inject_all(0, CHANNEL_PRESSURE.to_vec()).await;
        backend.inject_all(0, POLY_AFTERTOUCH.to_vec()).await;
        backend.inject_all(0, TIMING_CLOCK.to_vec()).await;
        backend.inject_all(0, NOTE_ON_MARKER.to_vec()).await;
        wait_until("the marker note", || {
            publisher.has_kind("midi.input.note_on")
        })
        .await;

        let kinds: Vec<String> = publisher
            .events
            .lock()
            .unwrap()
            .iter()
            .map(|event| event.kind.to_string())
            .filter(|kind| kind.starts_with("midi.input."))
            .collect();
        assert_eq!(kinds, ["midi.input.note_on"]);
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

    #[tokio::test]
    async fn every_port_bearing_event_uses_unified_port_name_key() {
        let backend = Arc::new(MockMidiBackend::new(vec![input_port("Deck")], vec![]));
        let publisher = RecordingPublisher::new();
        let client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        backend.inject_all(0, vec![0x90, 60, 100]).await;
        backend.inject_all(0, vec![0xB0, 7, 64]).await;
        backend.inject_all(0, vec![0xE0, 0, 64]).await;
        wait_until("the injected events", || {
            publisher.has_kind("midi.input.pitch_bend")
        })
        .await;

        backend.set_input_ports(vec![input_port("Deck"), input_port("New")]);
        client.rescan_ports().await.unwrap();
        wait_for_port_event(&publisher, "midi.port.added", "New").await;

        for kind in [
            "midi.input.note_on",
            "midi.input.control_change",
            "midi.input.pitch_bend",
            "midi.port.added",
        ] {
            let ev = publisher.find_kind(kind).unwrap();
            assert!(
                ev.payload
                    .get("port_name")
                    .and_then(|v| v.as_str())
                    .is_some(),
                "{kind} must carry port_name"
            );
            assert!(
                ev.payload.get("port").is_none(),
                "{kind} still carries legacy `port` key"
            );
            assert!(
                ev.payload.get("name").is_none(),
                "{kind} still carries legacy `name` key"
            );
        }
    }

    async fn settle() {
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }
    }

    fn note_on_ports(publisher: &RecordingPublisher) -> Vec<String> {
        publisher
            .find_all_kind("midi.input.note_on")
            .iter()
            .filter_map(|e| {
                e.payload
                    .get("port_name")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
            })
            .collect()
    }

    #[tokio::test]
    async fn disabled_engine_keeps_discovering_ports_but_leaves_them_unsubscribed() {
        let backend = Arc::new(MockMidiBackend::new(vec![input_port("A")], vec![]));
        let publisher = RecordingPublisher::new();
        let client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        client.disable_input().await.unwrap();
        wait_until("the disable", || !client.is_enabled()).await;

        backend.set_input_ports(vec![input_port("A"), input_port("B")]);
        client.rescan_ports().await.unwrap();
        wait_for_port_event(&publisher, "midi.port.added", "B").await;

        assert_eq!(
            open_ports(&backend),
            0,
            "a disabled engine must not subscribe to a newly discovered port"
        );
    }

    #[tokio::test]
    async fn enable_subscribes_ports_discovered_while_disabled() {
        let backend = Arc::new(MockMidiBackend::new(vec![input_port("A")], vec![]));
        let publisher = RecordingPublisher::new();
        let client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        client.disable_input().await.unwrap();
        wait_until("the disable", || !client.is_enabled()).await;
        backend.set_input_ports(vec![input_port("A"), input_port("B")]);
        client.rescan_ports().await.unwrap();
        wait_for_port_event(&publisher, "midi.port.added", "B").await;

        client.enable_input().await.unwrap();
        wait_until("both ports opening", || open_ports(&backend) == 2).await;
        backend.inject_all(0, vec![0x90, 60, 100]).await;

        wait_until("a note from the new port", || {
            note_on_ports(&publisher).contains(&"B".to_owned())
        })
        .await;
    }

    fn input_status(client: &MidiClient, port: &str) -> (Option<forge_types::UserText>, bool) {
        use forge_platform_core::{BuiltinContent, DetailSection};

        let sections = client.sections();
        let Some(DetailSection::TwoColumnLists { left, .. }) = sections.into_iter().next() else {
            panic!("the first MIDI section lists the ports");
        };
        let row = left
            .items
            .iter()
            .find(|item| item.name == port)
            .expect("the port is listed");
        (row.active_label.clone(), row.active)
    }

    #[tokio::test]
    async fn a_port_that_failed_to_open_shows_it_and_is_retried_by_the_next_discovery() {
        let backend = Arc::new(MockMidiBackend::new(vec![input_port("A")], vec![]));
        backend
            .state
            .lock()
            .unwrap()
            .refused_opens
            .insert("A".to_owned());
        let publisher = RecordingPublisher::new();
        let client = launch_client(&backend, &publisher);
        wait_for_port_event(&publisher, "midi.port.added", "A").await;

        assert_eq!(
            input_status(&client, "A"),
            (Some(crate::vocabulary::labels::OPEN_FAILED.into()), false)
        );

        backend.state.lock().unwrap().refused_opens.clear();
        client.rescan_ports().await.unwrap();
        wait_until("the retried open", || open_ports(&backend) == 1).await;
        wait_until("the subscribed status", || {
            input_status(&client, "A") == (Some(crate::vocabulary::labels::SUBSCRIBED.into()), true)
        })
        .await;
    }

    #[tokio::test]
    async fn a_disabled_engine_lists_its_ports_as_disabled_not_failed() {
        let backend = Arc::new(MockMidiBackend::new(vec![input_port("A")], vec![]));
        let client = start_client(Arc::clone(&backend), RecordingPublisher::new()).await;

        client.disable_input().await.unwrap();
        wait_until("the disable", || !client.is_enabled()).await;

        assert_eq!(
            input_status(&client, "A"),
            (Some(crate::vocabulary::labels::DISABLED.into()), false)
        );
    }

    #[tokio::test]
    async fn dropping_the_client_ends_the_supervisor_and_closes_its_ports() {
        let backend = Arc::new(MockMidiBackend::new(vec![input_port("A")], vec![]));
        let client = start_client(Arc::clone(&backend), RecordingPublisher::new()).await;
        assert_eq!(open_ports(&backend), 1);

        drop(client);

        wait_until("the supervisor releasing its ports", || {
            open_ports(&backend) == 0
        })
        .await;
    }

    fn seed_note_timestamp(client: &MidiClient, at: Instant) {
        client
            .health_state
            .lock()
            .unwrap()
            .event_timestamps
            .push_back(at);
    }

    #[tokio::test]
    async fn a_note_leaving_the_rate_window_pushes_a_lower_events_per_minute() {
        use forge_platform_core::{BuiltinHealth, HealthText, HealthValue};

        let client = MidiClient::new_for_test();
        let Some(stale) = Instant::now().checked_sub(Duration::from_secs(120)) else {
            return;
        };
        seed_note_timestamp(&client, stale);
        seed_note_timestamp(&client, Instant::now());
        let health: &dyn BuiltinHealth = &*client;
        let mut stream = health.stream();

        super::decay_events_per_minute(&client);

        let delta = stream.next().await.unwrap();
        assert_eq!(delta.metric, crate::health::EVENTS_PER_MINUTE_METRIC);
        assert!(
            matches!(
                delta.new_value,
                HealthValue::Text {
                    primary: HealthText::Count(1),
                    ..
                }
            ),
            "{:?}",
            delta.new_value
        );
    }

    #[tokio::test]
    async fn a_rate_window_with_no_expired_note_pushes_nothing() {
        use forge_platform_core::{BuiltinHealth, HealthDelta};

        let client = MidiClient::new_for_test();
        seed_note_timestamp(&client, Instant::now());
        let health: &dyn BuiltinHealth = &*client;
        let mut stream = health.stream();

        super::decay_events_per_minute(&client);
        client
            .health_tx
            .send(HealthDelta {
                metric: crate::health::INPUT_PORTS_METRIC,
                new_value: crate::health::input_port_health_value(true, 0),
            })
            .unwrap();

        let first = stream.next().await.unwrap();
        assert_eq!(
            first.metric,
            crate::health::INPUT_PORTS_METRIC,
            "a delta arrived ahead of the marker although nothing aged out"
        );
    }

    #[tokio::test]
    async fn disable_stops_an_already_established_subscription() {
        let backend = Arc::new(MockMidiBackend::new(vec![input_port("A")], vec![]));
        let publisher = RecordingPublisher::new();
        let client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        backend.inject_all(0, vec![0x90, 60, 100]).await;
        settle().await;
        assert_eq!(
            note_on_ports(&publisher).len(),
            1,
            "port A must start subscribed"
        );

        client.disable_input().await.unwrap();
        settle().await;
        backend.inject_all(0, vec![0x90, 62, 100]).await;
        settle().await;

        assert_eq!(
            note_on_ports(&publisher).len(),
            1,
            "disable must close the subscription opened before it"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_closed_control_channel_ends_the_supervisor_instead_of_spinning() {
        let client = MidiClient::new_for_test();
        let (merged_tx, merged_rx) = mpsc::channel(8);
        let (control_tx, control_rx) = mpsc::channel(8);
        let task = tokio::spawn(run_supervisor(
            Arc::downgrade(&client),
            merged_tx,
            merged_rx,
            control_rx,
        ));

        drop(control_tx);

        let finished = tokio::time::timeout(Duration::from_secs(30), task).await;
        assert!(
            finished.is_ok(),
            "the supervisor must exit once its control channel closes"
        );
    }

    #[tokio::test]
    async fn enable_while_already_enabled_does_not_duplicate_a_subscription() {
        let backend = Arc::new(MockMidiBackend::new(vec![input_port("A")], vec![]));
        let publisher = RecordingPublisher::new();
        let client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        client.enable_input().await.unwrap();
        settle().await;
        backend.inject_all(0, vec![0x90, 60, 100]).await;
        settle().await;

        assert_eq!(
            note_on_ports(&publisher).len(),
            1,
            "a redundant enable must not re-open an already open port"
        );
    }

    #[tokio::test]
    async fn engine_enable_and_disable_publish_their_own_event_kinds() {
        let backend = Arc::new(MockMidiBackend::new(vec![input_port("A")], vec![]));
        let publisher = RecordingPublisher::new();
        let client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        client.disable_input().await.unwrap();
        settle().await;
        assert!(publisher.has_kind("midi.engine.disabled"));
        assert!(!publisher.has_kind("midi.engine.enabled"));

        client.enable_input().await.unwrap();
        settle().await;
        assert!(publisher.has_kind("midi.engine.enabled"));
    }

    #[tokio::test]
    async fn disable_pushes_a_disabled_input_port_health_delta() {
        use forge_platform_core::{BuiltinHealth, HealthValue};

        let backend = Arc::new(MockMidiBackend::new(vec![input_port("A")], vec![]));
        let publisher = RecordingPublisher::new();
        let client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        let health: &dyn BuiltinHealth = &*client;
        let mut stream = health.stream();
        client.disable_input().await.unwrap();

        let delta = stream.next().await.unwrap();
        assert_eq!(delta.metric, crate::health::INPUT_PORTS_METRIC);
        assert!(
            matches!(
                &delta.new_value,
                HealthValue::Text { secondary, .. }
                    if *secondary == Some(crate::vocabulary::messages::health_disabled().into())
            ),
            "delta carried {:?}",
            delta.new_value
        );
    }

    #[test]
    fn every_health_delta_producer_targets_a_declared_metric() {
        use forge_platform_core::BuiltinHealth;

        let client = MidiClient::new_for_test();
        let declared: Vec<_> = client
            .metrics()
            .into_iter()
            .map(|metric| metric.id)
            .collect();
        let mut snap = crate::health::MidiHealthSnapshot {
            last_note_on_at: Some(time::OffsetDateTime::UNIX_EPOCH),
            ..Default::default()
        };

        let mut deltas = super::build_port_count_deltas(&client, &mut snap, 2, 1);
        deltas.extend(super::build_event_deltas(&mut snap, true));

        assert_eq!(deltas.len(), 4);
        for delta in &deltas {
            assert!(
                declared.contains(&delta.metric),
                "undeclared {}",
                delta.metric
            );
        }
    }

    #[tokio::test]
    async fn rescan_discovers_a_port_without_waiting_for_the_poll_tick() {
        let backend = Arc::new(MockMidiBackend::new(vec![], vec![]));
        let publisher = RecordingPublisher::new();
        let client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        backend.set_input_ports(vec![input_port("Late")]);
        client.rescan_ports().await.unwrap();

        wait_for_port_event(&publisher, "midi.port.added", "Late").await;
    }

    #[tokio::test]
    async fn monitor_stream_receives_every_decoded_input_event() {
        let backend = Arc::new(MockMidiBackend::new(vec![input_port("Pad")], vec![]));
        let publisher = RecordingPublisher::new();
        let client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        let mut monitor = client.monitor_stream();
        backend.inject_all(0, vec![0xB2, 7, 100]).await;

        let event = monitor.next().await.unwrap();
        assert_eq!(event.kind, crate::MidiSignalKind::ControlChange);
        assert_eq!(event.port_name, "Pad");
    }

    #[tokio::test]
    async fn output_port_added_emits_event_with_output_direction() {
        let backend = Arc::new(MockMidiBackend::new(vec![], vec![]));
        let publisher = RecordingPublisher::new();
        let client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;

        backend.set_output_ports(vec![output_port("Synth")]);
        client.rescan_ports().await.unwrap();
        wait_for_port_event(&publisher, "midi.port.added", "Synth").await;

        let added = publisher.find_all_kind("midi.port.added");
        assert!(
            added.iter().any(|e| e.payload["direction"] == "output"),
            "expected output port added event"
        );
    }

    #[tokio::test]
    async fn a_lost_output_port_releases_only_its_own_connection() {
        let backend = Arc::new(MockMidiBackend::new(
            vec![],
            vec![output_port("Synth"), output_port("Drums")],
        ));
        let publisher = RecordingPublisher::new();
        let client = start_client(Arc::clone(&backend), Arc::clone(&publisher)).await;
        wait_for_port_event(&publisher, "midi.port.added", "Synth").await;

        backend.set_output_ports(vec![output_port("Drums")]);
        client.rescan_ports().await.unwrap();
        wait_for_port_event(&publisher, "midi.port.removed", "Synth").await;

        assert_eq!(backend.released_outputs(), ["Synth"]);
    }

    #[tokio::test]
    async fn shutdown_closes_every_open_input_port_and_ends_the_engine_task() {
        let backend = Arc::new(MockMidiBackend::new(
            vec![input_port("A"), input_port("B")],
            vec![],
        ));
        let client = start_client(Arc::clone(&backend), RecordingPublisher::new()).await;
        let open_ports = || open_ports(&backend);
        assert_eq!(open_ports(), 2, "both inputs open while running");

        client.shutdown().await.unwrap();

        let stopped = tokio::time::timeout(Duration::from_secs(30), async {
            while open_ports() > 0 || Arc::strong_count(&client) > 1 {
                tokio::task::yield_now().await;
            }
        })
        .await;
        assert!(
            stopped.is_ok(),
            "{} port(s) open, {} extra client holder(s) after shutdown",
            open_ports(),
            Arc::strong_count(&client) - 1
        );
    }
}
