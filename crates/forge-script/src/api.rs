use std::sync::Arc;
use std::time::{Duration, Instant};

use forge_events::{Event, EventPublisher, EventSource};
use forge_storage::{DataProvider, GlobalsRepo};
use forge_types::{EventId, Variant};
use rhai::{EvalAltResult, ImmutableString, Module};
use tokio::runtime::Handle;

use crate::convert::{dynamic_to_variant, variant_to_dynamic};

/// Async TTS hook exposed to rhai scripts as `forge::tts::*`. The concrete impl
/// lives in `forge-app::speak_bridge` to keep this crate cycle-free with respect
/// to `forge-speak-queue`.
#[async_trait::async_trait]
pub trait SpeakRequester: Send + Sync {
    async fn speak(&self, text: String, voice_id_override: Option<String>);
    async fn skip(&self);
    async fn clear(&self);
}

/// The god-object exposed to rhai scripts as the `forge::*` namespace.
///
/// Holds `Arc` clones of the event publisher and storage needed by script-callable
/// methods. Created once per script execution. `deadline` is the absolute wall-clock
/// time at which `forge::sleep` must stop sleeping to respect the wall-time budget.
pub struct ForgeApi {
    publisher: Arc<dyn EventPublisher>,
    dp: Arc<dyn DataProvider>,
    caused_by: EventId,
    speak: Option<Arc<dyn SpeakRequester>>,
    pub deadline: Instant,
}

impl ForgeApi {
    pub fn new(
        publisher: Arc<dyn EventPublisher>,
        dp: Arc<dyn DataProvider>,
        caused_by: EventId,
        deadline: Instant,
    ) -> Self {
        Self {
            publisher,
            dp,
            caused_by,
            speak: None,
            deadline,
        }
    }

    /// Optional builder — wires the TTS hook so `forge::tts::*` rhai functions become
    /// active. Without this, the `tts` sub-module is registered but empty.
    pub fn with_speak_requester(mut self, requester: Arc<dyn SpeakRequester>) -> Self {
        self.speak = Some(requester);
        self
    }

    /// Consumes `self` and builds the full `forge::*` module tree ready for
    /// `Engine::register_static_module("forge", module)`.
    pub fn into_module(self) -> Arc<Module> {
        let mut root = Module::new();

        let caused_by = self.caused_by;
        root.set_native_fn(
            "log",
            move |msg: ImmutableString| -> Result<(), Box<EvalAltResult>> {
                tracing::info!(caused_by = %caused_by, message = %msg);
                Ok(())
            },
        );
        root.set_native_fn(
            "warn",
            move |msg: ImmutableString| -> Result<(), Box<EvalAltResult>> {
                tracing::warn!(caused_by = %caused_by, message = %msg);
                Ok(())
            },
        );
        root.set_native_fn(
            "error",
            move |msg: ImmutableString| -> Result<(), Box<EvalAltResult>> {
                tracing::error!(caused_by = %caused_by, message = %msg);
                Ok(())
            },
        );

        let deadline = self.deadline;
        root.set_native_fn("sleep", move |ms: i64| -> Result<(), Box<EvalAltResult>> {
            let now = Instant::now();
            if now >= deadline {
                return Err("script execution deadline exceeded".into());
            }
            let remaining_ms = (deadline - now).as_millis() as u64;
            let clamped = (ms.max(0) as u64).min(5_000).min(remaining_ms);
            std::thread::sleep(Duration::from_millis(clamped));
            Ok(())
        });

        let chat = build_chat_module(Arc::clone(&self.publisher), self.caused_by);
        root.set_sub_module("chat", chat);

        let globals = build_globals_module(self.publisher, self.caused_by, self.dp);
        root.set_sub_module("globals", globals);

        root.set_sub_module("audio", Module::new());
        let tts = match self.speak {
            Some(requester) => build_tts_module(requester),
            None => Module::new(),
        };
        root.set_sub_module("tts", tts);
        root.set_sub_module("obs", Module::new());
        root.set_sub_module("http", Module::new());

        Arc::new(root)
    }
}

