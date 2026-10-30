use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use forge_storage::{SoundboardClipsRepo, TriggerInstanceRepo};
use tokio::sync::{Mutex, watch};

use crate::binding_store::{bound_hotkey_combos, persisted_hotkey_combos};
use crate::client::HotkeyClient;
use crate::clip_bindings::{ClipBindings, clip_bindings_of};
use crate::combo::HotkeyCombo;
use crate::error::HotkeyError;

pub struct HotkeyReconciler {
    client: Arc<HotkeyClient>,
    triggers: Arc<dyn TriggerInstanceRepo>,
    clips: Arc<dyn SoundboardClipsRepo>,
    serial: Mutex<()>,
    clip_bindings: watch::Sender<Arc<ClipBindings>>,
    known_combos: std::sync::Mutex<Option<BTreeSet<String>>>,
    enable_engine: OnceLock<Arc<dyn Fn() + Send + Sync>>,
}

impl HotkeyReconciler {
    pub fn new(
        client: Arc<HotkeyClient>,
        triggers: Arc<dyn TriggerInstanceRepo>,
        clips: Arc<dyn SoundboardClipsRepo>,
    ) -> Arc<Self> {
        let (clip_bindings, _) = watch::channel(Arc::new(ClipBindings::new()));
        Arc::new(Self {
            client,
            triggers,
            clips,
            serial: Mutex::new(()),
            clip_bindings,
            known_combos: std::sync::Mutex::new(None),
            enable_engine: OnceLock::new(),
        })
    }

    pub fn enable_engine_on_new_bindings(&self, enable: impl Fn() + Send + Sync + 'static) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the first registered enable hook wins"
        )]
        let _ = self.enable_engine.set(Arc::new(enable));
    }

    fn note_bound(&self, bound: BTreeSet<String>, wanted: &BTreeSet<String>) -> bool {
        let mut known = self.known_combos.lock().unwrap_or_else(|p| p.into_inner());
        let grew = known
            .as_ref()
            .is_some_and(|previous| wanted.iter().any(|combo| !previous.contains(combo)));
        *known = Some(bound);
        grew
    }

    pub fn client(&self) -> &Arc<HotkeyClient> {
        &self.client
    }

    pub fn clip_bindings(&self) -> watch::Receiver<Arc<ClipBindings>> {
        self.clip_bindings.subscribe()
    }

    pub async fn reconcile(&self) {
        let _serial = self.serial.lock().await;
        let instances = match self.triggers.list_all().await {
            Ok(instances) => instances,
            Err(e) => {
                tracing::warn!(error = %e, "could not load hotkey triggers to sync registrations");
                return;
            }
        };
        let clips = match self.clips.list().await {
            Ok(clips) => clips,
            Err(e) => {
                tracing::warn!(error = %e, "could not load clip hotkeys to sync registrations");
                return;
            }
        };
        let bindings = clip_bindings_of(&clips);
        let mut wanted = persisted_hotkey_combos(&instances);
        wanted.extend(bindings.keys().cloned());
        let mut bound = bound_hotkey_combos(&instances);
        bound.extend(bindings.keys().cloned());
        self.clip_bindings.send_replace(Arc::new(bindings));
        if self.note_bound(bound, &wanted)
            && let Some(enable) = self.enable_engine.get()
        {
            enable();
        }
        apply_wanted(&self.client, &wanted).await;
    }

    pub async fn claim(&self, combo: HotkeyCombo) -> Result<(), HotkeyError> {
        let _serial = self.serial.lock().await;
        if self
            .client
            .registered_combos()
            .iter()
            .any(|(_, known)| known.as_str() == combo.as_str())
        {
            return Ok(());
        }
        self.client.register(combo).await.map(|_| ())
    }
}

async fn apply_wanted(client: &HotkeyClient, wanted: &BTreeSet<String>) {
    let registered = client.registered_combos();

    for (id, combo) in &registered {
        if !wanted.contains(combo.as_str())
            && let Err(e) = client.unregister(*id).await
        {
            tracing::warn!(combo = %combo, error = %e, "could not release a hotkey nothing binds");
        }
    }

    for raw in wanted.iter().filter(|raw| !raw.is_empty()) {
        if registered.iter().any(|(_, known)| known.as_str() == raw) {
            continue;
        }
        let outcome = match HotkeyCombo::parse(raw) {
            Ok(combo) => client.register(combo).await.map(|_| ()),
            Err(e) => Err(e),
        };
        if let Err(e) = outcome {
            tracing::warn!(combo = %raw, error = %e, "could not register a stored hotkey binding");
        }
    }
}
