#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_events::{Event, EventPublisher};
use forge_script::{
    Engine, EngineConfig, ForgeApi, GlobalDeletion, GlobalIncrement, ScriptGlobals,
    ScriptGlobalsError,
};
use forge_types::{EventId, SCRIPT_LOG_TARGET, Variant};
use tracing::field::{Field, Visit};
use tracing::span;
use tracing::subscriber::Interest;

#[derive(Clone)]
struct CapturedLine {
    target: String,
    fields: BTreeMap<String, String>,
}

#[derive(Default)]
struct FieldCollector(BTreeMap<String, String>);

impl Visit for FieldCollector {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }
}

struct Recorder {
    sink: Option<Arc<Mutex<Vec<CapturedLine>>>>,
}

impl tracing::Subscriber for Recorder {
    fn register_callsite(&self, _: &'static tracing::Metadata<'static>) -> Interest {
        Interest::sometimes()
    }

    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        self.sink.is_some()
    }

    fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
        span::Id::from_u64(1)
    }

    fn record(&self, _: &span::Id, _: &span::Record<'_>) {}

    fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let Some(sink) = self.sink.as_ref() else {
            return;
        };
        let mut collector = FieldCollector::default();
        event.record(&mut collector);
        sink.lock().unwrap().push(CapturedLine {
            target: event.metadata().target().to_owned(),
            fields: collector.0,
        });
    }

    fn enter(&self, _: &span::Id) {}

    fn exit(&self, _: &span::Id) {}
}

fn install_interest_floor() {
    static INSTALLED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    INSTALLED.get_or_init(|| {
        drop(tracing::subscriber::set_global_default(Recorder {
            sink: None,
        }));
        tracing::callsite::rebuild_interest_cache();
    });
}

fn capture(body: impl FnOnce()) -> Vec<CapturedLine> {
    let lines = Arc::new(Mutex::new(Vec::new()));
    install_interest_floor();
    tracing::subscriber::with_default(
        Recorder {
            sink: Some(Arc::clone(&lines)),
        },
        || {
            tracing::callsite::rebuild_interest_cache();
            body();
        },
    );
    lines.lock().unwrap().clone()
}

struct NullPublisher;

impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

struct UnusedGlobals;

fn unused() -> ScriptGlobalsError {
    ScriptGlobalsError("globals are not under test".to_owned())
}

#[async_trait::async_trait]
impl ScriptGlobals for UnusedGlobals {
    async fn get(&self, _key: &str) -> Result<Option<Variant>, ScriptGlobalsError> {
        Err(unused())
    }

    async fn set(
        &self,
        _key: &str,
        _value: Variant,
        _persisted: bool,
        _caused_by: EventId,
    ) -> Result<Option<Variant>, ScriptGlobalsError> {
        Err(unused())
    }

    async fn increment(
        &self,
        _key: &str,
        _delta: i64,
        _caused_by: EventId,
    ) -> Result<GlobalIncrement, ScriptGlobalsError> {
        Err(unused())
    }

    async fn delete(
        &self,
        _key: &str,
        _caused_by: EventId,
    ) -> Result<GlobalDeletion, ScriptGlobalsError> {
        Err(unused())
    }
}

fn engine() -> Engine {
    let api = ForgeApi::new(
        Arc::new(NullPublisher),
        Arc::new(UnusedGlobals),
        EventId::new(),
        Instant::now() + Duration::from_secs(10),
    );
    Engine::with_api(EngineConfig::default(), api)
}

#[tokio::test]
async fn every_script_log_native_emits_on_the_script_target() {
    let engine = engine();

    let lines = capture(|| {
        for native in ["log", "warn", "error"] {
            let _evaluated = engine
                .eval_script(&format!(r#"forge::{native}("marker-{native}")"#))
                .unwrap();
        }
    });

    for native in ["log", "warn", "error"] {
        let marker = format!("marker-{native}");
        let emitted: Vec<&CapturedLine> = lines
            .iter()
            .filter(|line| line.fields.get("message") == Some(&marker))
            .collect();

        assert_eq!(
            emitted.len(),
            1,
            "forge::{native} must emit exactly one line, got {}",
            emitted.len()
        );
        assert_eq!(
            emitted[0].target, SCRIPT_LOG_TARGET,
            "forge::{native} escaped the script section of the diagnostic bundle"
        );
    }
}