fn build_tts_module(requester: Arc<dyn SpeakRequester>) -> Module {
    let mut m = Module::new();

    let r_speak = Arc::clone(&requester);
    m.set_native_fn(
        "speak",
        move |text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let r = Arc::clone(&r_speak);
            let text_owned = text.to_string();
            Handle::current().block_on(async move { r.speak(text_owned, None).await });
            Ok(())
        },
    );

    let r_speak_as = Arc::clone(&requester);
    m.set_native_fn(
        "speak_as",
        move |voice_id: ImmutableString, text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let r = Arc::clone(&r_speak_as);
            let voice_owned = voice_id.to_string();
            let text_owned = text.to_string();
            Handle::current().block_on(async move { r.speak(text_owned, Some(voice_owned)).await });
            Ok(())
        },
    );

    let r_skip = Arc::clone(&requester);
    m.set_native_fn("skip", move || -> Result<(), Box<EvalAltResult>> {
        let r = Arc::clone(&r_skip);
        Handle::current().block_on(async move { r.skip().await });
        Ok(())
    });

    let r_clear = requester;
    m.set_native_fn("clear", move || -> Result<(), Box<EvalAltResult>> {
        let r = Arc::clone(&r_clear);
        Handle::current().block_on(async move { r.clear().await });
        Ok(())
    });

    m
}

fn build_chat_module(publisher: Arc<dyn EventPublisher>, caused_by: EventId) -> Module {
    let mut m = Module::new();

    let pub_send = Arc::clone(&publisher);
    m.set_native_fn(
        "send",
        move |text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            pub_send.publish(Event::caused_by(
                EventSource::Rhai,
                "chat.send.request",
                serde_json::json!({"message": text.as_str()}),
                caused_by,
            ));
            Ok(())
        },
    );

    let pub_reply = Arc::clone(&publisher);
    m.set_native_fn(
        "reply",
        move |to: ImmutableString, text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            pub_reply.publish(Event::caused_by(
                EventSource::Rhai,
                "chat.send.request",
                serde_json::json!({"message": text.as_str(), "reply_to": to.as_str()}),
                caused_by,
            ));
            Ok(())
        },
    );

    let pub_whisper = publisher;
    m.set_native_fn(
        "whisper",
        move |user: ImmutableString, text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            pub_whisper.publish(Event::caused_by(
                EventSource::Rhai,
                "chat.send.request",
                serde_json::json!({"message": text.as_str(), "whisper_to": user.as_str()}),
                caused_by,
            ));
            Ok(())
        },
    );

    m
}

