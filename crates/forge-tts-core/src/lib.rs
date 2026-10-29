use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use forge_speech_types::{EngineId, PcmBuffer, TtsVoice, VoiceId};
use serde::{Deserialize, Serialize};

mod ssml;

pub use ssml::xml_escape;

pub const ENGINE_ID_PIPER: &str = "piper";
pub const ENGINE_ID_ESPEAK_NG: &str = "espeak-ng";
pub const ENGINE_ID_SAPI: &str = "sapi";
pub const ENGINE_ID_NSSPEECH: &str = "nsspeech";
pub const ENGINE_ID_AZURE: &str = "azure";
pub const ENGINE_ID_ELEVENLABS: &str = "elevenlabs";
pub const ENGINE_ID_OPENAI: &str = "openai";
pub const ENGINE_ID_POLLY: &str = "polly";

pub fn engine_display_name(id: &str) -> &str {
    match id {
        ENGINE_ID_PIPER => "Piper",
        ENGINE_ID_ESPEAK_NG => "eSpeak-NG",
        ENGINE_ID_SAPI => "Microsoft SAPI 5",
        ENGINE_ID_NSSPEECH => "Apple AVSpeech",
        ENGINE_ID_AZURE => "Azure Speech",
        ENGINE_ID_ELEVENLABS => "ElevenLabs",
        ENGINE_ID_OPENAI => "OpenAI TTS",
        ENGINE_ID_POLLY => "Amazon Polly",
        other => other,
    }
}

