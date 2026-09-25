use std::collections::HashSet;
use std::sync::Arc;

use forge_events::{Event, EventSource};
use forge_hotkey::HotkeyCombo;
use forge_storage::{DataProvider, StoredClip};
use forge_types::ClipId;

use crate::combo_conflict::{Claimant, ComboHolder, ComboHolders, clip_keys};
use crate::hotkey_bindings::{BindingRow, load_bindings, registered_combos};
use crate::hotkey_sync::HotkeyReconciler;

const REGISTERED_KIND: &str = "hotkey.registered";
const UNREGISTERED_KIND: &str = "hotkey.unregistered";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PadKey {
    Live,
    NotLive,
    EngineUnavailable,
    Shared(ComboHolder),
}

impl PadKey {
    pub fn is_warning(&self) -> bool {
        *self != PadKey::Live
    }
}

/// An unavailable engine outranks a missing registration, which outranks a combo shared with another holder.
pub fn pad_key(
    available: bool,
    live: &HashSet<String>,
    holders: &ComboHolders,
    clip_id: ClipId,
    combo: &str,
) -> PadKey {
    if !available {
        return PadKey::EngineUnavailable;
    }
    if !live.contains(combo) {
        return PadKey::NotLive;
    }
    match holders.holder_of(combo, Claimant::Clip(Some(clip_id))) {
        Some(holder) => PadKey::Shared(holder),
        None => PadKey::Live,
    }
}

pub fn canonical_combo(raw: &str) -> String {
    HotkeyCombo::parse(raw)
        .map(|parsed| parsed.as_str().to_owned())
        .unwrap_or_else(|_| raw.to_owned())
}

pub fn changes_registrations(event: &Event) -> bool {
    event.source == EventSource::Hotkey
        && matches!(event.kind.as_str(), REGISTERED_KIND | UNREGISTERED_KIND)
}

pub struct ClipKeyState {
    reconciler: Option<Arc<HotkeyReconciler>>,
    backend: Arc<dyn DataProvider>,
    live: HashSet<String>,
    holders: ComboHolders,
}

impl ClipKeyState {
    pub fn new(reconciler: Option<Arc<HotkeyReconciler>>, backend: Arc<dyn DataProvider>) -> Self {
        let mut state = Self {
            reconciler,
            backend,
            live: HashSet::new(),
            holders: ComboHolders::default(),
        };
        state.refresh_live();
        state
    }

    pub fn available(&self) -> bool {
        self.reconciler.is_some()
    }

    pub fn reconciler(&self) -> Option<&Arc<HotkeyReconciler>> {
        self.reconciler.as_ref()
    }

    pub fn backend(&self) -> &Arc<dyn DataProvider> {
        &self.backend
    }

    pub fn holders(&self) -> ComboHolders {
        self.holders.clone()
    }

    pub fn refresh_live(&mut self) {
        self.live = match &self.reconciler {
            Some(reconciler) => registered_combos(reconciler.client())
                .into_iter()
                .map(|(_, combo)| combo)
                .collect(),
            None => HashSet::new(),
        };
    }

    pub fn set_clips(&mut self, clips: &[StoredClip]) {
        self.holders.clips = clip_keys(clips);
    }

    pub fn set_rows(&mut self, rows: Vec<BindingRow>) {
        self.holders.rows = Arc::new(rows);
    }

    pub fn pad_key(&self, clip_id: ClipId, combo: &str) -> PadKey {
        pad_key(
            self.available(),
            &self.live,
            &self.holders,
            clip_id,
            &canonical_combo(combo),
        )
    }

    pub fn load_rows(
        &self,
    ) -> Option<impl Future<Output = Result<Vec<BindingRow>, String>> + Send + 'static> {
        let reconciler = self.reconciler.as_ref()?;
        let registered = registered_combos(reconciler.client());
        Some(load_bindings(Arc::clone(&self.backend), registered))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use forge_hotkey::HotkeyCombo;
    use forge_types::{ActionId, TriggerInstanceId};
    use serde_json::json;

    use super::*;
    use crate::combo_conflict::ClipKey;
    use crate::hotkey_bindings::{BindingHalf, HOTKEY_PRESSED_KIND};
    use crate::test_support::sandboxed_backend;

    const TEST_KEY: [u8; 32] = [0x11; 32];

    fn live(combos: &[&str]) -> HashSet<String> {
        combos.iter().map(|combo| (*combo).to_owned()).collect()
    }