fn build_globals_module(
    publisher: Arc<dyn EventPublisher>,
    caused_by: EventId,
    dp: Arc<dyn DataProvider>,
) -> Module {
    let mut m = Module::new();

    let dp_get = Arc::clone(&dp);
    m.set_native_fn(
        "get",
        move |key: ImmutableString| -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            match Handle::current().block_on(GlobalsRepo::get(dp_get.as_ref(), key.as_str())) {
                Ok(Some(v)) => Ok(variant_to_dynamic(v)),
                Ok(None) => Ok(rhai::Dynamic::UNIT),
                Err(e) => Err(e.to_string().into()),
            }
        },
    );

    let dp_set = Arc::clone(&dp);
    let pub_set = Arc::clone(&publisher);
    m.set_native_fn(
        "set",
        move |key: ImmutableString,
              val: rhai::Dynamic,
              persisted: bool|
              -> Result<(), Box<EvalAltResult>> {
            let key_str = key.as_str();
            let variant =
                dynamic_to_variant(val).map_err(|e| -> Box<EvalAltResult> { e.into() })?;
            let new_value_str = variant.to_string();
            Handle::current()
                .block_on(GlobalsRepo::set(
                    dp_set.as_ref(),
                    key_str,
                    variant,
                    persisted,
                ))
                .map_err(|e| -> Box<EvalAltResult> { e.to_string().into() })?;
            pub_set.publish(Event::caused_by(
                EventSource::Core,
                "global.set",
                serde_json::json!({ "key": key_str, "new_value": new_value_str }),
                caused_by,
            ));
            Ok(())
        },
    );

    let dp_incr = Arc::clone(&dp);
    let pub_incr = Arc::clone(&publisher);
    m.set_native_fn(
        "incr",
        move |key: ImmutableString, amount: i64| -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            let key_str = key.as_str();
            let new_val = Handle::current()
                .block_on(GlobalsRepo::incr(dp_incr.as_ref(), key_str, amount))
                .map_err(|e| -> Box<EvalAltResult> { e.to_string().into() })?;
            let new_val_json = match &new_val {
                Variant::Int(i) => serde_json::Value::from(*i),
                _ => serde_json::Value::String(new_val.to_string()),
            };
            pub_incr.publish(Event::caused_by(
                EventSource::Core,
                "global.incr",
                serde_json::json!({ "key": key_str, "delta": amount, "new_value": new_val_json }),
                caused_by,
            ));
            Ok(variant_to_dynamic(new_val))
        },
    );

    let dp_del = dp;
    let pub_del = publisher;
    m.set_native_fn(
        "del",
        move |key: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            let key_str = key.as_str();
            let existed = Handle::current()
                .block_on(GlobalsRepo::delete(dp_del.as_ref(), key_str))
                .map_err(|e| -> Box<EvalAltResult> { e.to_string().into() })?;
            pub_del.publish(Event::caused_by(
                EventSource::Core,
                "global.del",
                serde_json::json!({ "key": key_str }),
                caused_by,
            ));
            Ok(existed)
        },
    );

    m
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::engine::{Engine, EngineConfig};
    use forge_events::Event;
    use forge_storage::GlobalsRepo;
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{EventId, Variant};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    struct CapturingPublisher(Arc<Mutex<Vec<Event>>>);

    impl EventPublisher for CapturingPublisher {
        fn publish(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    async fn open_dp() -> Arc<SqliteBackend> {
        Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        )
    }

    fn make_api_with_publisher(
        dp: Arc<SqliteBackend>,
        captured: Arc<Mutex<Vec<Event>>>,
    ) -> (ForgeApi, EventId) {
        let caused_by = EventId::new();
        let api = ForgeApi::new(
            Arc::new(CapturingPublisher(captured)),
            dp,
            caused_by,
            Instant::now() + std::time::Duration::from_secs(10),
        );
        (api, caused_by)
    }

    #[tokio::test]
    async fn forge_globals_set_emits_global_set_event() {
        let dp = open_dp().await;
        let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let (api, caused_by) = make_api_with_publisher(Arc::clone(&dp), Arc::clone(&captured));
        let engine = Engine::with_api(EngineConfig::default(), api);

        tokio::task::spawn_blocking(move || {
            let _ = engine
                .eval_script(r#"forge::globals::set("score", 77, false)"#)
                .unwrap();
        })
        .await
        .unwrap();

        let events = captured.lock().unwrap();
        assert!(
            events.iter().any(|e| e.kind == "global.set"),
            "global.set must be emitted"
        );
        let ev = events.iter().find(|e| e.kind == "global.set").unwrap();
        assert_eq!(ev.caused_by, Some(caused_by));
        assert_eq!(ev.payload["key"].as_str(), Some("score"));
        assert_eq!(ev.payload["new_value"].as_str(), Some("77"));
    }

    #[tokio::test]
    async fn forge_globals_incr_emits_global_incr_event() {
        let dp = open_dp().await;
        GlobalsRepo::set(dp.as_ref(), "hits", Variant::Int(10), false)
            .await
            .unwrap();

        let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let (api, caused_by) = make_api_with_publisher(Arc::clone(&dp), Arc::clone(&captured));
        let engine = Engine::with_api(EngineConfig::default(), api);

        tokio::task::spawn_blocking(move || {
            let _ = engine
                .eval_script(r#"forge::globals::incr("hits", 3)"#)
                .unwrap();
        })
        .await
        .unwrap();

        let events = captured.lock().unwrap();
        assert!(
            events.iter().any(|e| e.kind == "global.incr"),
            "global.incr must be emitted"
        );
        let ev = events.iter().find(|e| e.kind == "global.incr").unwrap();
        assert_eq!(ev.caused_by, Some(caused_by));
        assert_eq!(ev.payload["key"].as_str(), Some("hits"));
        assert_eq!(ev.payload["delta"].as_i64(), Some(3));
        assert_eq!(ev.payload["new_value"].as_i64(), Some(13));
    }

    #[tokio::test]
    async fn forge_globals_del_emits_global_del_event() {
        let dp = open_dp().await;
        GlobalsRepo::set(dp.as_ref(), "temp", Variant::Int(1), false)
            .await
            .unwrap();

        let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let (api, caused_by) = make_api_with_publisher(Arc::clone(&dp), Arc::clone(&captured));
        let engine = Engine::with_api(EngineConfig::default(), api);

        tokio::task::spawn_blocking(move || {
            let _ = engine
                .eval_script(r#"forge::globals::del("temp")"#)
                .unwrap();
        })
        .await
        .unwrap();

        let events = captured.lock().unwrap();
        assert!(
            events.iter().any(|e| e.kind == "global.del"),
            "global.del must be emitted"
        );
        let ev = events.iter().find(|e| e.kind == "global.del").unwrap();
        assert_eq!(ev.caused_by, Some(caused_by));
        assert_eq!(ev.payload["key"].as_str(), Some("temp"));
    }
}