#[derive(Debug, Clone)]
pub struct SynthesisRequest {
    pub text: String,
    pub voice_id: VoiceId,
    pub pitch_semitones: f32,
    pub rate_multiplier: f32,
    pub ssml: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EngineCapabilities {
    pub ssml: bool,
    pub neural_voices: bool,
    pub streaming: bool,
    pub custom_lexicons: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EngineClass {
    Local,
    System,
    Cloud,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredEngine {
    pub id: EngineId,
    pub class: EngineClass,
}

#[derive(Debug, thiserror::Error)]
pub enum TtsError {
    #[error("synthesis timed out after {ms}ms")]
    Timeout { ms: u64 },

    #[error("authentication failed: {reason}")]
    AuthFailed { reason: String },

    #[error("engine {id:?} is unavailable: {detail}")]
    EngineUnavailable { id: EngineId, detail: String },

    #[error("rate limited; retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("voice {id:?} is not recognized by this engine")]
    InvalidVoice { id: VoiceId },

    #[error("network failure: {0}")]
    NetworkFailed(String),

    #[error("SSML is not supported by engine {id:?}")]
    SsmlUnsupported { id: EngineId },

    #[error("engine I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("quota exhausted for engine {id:?}: {detail}")]
    QuotaExceeded { id: EngineId, detail: String },

    #[error("engine {id:?} failed to synthesize: {detail}")]
    SynthesisFailed { id: EngineId, detail: String },
}

#[async_trait]
pub trait TtsEngine: Send + Sync {
    fn engine_id(&self) -> &EngineId;
    fn capabilities(&self) -> &EngineCapabilities;
    async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError>;
    async fn synthesize(&self, request: SynthesisRequest) -> Result<PcmBuffer, TtsError>;

    async fn test_connection(&self) -> Result<(), TtsError> {
        self.list_voices().await.map(|_| ())
    }
}

pub trait TtsEngineFactory: Send + Sync {
    fn class(&self) -> EngineClass;

    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError>;
}

pub async fn build_engine(
    id: &EngineId,
    factory: Arc<dyn TtsEngineFactory>,
) -> Result<Arc<dyn TtsEngine>, TtsError> {
    tokio::task::spawn_blocking(move || factory.create())
        .await
        .map_err(|e| TtsError::EngineUnavailable {
            id: id.clone(),
            detail: e.to_string(),
        })?
        .map(Arc::from)
}

pub struct EngineSlot {
    id: EngineId,
    factory: Arc<dyn TtsEngineFactory>,
    engine: tokio::sync::OnceCell<Arc<dyn TtsEngine>>,
}

impl EngineSlot {
    fn unbuilt(id: EngineId, factory: Arc<dyn TtsEngineFactory>) -> Self {
        Self {
            id,
            factory,
            engine: tokio::sync::OnceCell::new(),
        }
    }

    fn built(id: EngineId, factory: Arc<dyn TtsEngineFactory>, engine: Arc<dyn TtsEngine>) -> Self {
        Self {
            id,
            factory,
            engine: tokio::sync::OnceCell::new_with(Some(engine)),
        }
    }

    pub async fn engine(&self) -> Result<Arc<dyn TtsEngine>, TtsError> {
        self.engine
            .get_or_try_init(|| build_engine(&self.id, Arc::clone(&self.factory)))
            .await
            .cloned()
    }
}

pub struct TtsRegistry {
    slots: HashMap<EngineId, Arc<EngineSlot>>,
}

impl TtsRegistry {
    pub fn new() -> Self {
        Self {
            slots: HashMap::new(),
        }
    }

    pub fn register(&mut self, id: EngineId, factory: Arc<dyn TtsEngineFactory>) {
        let slot = EngineSlot::unbuilt(id.clone(), factory);
        self.slots.insert(id, Arc::new(slot));
    }

    pub fn register_built(
        &mut self,
        id: EngineId,
        factory: Arc<dyn TtsEngineFactory>,
        engine: Arc<dyn TtsEngine>,
    ) {
        let slot = EngineSlot::built(id.clone(), factory, engine);
        self.slots.insert(id, Arc::new(slot));
    }

    pub fn slot(&self, id: &EngineId) -> Option<Arc<EngineSlot>> {
        self.slots.get(id).cloned()
    }

    pub fn invalidate(&mut self, id: &EngineId) {
        if let Some(slot) = self.slots.get_mut(id) {
            *slot = Arc::new(EngineSlot::unbuilt(id.clone(), Arc::clone(&slot.factory)));
        }
    }

    pub fn invalidate_all(&mut self) {
        let ids: Vec<EngineId> = self.slots.keys().cloned().collect();
        for id in ids {
            self.invalidate(&id);
        }
    }

    pub fn roster(&self) -> Vec<RegisteredEngine> {
        let mut roster: Vec<_> = self
            .slots
            .iter()
            .map(|(id, slot)| RegisteredEngine {
                id: id.clone(),
                class: slot.factory.class(),
            })
            .collect();
        roster.sort_by(|a, b| a.id.0.cmp(&b.id.0));
        roster
    }

    pub fn engine_ids(&self) -> Vec<EngineId> {
        let mut ids: Vec<_> = self.slots.keys().cloned().collect();
        ids.sort_by(|a, b| a.0.cmp(&b.0));
        ids
    }
}

impl Default for TtsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct NullEngine {
        id: EngineId,
        caps: EngineCapabilities,
    }

    #[async_trait]
    impl TtsEngine for NullEngine {
        fn engine_id(&self) -> &EngineId {
            &self.id
        }
        fn capabilities(&self) -> &EngineCapabilities {
            &self.caps
        }
        async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError> {
            Ok(Vec::new())
        }
        async fn synthesize(&self, _request: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
            Ok(PcmBuffer::new(vec![0; 4], 22_050, 1))
        }
    }

    fn null_engine(id: &str) -> Arc<dyn TtsEngine> {
        Arc::new(NullEngine {
            id: EngineId(id.into()),
            caps: EngineCapabilities::default(),
        })
    }

    struct CountingFactory {
        id: EngineId,
        creates: AtomicUsize,
        failing_first: usize,
        gate: Option<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
        entered: Option<tokio::sync::mpsc::UnboundedSender<()>>,
    }

    impl CountingFactory {
        fn new(id: &str) -> Arc<Self> {
            Arc::new(Self::plain(id))
        }

        fn plain(id: &str) -> Self {
            Self {
                id: EngineId(id.into()),
                creates: AtomicUsize::new(0),
                failing_first: 0,
                gate: None,
                entered: None,
            }
        }

        fn creates(&self) -> usize {
            self.creates.load(Ordering::SeqCst)
        }
    }

    impl TtsEngineFactory for CountingFactory {
        fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
            let attempt = self.creates.fetch_add(1, Ordering::SeqCst);
            if let Some(entered) = &self.entered {
                entered.send(()).expect("test still listening");
            }
            if let Some(gate) = &self.gate {
                gate.lock().unwrap().recv().expect("gate opened");
            }
            if attempt < self.failing_first {
                return Err(TtsError::EngineUnavailable {
                    id: self.id.clone(),
                    detail: "not ready yet".into(),
                });
            }
            Ok(Box::new(NullEngine {
                id: self.id.clone(),
                caps: EngineCapabilities::default(),
            }))
        }
    }

    struct PanickingFactory;

    impl TtsEngineFactory for PanickingFactory {
        fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
            panic!("engine constructor crashed")
        }
    }

