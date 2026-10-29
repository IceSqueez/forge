#![allow(clippy::unwrap_used)]
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_audio::{AudioError, AudioSink};
use forge_events::{
    Event, EventChannel, EventObserver, EventPublisher, EventSource, EventSubscription,
};
use forge_hotkey::ClipBindings;
use forge_hotkey::vocabulary::event_kinds;
use forge_hotkey::vocabulary::fields::COMBO;
use forge_soundboard::NullAudioEventSink;
use forge_soundboard::{
    AudioSinkFactory, ClipLibrary, SoundboardPlayer, SoundboardSettingsHandle, pressed_combo,
    spawn_clip_hotkey_dispatcher,
};
use forge_storage::{MockMediaRepo, SoundboardClipsRepo, StorageError, StoredClip};
use forge_types::{ClipId, OutputDevice};
use serde_json::json;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::watch;

const EVENT_DEADLINE: Duration = Duration::from_secs(30);
const EVENT_CAPACITY: usize = 16;

struct ChannelObserver<'a>(&'a EventChannel);

impl EventObserver for ChannelObserver<'_> {
    fn observe(&self, _consumer: &'static str) -> EventSubscription {
        self.0.subscribe()
    }
}

struct LookupProbe(UnboundedSender<ClipId>);

#[async_trait]
impl SoundboardClipsRepo for LookupProbe {
    async fn list(&self) -> Result<Vec<StoredClip>, StorageError> {
        Ok(Vec::new())
    }

    async fn get(&self, id: ClipId) -> Result<Option<StoredClip>, StorageError> {
        self.0.send(id).unwrap();
        Ok(None)
    }

    async fn save(&self, _clip: &StoredClip) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: ClipId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

struct NoDevice;

#[async_trait]
impl AudioSinkFactory for NoDevice {
    async fn build(&self, _device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError> {
        Err(AudioError::Host("no device in tests".to_owned()))
    }
}

struct Dispatching {
    bus: EventChannel,
    bindings: watch::Sender<Arc<ClipBindings>>,
    toggled: UnboundedReceiver<ClipId>,
}

impl Dispatching {
    fn start(bindings: ClipBindings) -> Self {
        let (probe, toggled) = unbounded_channel();
        let library = Arc::new(ClipLibrary::new(
            Arc::new(LookupProbe(probe)),
            Arc::new(MockMediaRepo::new()),
        ));
        let player = Arc::new(SoundboardPlayer::with_settings(
            Arc::new(NoDevice),
            Arc::new(NullAudioEventSink),
            library,
            SoundboardSettingsHandle::default(),
        ));
        let bus = EventChannel::new(EVENT_CAPACITY);
        let (bindings, snapshot) = watch::channel(Arc::new(bindings));
        spawn_clip_hotkey_dispatcher(&ChannelObserver(&bus), snapshot, player);
        Self {
            bus,
            bindings,
            toggled,
        }
    }

    fn press(&self, combo: &str) {
        self.bus.publish(Event::new(
            EventSource::Hotkey,
            event_kinds::GLOBAL_PRESSED,
            json!({ COMBO: combo }),
        ));
    }

    async fn next_toggled(&mut self) -> ClipId {
        tokio::time::timeout(EVENT_DEADLINE, self.toggled.recv())
            .await
            .unwrap()
            .unwrap()
    }
}

fn bound(combo: &str, clips: &[ClipId]) -> ClipBindings {
    ClipBindings::from([(combo.to_owned(), clips.to_vec())])
}

#[test]
fn pressed_combo_answers_only_a_hotkey_press_that_names_its_combo() {
    for (source, kind, payload, expected) in [
        (
            EventSource::Hotkey,
            event_kinds::GLOBAL_PRESSED,
            json!({ "combo": "F9" }),
            Some("F9"),
        ),
        (
            EventSource::Hotkey,
            event_kinds::GLOBAL_RELEASED,
            json!({ "combo": "F9" }),
            None,
        ),
        (
            EventSource::Midi,
            event_kinds::GLOBAL_PRESSED,
            json!({ "combo": "F9" }),
            None,
        ),
        (
            EventSource::Hotkey,
            event_kinds::GLOBAL_PRESSED,
            json!({}),
            None,
        ),
        (
            EventSource::Hotkey,
            event_kinds::GLOBAL_PRESSED,
            json!({ "combo": 9 }),
            None,
        ),
    ] {
        let event = Event::new(source, kind, payload.clone());

        assert_eq!(
            pressed_combo(&event),
            expected,
            "{source:?} {kind} {payload}"
        );
    }
}

#[tokio::test]
async fn a_press_toggles_every_clip_bound_to_the_combo_and_no_other() {
    let first = ClipId::new();
    let second = ClipId::new();
    let elsewhere = ClipId::new();
    let mut bindings = bound("F9", &[first, second]);
    bindings.insert("F10".to_owned(), vec![elsewhere]);
    let mut dispatching = Dispatching::start(bindings);

    dispatching.press("F9");
    dispatching.press("F10");

    let mut toggled = [
        dispatching.next_toggled().await,
        dispatching.next_toggled().await,
    ];
    toggled.sort();
    let mut expected = [first, second];
    expected.sort();
    assert_eq!(
        (toggled, dispatching.next_toggled().await),
        (expected, elsewhere)
    );
}

#[tokio::test]
async fn a_press_follows_the_latest_published_bindings() {
    let before = ClipId::new();
    let after = ClipId::new();
    let mut dispatching = Dispatching::start(bound("F9", &[before]));

    dispatching
        .bindings
        .send_replace(Arc::new(bound("F9", &[after])));
    dispatching.press("F9");

    assert_eq!(dispatching.next_toggled().await, after);
}
