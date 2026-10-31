use std::sync::Arc;
use std::time::Duration;

use forge_events::test_support::CapturingPublisher;
use forge_midi::testing::{MockMidiBackend, input_port, output_port};
use forge_midi::{MidiClient, MidiConfig, MidiError, MidiOutMessage};
use tokio_stream::StreamExt;

const HANG_GUARD: Duration = Duration::from_secs(30);
const PIANO: &str = "Piano";
const SYNTH: &str = "Synth";

async fn wait_until(what: &str, done: impl Fn() -> bool) {
    let waited = tokio::time::timeout(HANG_GUARD, async {
        while !done() {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(waited.is_ok(), "{what} never happened");
}

struct Engine {
    backend: Arc<MockMidiBackend>,
    publisher: Arc<CapturingPublisher>,
    client: Arc<MidiClient>,
}

async fn started_engine() -> Engine {
    let backend = Arc::new(MockMidiBackend::new(
        vec![input_port(PIANO)],
        vec![output_port(SYNTH)],
    ));
    let publisher = CapturingPublisher::shared();
    let client = MidiClient::start_with_backend(
        MidiConfig::default(),
        Arc::clone(&publisher) as _,
        Arc::clone(&backend),
    );
    let engine = Engine {
        backend,
        publisher,
        client,
    };
    wait_until("the input port opening", || {
        engine.backend.open_input_count() == 1
    })
    .await;
    engine
}

#[tokio::test]
async fn bytes_on_an_opened_port_reach_the_bus_and_the_monitor() {
    let engine = started_engine().await;
    let mut monitor = engine.client.monitor_stream();

    engine.backend.inject_all(0, vec![0x90, 60, 127]).await;
    wait_until("the note reaching the bus", || {
        engine.publisher.find_kind("midi.input.note_on").is_some()
    })
    .await;
    let monitored = tokio::time::timeout(HANG_GUARD, monitor.next()).await;

    let published = engine.publisher.find_kind("midi.input.note_on").unwrap();
    assert_eq!(published.payload["note"], 60);
    assert_eq!(published.payload["port_name"], PIANO);
    assert!(matches!(monitored, Ok(Some(_))));
}

#[tokio::test]
async fn an_output_message_leaves_on_the_named_port_as_its_wire_bytes() {
    let engine = started_engine().await;

    engine
        .client
        .send_output(
            SYNTH,
            &MidiOutMessage::NoteOn {
                note: 60,
                velocity: 100,
                channel: 1,
            },
        )
        .await
        .unwrap();

    assert_eq!(
        engine.backend.sent_outputs(),
        [(SYNTH.to_owned(), vec![0x91, 60, 100])]
    );
}

#[tokio::test]
async fn an_output_to_an_unknown_port_is_refused_by_name() {
    let engine = started_engine().await;

    let refused = engine
        .client
        .send_output(
            "Gone",
            &MidiOutMessage::NoteOn {
                note: 60,
                velocity: 100,
                channel: 0,
            },
        )
        .await;

    assert!(matches!(
        refused,
        Err(MidiError::PortNotFound { name }) if name == "Gone"
    ));
}

#[tokio::test]
async fn a_port_plugged_in_after_start_is_opened_on_a_rescan() {
    let engine = started_engine().await;

    engine
        .backend
        .set_input_ports(vec![input_port(PIANO), input_port("Pad")]);
    engine.client.rescan_ports().await.unwrap();
    wait_until("the new port opening", || {
        engine.backend.open_input_count() == 2
    })
    .await;

    engine.backend.inject_all(0, vec![0x90, 48, 90]).await;
    wait_until("both open ports reaching the bus", || {
        engine.publisher.of_kind("midi.input.note_on").len() == 2
    })
    .await;
}

#[tokio::test]
async fn a_disabled_engine_stops_listening_until_it_is_enabled_again() {
    let engine = started_engine().await;

    engine.client.disable_input().await.unwrap();
    wait_until("the input port closing", || {
        engine.backend.open_input_count() == 0
    })
    .await;
    engine.client.enable_input().await.unwrap();
    wait_until("the input port reopening", || {
        engine.backend.open_input_count() == 1
    })
    .await;

    assert!(engine.client.is_enabled());
}

#[tokio::test]
async fn commands_after_shutdown_report_the_supervisor_as_gone() {
    let engine = started_engine().await;

    engine.client.shutdown().await.unwrap();

    assert!(matches!(
        engine.client.rescan_ports().await,
        Err(MidiError::SupervisorUnavailable)
    ));
}