    fn id(name: &str) -> EngineId {
        EngineId(name.into())
    }

    async fn engine_of(reg: &TtsRegistry, name: &str) -> Arc<dyn TtsEngine> {
        reg.slot(&id(name))
            .expect("registered")
            .engine()
            .await
            .expect("engine builds")
    }

    #[test]
    fn engine_ids_show_their_product_names_and_unknown_ids_show_as_is() {
        for (id, shown) in [
            ("piper", "Piper"),
            ("espeak-ng", "eSpeak-NG"),
            ("sapi", "Microsoft SAPI 5"),
            ("nsspeech", "Apple AVSpeech"),
            ("azure", "Azure Speech"),
            ("elevenlabs", "ElevenLabs"),
            ("openai", "OpenAI TTS"),
            ("polly", "Amazon Polly"),
            ("avfoundation", "avfoundation"),
            ("my-engine", "my-engine"),
            ("", ""),
        ] {
            assert_eq!(engine_display_name(id), shown, "{id:?}");
        }
    }

    #[test]
    fn engine_ids_lists_every_registered_engine_sorted_by_id() {
        let mut reg = TtsRegistry::new();
        reg.register(id("zzz"), CountingFactory::new("zzz"));
        reg.register(id("aaa"), CountingFactory::new("aaa"));
        reg.register_built(id("mmm"), CountingFactory::new("mmm"), null_engine("mmm"));

        assert_eq!(reg.engine_ids(), vec![id("aaa"), id("mmm"), id("zzz")]);
    }

    #[tokio::test]
    async fn every_use_of_a_registered_engine_shares_one_built_instance() {
        let factory = CountingFactory::new("alpha");
        let mut reg = TtsRegistry::new();
        reg.register(id("alpha"), factory.clone());

        let first = engine_of(&reg, "alpha").await;
        for _ in 0..4 {
            assert!(Arc::ptr_eq(&first, &engine_of(&reg, "alpha").await));
        }
        assert_eq!(factory.creates(), 1);
    }

