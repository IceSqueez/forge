use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource, EventsError};
use forge_hotkey::HotkeyCombo;
use forge_runtime::EventSubscription;
use forge_soundboard::SoundboardPlayer;
use forge_storage::{SoundboardClipsRepo, StorageError, StoredClip};
use forge_types::ClipId;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::hotkey_bindings::HOTKEY_PRESSED_KIND;
use crate::hotkey_sync::HotkeyReconciler;

pub const CLIP_COMBO_FIELD: &str = "hotkey";
const PRESSED_COMBO_KEY: &str = "combo";

/// Several clips on one combo is a collision already in the data; every one of them answers the press.
pub type ClipBindings = BTreeMap<String, Vec<ClipId>>;

pub fn clip_bindings_of(clips: &[StoredClip]) -> ClipBindings {
    let mut bindings = ClipBindings::new();
    for clip in clips {
        if let Some(combo) = stored_combo(clip) {
            bindings.entry(combo).or_default().push(clip.id);
        }
    }
    bindings
}

/// Canonical when the stored text parses, so a hand-typed `f5` matches a captured `F5`.
pub fn stored_combo(clip: &StoredClip) -> Option<String> {
    let raw = clip.hotkey.as_deref().map(str::trim)?;
    if raw.is_empty() {
        return None;
    }
    Some(
        HotkeyCombo::parse(raw)
            .map(|parsed| parsed.as_str().to_owned())
            .unwrap_or_else(|_| raw.to_owned()),
    )
}

pub async fn clear_clip_combo(
    repo: Arc<dyn SoundboardClipsRepo>,
    id: ClipId,
) -> Result<(), String> {
    let Some(mut clip) = repo.get(id).await.map_err(|e| e.to_string())? else {
        return Ok(());
    };
    clip.hotkey = None;
    repo.save(&clip).await.map_err(|e| e.to_string())
}

/// A blank combo is stored as no binding; an unparseable one is refused on the binding field.
pub fn canonical_clip(clip: &StoredClip) -> Result<StoredClip, StorageError> {
    let mut canonical = clip.clone();
    canonical.hotkey = match clip.hotkey.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(raw) => {
            let combo = HotkeyCombo::parse(raw).map_err(|e| StorageError::ValidationFailed {
                field: CLIP_COMBO_FIELD.to_owned(),
                reason: e.to_string(),
            })?;
            Some(combo.as_str().to_owned())
        }
    };
    Ok(canonical)
}

/// Every clip write, whichever screen or service made it, ends in a reconcile run.
pub struct HotkeySyncedClipsRepo {
    inner: Arc<dyn SoundboardClipsRepo>,
    reconciler: Option<Arc<HotkeyReconciler>>,
}

impl HotkeySyncedClipsRepo {
    pub fn wrap(
        inner: Arc<dyn SoundboardClipsRepo>,
        reconciler: Option<Arc<HotkeyReconciler>>,
    ) -> Arc<dyn SoundboardClipsRepo> {
        Arc::new(Self { inner, reconciler })
    }

    async fn sync(&self) {
        if let Some(reconciler) = &self.reconciler {
            reconciler.reconcile().await;
        }
    }
}

#[async_trait]
impl SoundboardClipsRepo for HotkeySyncedClipsRepo {
    async fn list(&self) -> Result<Vec<StoredClip>, StorageError> {
        self.inner.list().await
    }

    async fn get(&self, id: ClipId) -> Result<Option<StoredClip>, StorageError> {
        self.inner.get(id).await
    }

    async fn save(&self, clip: &StoredClip) -> Result<(), StorageError> {
        let canonical = canonical_clip(clip)?;
        self.inner.save(&canonical).await?;
        self.sync().await;
        Ok(())
    }

    async fn delete(&self, id: ClipId) -> Result<bool, StorageError> {
        let deleted = self.inner.delete(id).await?;
        self.sync().await;
        Ok(deleted)
    }
}

