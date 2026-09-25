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