    #[tokio::test]
    async fn a_failed_build_leaves_the_slot_empty_and_the_next_use_retries() {
        let factory = Arc::new(CountingFactory {
            failing_first: 1,
            ..CountingFactory::plain("alpha")
        });
        let mut reg = TtsRegistry::new();
        reg.register(id("alpha"), factory.clone());
        let slot = reg.slot(&id("alpha")).expect("registered");

        let failed = slot.engine().await;
        let served = slot.engine().await;

        assert!(matches!(failed, Err(TtsError::EngineUnavailable { .. })));
        assert!(served.is_ok(), "the retry must serve the engine");
        assert_eq!(factory.creates(), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_first_uses_build_the_engine_once() {
        const USERS: usize = 8;
        let (open_gate, gate) = std::sync::mpsc::channel();
        let (entered_tx, mut entered_rx) = tokio::sync::mpsc::unbounded_channel();
        let factory = Arc::new(CountingFactory {
            gate: Some(std::sync::Mutex::new(gate)),
            entered: Some(entered_tx),
            ..CountingFactory::plain("alpha")
        });
        let mut reg = TtsRegistry::new();
        reg.register(id("alpha"), factory.clone());
        let slot = reg.slot(&id("alpha")).expect("registered");

        let users: Vec<_> = (0..USERS)
            .map(|_| {
                let slot = Arc::clone(&slot);
                tokio::spawn(async move { slot.engine().await })
            })
            .collect();
        entered_rx.recv().await.expect("a build started");
        for _ in 0..USERS {
            tokio::task::yield_now().await;
        }
        for _ in 0..USERS {
            open_gate.send(()).expect("factory listening");
        }
        let mut engines = Vec::new();
        for user in users {
            engines.push(user.await.expect("task").expect("engine"));
        }

        assert_eq!(factory.creates(), 1);
        assert!(engines.iter().all(|e| Arc::ptr_eq(e, &engines[0])));
    }

    #[tokio::test]
    async fn a_pre_built_engine_is_served_without_calling_its_factory() {
        let factory = CountingFactory::new("alpha");
        let prebuilt = null_engine("alpha");
        let mut reg = TtsRegistry::new();
        reg.register_built(id("alpha"), factory.clone(), prebuilt.clone());

        let served = engine_of(&reg, "alpha").await;

        assert!(Arc::ptr_eq(&served, &prebuilt));
        assert_eq!(factory.creates(), 0);
    }

    #[tokio::test]
    async fn invalidating_one_engine_rebuilds_only_that_engine_from_its_factory() {
        let alpha = CountingFactory::new("alpha");
        let beta = CountingFactory::new("beta");
        let alpha_prebuilt = null_engine("alpha");
        let mut reg = TtsRegistry::new();
        reg.register_built(id("alpha"), alpha.clone(), alpha_prebuilt.clone());
        reg.register(id("beta"), beta.clone());
        let beta_before = engine_of(&reg, "beta").await;

        reg.invalidate(&id("alpha"));
        let alpha_after = engine_of(&reg, "alpha").await;
        let beta_after = engine_of(&reg, "beta").await;

        assert!(!Arc::ptr_eq(&alpha_after, &alpha_prebuilt));
        assert_eq!(alpha.creates(), 1);
        assert!(Arc::ptr_eq(&beta_before, &beta_after));
        assert_eq!(beta.creates(), 1);
    }

    #[tokio::test]
    async fn invalidating_all_engines_rebuilds_each_on_its_next_use() {
        let factories = [CountingFactory::new("alpha"), CountingFactory::new("beta")];
        let mut reg = TtsRegistry::new();
        for (name, factory) in ["alpha", "beta"].into_iter().zip(&factories) {
            reg.register(id(name), factory.clone());
            engine_of(&reg, name).await;
        }

        reg.invalidate_all();
        for name in ["alpha", "beta"] {
            engine_of(&reg, name).await;
            engine_of(&reg, name).await;
        }

        assert_eq!(factories.map(|f| f.creates()), [2, 2]);
    }

    #[tokio::test]
    async fn a_slot_taken_before_invalidation_keeps_serving_its_engine() {
        let factory = CountingFactory::new("alpha");
        let mut reg = TtsRegistry::new();
        reg.register(id("alpha"), factory.clone());
        let in_flight = reg.slot(&id("alpha")).expect("registered");
        let old = in_flight.engine().await.expect("engine");

        reg.invalidate(&id("alpha"));
        let still_old = in_flight.engine().await.expect("engine");

        assert!(Arc::ptr_eq(&old, &still_old));
        assert_eq!(factory.creates(), 1);
    }

    #[test]
    fn invalidating_an_unregistered_engine_registers_nothing() {
        let mut reg = TtsRegistry::new();

        reg.invalidate(&id("ghost"));

        assert!(reg.slot(&id("ghost")).is_none());
        assert!(reg.engine_ids().is_empty());
    }

    #[tokio::test]
    async fn a_factory_that_panics_reports_its_engine_as_unavailable() {
        let result = build_engine(&id("alpha"), Arc::new(PanickingFactory)).await;

        assert!(
            matches!(&result, Err(TtsError::EngineUnavailable { id, .. }) if id.0 == "alpha"),
            "expected EngineUnavailable for alpha"
        );
    }
}