pub fn pressed_combo(event: &Event) -> Option<&str> {
    if event.source != EventSource::Hotkey || event.kind != HOTKEY_PRESSED_KIND {
        return None;
    }
    event.payload.get(PRESSED_COMBO_KEY)?.as_str()
}

pub fn clips_bound_to(bindings: &ClipBindings, combo: &str) -> Vec<ClipId> {
    bindings.get(combo).cloned().unwrap_or_default()
}

/// Press edges only; the hotkey engine already drops auto-repeat and presses while it is off.
pub fn spawn_clip_hotkey_dispatcher(
    mut events: EventSubscription,
    bindings: watch::Receiver<Arc<ClipBindings>>,
    player: Arc<SoundboardPlayer>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let event = match events.recv().await {
                Ok(event) => event,
                Err(EventsError::LaggingReceiver) => {
                    tracing::warn!("clip hotkey dispatcher lagged; some presses were dropped");
                    continue;
                }
                Err(_) => break,
            };
            let Some(combo) = pressed_combo(&event) else {
                continue;
            };
            let clips = clips_bound_to(&bindings.borrow(), combo);
            for clip_id in clips {
                let player = Arc::clone(&player);
                tokio::spawn(async move {
                    if let Err(e) = player.toggle(clip_id, None).await {
                        tracing::warn!(clip_id = %clip_id, error = %e, "clip hotkey could not toggle the clip");
                    }
                });
            }
        }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use forge_audio::{AudioError, AudioSink, NullAudioEventSink};
    use forge_runtime::EventBus;
    use forge_soundboard::{AudioSinkFactory, ClipLibrary, SoundboardSettingsHandle};
    use forge_storage::MockMediaRepo;
    use forge_types::OutputDevice;
    use serde_json::json;
    use time::OffsetDateTime;
    use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

    use super::*;
    use crate::hotkey_bindings::HOTKEY_RELEASED_KIND;
    use crate::test_support::StubEventLog;

    const EVENT_DEADLINE: Duration = Duration::from_secs(2);

    fn clip(hotkey: Option<&str>) -> StoredClip {
        StoredClip {
            id: ClipId::new(),
            name: "airhorn".to_owned(),
            file_path: PathBuf::from("/clips/airhorn.wav"),
            volume: 1.0,
            output_device: OutputDevice::Default,
            hotkey: hotkey.map(str::to_owned),
            created_at: OffsetDateTime::now_utc(),
            category: String::new(),
            loop_playback: false,
            duration_secs: None,
            builtin_id: None,
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
        bus: Arc<EventBus>,
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
            let bus = EventBus::new(Arc::new(StubEventLog));
            let (bindings, snapshot) = watch::channel(Arc::new(bindings));
            spawn_clip_hotkey_dispatcher(bus.subscribe(), snapshot, player);
            Self {
                bus,
                bindings,
                toggled,
            }
        }

        fn press(&self, combo: &str) {
            self.bus.publish(Event::new(
                EventSource::Hotkey,
                HOTKEY_PRESSED_KIND,
                json!({ PRESSED_COMBO_KEY: combo }),
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
                HOTKEY_PRESSED_KIND,
                json!({ "combo": "F9" }),
                Some("F9"),
            ),
            (
                EventSource::Hotkey,
                HOTKEY_RELEASED_KIND,
                json!({ "combo": "F9" }),
                None,
            ),
            (
                EventSource::Midi,
                HOTKEY_PRESSED_KIND,
                json!({ "combo": "F9" }),
                None,
            ),
            (EventSource::Hotkey, HOTKEY_PRESSED_KIND, json!({}), None),
            (
                EventSource::Hotkey,
                HOTKEY_PRESSED_KIND,
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

    #[test]
    fn clip_bindings_group_every_clip_on_a_combo_under_its_canonical_spelling() {
        let typed = clip(Some("f9"));
        let canonical = clip(Some("F9"));
        let clips = [
            typed.clone(),
            canonical.clone(),
            clip(None),
            clip(Some("  ")),
        ];

        let bindings = clip_bindings_of(&clips);

        assert_eq!(bindings, bound("F9", &[typed.id, canonical.id]));
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
}