    fn trigger_on(combo: &str) -> BindingRow {
        let press = TriggerInstanceId::new();
        BindingRow {
            key: press,
            combo: combo.to_owned(),
            registered: true,
            press: Some(BindingHalf {
                instance_id: press,
                enabled: true,
                action: Some((ActionId::new(), "Scene".to_owned())),
            }),
            release: None,
        }
    }

    fn clip_on(id: ClipId, combo: &str) -> ClipKey {
        ClipKey {
            id,
            name: "airhorn".to_owned(),
            combo: combo.to_owned(),
        }
    }

    #[test]
    fn an_unavailable_engine_outranks_a_missing_registration_which_outranks_sharing() {
        let this = ClipId::new();
        let other = ClipId::new();
        let shared = ComboHolders {
            rows: Arc::new(vec![trigger_on("F9")]),
            clips: vec![clip_on(this, "F9")],
        };
        let alone = ComboHolders {
            rows: Arc::new(Vec::new()),
            clips: vec![clip_on(this, "F9")],
        };
        let with_clip = ComboHolders {
            rows: Arc::new(Vec::new()),
            clips: vec![clip_on(this, "F9"), clip_on(other, "F9")],
        };

        for (case, available, registered, holders, expected) in [
            (
                "no engine, nothing live",
                false,
                live(&[]),
                &shared,
                PadKey::EngineUnavailable,
            ),
            (
                "engine up, key not live, shared",
                true,
                live(&["F10"]),
                &shared,
                PadKey::NotLive,
            ),
            (
                "live and shared with a trigger",
                true,
                live(&["F9"]),
                &shared,
                PadKey::Shared(ComboHolder::Action(Some("Scene".to_owned()))),
            ),
            (
                "live and shared with another clip",
                true,
                live(&["F9"]),
                &with_clip,
                PadKey::Shared(ComboHolder::Clip {
                    id: other,
                    name: "airhorn".to_owned(),
                }),
            ),
            (
                "live and held by this clip alone",
                true,
                live(&["F9"]),
                &alone,
                PadKey::Live,
            ),
        ] {
            assert_eq!(
                pad_key(available, &registered, holders, this, "F9"),
                expected,
                "{case}"
            );
        }
    }

    #[test]
    fn only_a_hotkey_registration_change_refreshes_the_key_badges() {
        for (source, kind, expected) in [
            (EventSource::Hotkey, REGISTERED_KIND, true),
            (EventSource::Hotkey, UNREGISTERED_KIND, true),
            (EventSource::Hotkey, HOTKEY_PRESSED_KIND, false),
            (EventSource::Midi, REGISTERED_KIND, false),
        ] {
            let event = Event::new(source, kind, json!({ "combo": "F9" }));

            assert_eq!(changes_registrations(&event), expected, "{source:?} {kind}");
        }
    }

    #[test]
    fn canonical_combo_normalises_a_parseable_key_and_keeps_anything_else_verbatim() {
        for (raw, expected) in [
            ("ctrl+f9", "Ctrl+F9"),
            ("Ctrl+F9", "Ctrl+F9"),
            ("not a key", "not a key"),
        ] {
            assert_eq!(canonical_combo(raw), expected, "{raw}");
        }
    }

    struct SilentPublisher;

    impl forge_events::EventPublisher for SilentPublisher {
        fn publish(&self, _: forge_events::Event) {}
    }

    #[tokio::test]
    async fn the_badge_follows_the_registered_set_only_when_refreshed() {
        let backend = sandboxed_backend("sqlite::memory:", TEST_KEY)
            .await
            .map(|backend| Arc::new(backend) as Arc<dyn DataProvider>);
        let (client, _recorder) = forge_hotkey::testing::test_client(
            forge_hotkey::HotkeyConfig::default(),
            Arc::new(SilentPublisher),
        );
        let reconciler = HotkeyReconciler::new(
            Arc::clone(&client),
            backend.trigger_instance_repo(),
            backend.soundboard_clips_repo(),
        );
        let mut state = ClipKeyState::new(Some(reconciler), Arc::clone(&backend));
        let clip = ClipId::new();
        let mut seen = vec![state.pad_key(clip, "f9")];

        let id = client
            .register(HotkeyCombo::parse("F9").unwrap())
            .await
            .unwrap();
        seen.push(state.pad_key(clip, "f9"));
        state.refresh_live();
        seen.push(state.pad_key(clip, "f9"));
        client.unregister(id).await.unwrap();
        state.refresh_live();
        seen.push(state.pad_key(clip, "f9"));

        assert_eq!(
            seen,
            [
                PadKey::NotLive,
                PadKey::NotLive,
                PadKey::Live,
                PadKey::NotLive
            ]
        );
    }
}
