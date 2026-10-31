#![expect(
    clippy::panic,
    clippy::unwrap_used,
    reason = "fixture helpers fail the calling test"
)]

use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventPublisher};
use forge_hotkey::testing::{FiredEdge, RecordingBackend, test_client};
use forge_hotkey::vocabulary::event_kinds::{GLOBAL_PRESSED, GLOBAL_RELEASED};
use forge_hotkey::vocabulary::fields;
use forge_hotkey::{HotkeyClient, HotkeyCombo, HotkeyConfig};
use tokio::sync::watch;

const HANG_GUARD: Duration = Duration::from_secs(60);
const SHORTEST_CEILING_SECS: u64 = 1;
const ID_FIELD: &str = "id";
const SYNTHESIZED_FIELD: &str = "synthesized";
const COMBO: &str = "Ctrl+F1";
const LATER_COMBO: &str = "Ctrl+F2";

struct RecordingBus {
    events: watch::Sender<Vec<Event>>,
}

impl EventPublisher for RecordingBus {
    fn publish(&self, event: Event) {
        self.events.send_modify(|events| events.push(event));
    }
}

impl RecordingBus {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            events: watch::Sender::new(Vec::new()),
        })
    }

    async fn wait_for(&self, kind: &str, combo: &HotkeyCombo) -> Event {
        let mut seen = self.events.subscribe();
        let matches =
            |event: &Event| event.kind == kind && event.payload[fields::COMBO] == combo.as_str();
        let events = tokio::time::timeout(
            HANG_GUARD,
            seen.wait_for(|events| events.iter().any(matches)),
        )
        .await
        .unwrap_or_else(|_| panic!("{kind} for {combo} was never published"))
        .unwrap();
        events.iter().find(|event| matches(event)).unwrap().clone()
    }

    fn count(&self, kind: &str, combo: &HotkeyCombo) -> usize {
        self.events
            .borrow()
            .iter()
            .filter(|event| event.kind == kind && event.payload[fields::COMBO] == combo.as_str())
            .count()
    }
}

struct Engine {
    client: Arc<HotkeyClient>,
    backend: Arc<RecordingBackend>,
    bus: Arc<RecordingBus>,
    combo: HotkeyCombo,
    later_combo: HotkeyCombo,
}

impl Engine {
    async fn with_both_combos_registered() -> Self {
        let bus = RecordingBus::new();
        let (client, backend) = test_client(HotkeyConfig::default(), bus.clone());
        let combo = HotkeyCombo::parse(COMBO).unwrap();
        let later_combo = HotkeyCombo::parse(LATER_COMBO).unwrap();
        client.register(combo.clone()).await.unwrap();
        client.register(later_combo.clone()).await.unwrap();
        Self {
            client,
            backend,
            bus,
            combo,
            later_combo,
        }
    }

    async fn press_and_see_it(&self) -> Event {
        self.backend.fire(&self.combo, FiredEdge::Press).await;
        self.bus.wait_for(GLOBAL_PRESSED, &self.combo).await
    }

    async fn prove_earlier_edges_were_handled(&self) {
        self.backend.fire(&self.later_combo, FiredEdge::Press).await;
        self.bus.wait_for(GLOBAL_PRESSED, &self.later_combo).await;
    }
}

#[tokio::test]
async fn a_press_edge_publishes_the_combo_and_its_registered_id() {
    let engine = Engine::with_both_combos_registered().await;
    let registered = engine.client.registered_combos();
    let id = registered
        .iter()
        .find(|(_, c)| *c == engine.combo)
        .unwrap()
        .0;

    let pressed = engine.press_and_see_it().await;

    assert_eq!(pressed.payload[ID_FIELD], id.0);
}

#[tokio::test]
async fn a_release_edge_closes_the_hold_as_an_observed_release_of_that_press() {
    let engine = Engine::with_both_combos_registered().await;
    let pressed = engine.press_and_see_it().await;

    engine.backend.fire(&engine.combo, FiredEdge::Release).await;

    let released = engine.bus.wait_for(GLOBAL_RELEASED, &engine.combo).await;
    assert_eq!(
        (released.caused_by, &released.payload[SYNTHESIZED_FIELD]),
        (Some(pressed.id), &serde_json::Value::Bool(false))
    );
}

#[tokio::test]
async fn key_repeat_while_held_publishes_one_press() {
    let engine = Engine::with_both_combos_registered().await;
    engine.press_and_see_it().await;

    engine.backend.fire(&engine.combo, FiredEdge::Press).await;
    engine.prove_earlier_edges_were_handled().await;

    assert_eq!(engine.bus.count(GLOBAL_PRESSED, &engine.combo), 1);
}

#[tokio::test]
async fn a_press_of_a_combo_nobody_registered_publishes_nothing() {
    let engine = Engine::with_both_combos_registered().await;
    let stranger = HotkeyCombo::parse("Ctrl+F3").unwrap();

    engine.backend.fire(&stranger, FiredEdge::Press).await;
    engine.prove_earlier_edges_were_handled().await;

    assert_eq!(engine.bus.count(GLOBAL_PRESSED, &stranger), 0);
}

#[tokio::test]
async fn a_disabled_engine_drops_presses_until_it_is_enabled_again() {
    let engine = Engine::with_both_combos_registered().await;
    engine.client.disable().await.unwrap();

    engine.backend.fire(&engine.combo, FiredEdge::Press).await;
    let failures = engine.client.enable().await.unwrap();
    engine.prove_earlier_edges_were_handled().await;

    assert!(failures.is_empty());
    assert_eq!(engine.bus.count(GLOBAL_PRESSED, &engine.combo), 0);
    engine.press_and_see_it().await;
}

#[tokio::test]
async fn disabling_the_engine_releases_a_held_combo_as_synthesized() {
    let engine = Engine::with_both_combos_registered().await;
    engine.press_and_see_it().await;

    engine.client.disable().await.unwrap();

    let released = engine.bus.wait_for(GLOBAL_RELEASED, &engine.combo).await;
    assert_eq!(released.payload[SYNTHESIZED_FIELD], true);
}

#[tokio::test]
async fn the_hold_ceiling_releases_a_combo_whose_release_never_arrives() {
    let engine = Engine::with_both_combos_registered().await;
    engine.client.set_hold_ceiling(Some(SHORTEST_CEILING_SECS));
    engine.press_and_see_it().await;

    let released = engine.bus.wait_for(GLOBAL_RELEASED, &engine.combo).await;

    assert_eq!(released.payload[SYNTHESIZED_FIELD], true);
}
