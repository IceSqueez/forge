use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use forge_events::{Event, EventPublisher, EventSource};
use forge_storage::GlobalsRepo;
use forge_types::{EventId, ScriptId, Variant};
use rhai::{EvalAltResult, ImmutableString, Module, Position};
use tokio::runtime::Handle;

use crate::convert::{dynamic_to_variant, variant_to_dynamic};
use crate::http_client::{HttpError, HttpResponse, ScriptHttpClient};

/// Async TTS hook exposed to rhai scripts as `forge::tts::*`. The concrete impl
/// lives in `forge-app::speak_bridge` to keep this crate cycle-free with respect
/// to `forge-speak-queue`.
#[async_trait::async_trait]
pub trait SpeakRequester: Send + Sync {
    async fn speak(&self, text: String, voice_id_override: Option<String>);
    async fn skip(&self);
    async fn clear(&self);
}

/// Created once per script execution; `deadline` is the absolute wall-time limit for `forge::sleep`.
pub struct ForgeApi {
    publisher: Arc<dyn EventPublisher>,
    globals: Arc<dyn GlobalsRepo>,
    caused_by: EventId,
    script_id: Option<ScriptId>,
    error_count: Arc<AtomicU32>,
    speak: Option<Arc<dyn SpeakRequester>>,
    http: Option<Arc<ScriptHttpClient>>,
    pub deadline: Instant,
}

impl ForgeApi {
    pub fn new(
        publisher: Arc<dyn EventPublisher>,
        globals: Arc<dyn GlobalsRepo>,
        caused_by: EventId,
        deadline: Instant,
    ) -> Self {
        Self {
            publisher,
            globals,
            caused_by,
            script_id: None,
            error_count: Arc::new(AtomicU32::new(0)),
            speak: None,
            http: None,
            deadline,
        }
    }

    /// Tags every `forge::log/warn/error` bus event this API emits with the owning
    /// script so a single editor console can filter to just its own run.
    pub fn with_script_id(mut self, script_id: ScriptId) -> Self {
        self.script_id = Some(script_id);
        self
    }

    /// Shares the counter incremented by every `forge::error` call so the caller
    /// can read the real error total after the run completes.
    pub fn error_count_handle(&self) -> Arc<AtomicU32> {
        Arc::clone(&self.error_count)
    }

    /// Optional builder — wires the TTS hook so `forge::tts::*` rhai functions become
    /// active. Without this, the `tts` sub-module is registered but empty.
    pub fn with_speak_requester(mut self, requester: Arc<dyn SpeakRequester>) -> Self {
        self.speak = Some(requester);
        self
    }

    pub fn with_http(mut self, client: Arc<ScriptHttpClient>) -> Self {
        self.http = Some(client);
        self
    }

    /// Consumes `self` and builds the full `forge::*` module tree ready for
    /// `Engine::register_static_module("forge", module)`.
    pub fn into_module(self) -> Arc<Module> {
        let mut root = Module::new();

        let caused_by = self.caused_by;
        let script_id_str = self.script_id.map(|id| id.to_string());

        let log_pub = Arc::clone(&self.publisher);
        let log_sid = script_id_str.clone();
        root.set_native_fn(
            "log",
            move |msg: ImmutableString| -> Result<(), Box<EvalAltResult>> {
                tracing::info!(caused_by = %caused_by, message = %msg);
                log_pub.publish(script_log_event(
                    "info",
                    msg.as_str(),
                    caused_by,
                    log_sid.as_deref(),
                ));
                Ok(())
            },
        );
        let warn_pub = Arc::clone(&self.publisher);
        let warn_sid = script_id_str.clone();
        root.set_native_fn(
            "warn",
            move |msg: ImmutableString| -> Result<(), Box<EvalAltResult>> {
                tracing::warn!(caused_by = %caused_by, message = %msg);
                warn_pub.publish(script_log_event(
                    "warn",
                    msg.as_str(),
                    caused_by,
                    warn_sid.as_deref(),
                ));
                Ok(())
            },
        );
        let error_pub = Arc::clone(&self.publisher);
        let error_sid = script_id_str.clone();
        let error_counter = Arc::clone(&self.error_count);
        root.set_native_fn(
            "error",
            move |msg: ImmutableString| -> Result<(), Box<EvalAltResult>> {
                tracing::error!(caused_by = %caused_by, message = %msg);
                error_counter.fetch_add(1, Ordering::Relaxed);
                error_pub.publish(script_log_event(
                    "error",
                    msg.as_str(),
                    caused_by,
                    error_sid.as_deref(),
                ));
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

        let http = match self.http {
            Some(client) => build_http_module(client, Arc::clone(&self.publisher), self.caused_by),
            None => Module::new(),
        };

        let globals = build_globals_module(self.publisher, self.caused_by, self.globals);
        root.set_sub_module("globals", globals);

        root.set_sub_module("audio", Module::new());
        let tts = match self.speak {
            Some(requester) => build_tts_module(requester),
            None => Module::new(),
        };
        root.set_sub_module("tts", tts);
        root.set_sub_module("time", build_time_module());
        root.set_sub_module("obs", Module::new());
        root.set_sub_module("http", http);

        Arc::new(root)
    }
}

/// Builds the observability event emitted by `forge::log/warn/error`. `script_id`
/// is `null` for live action-chain runs (no editor console owns them) and set to
/// the owning script for editor test-runs so the console can filter to its own run.
fn script_log_event(
    level: &str,
    message: &str,
    caused_by: EventId,
    script_id: Option<&str>,
) -> Event {
    Event::caused_by(
        EventSource::Rhai,
        "script.log",
        serde_json::json!({
            "level": level,
            "message": message,
            "script_id": script_id,
        }),
        caused_by,
    )
}

fn build_http_module(
    client: Arc<ScriptHttpClient>,
    publisher: Arc<dyn EventPublisher>,
    caused_by: EventId,
) -> Module {
    let mut m = Module::new();
    let counter = Arc::new(AtomicU32::new(0));

    {
        let client = Arc::clone(&client);
        let counter = Arc::clone(&counter);
        let publisher = Arc::clone(&publisher);
        m.set_native_fn(
            "get",
            move |url: ImmutableString| -> Result<rhai::Map, Box<EvalAltResult>> {
                let result = client.get(url.as_str(), &counter);
                if let Ok(ref resp) = result {
                    publisher.publish(Event::caused_by(
                        EventSource::Rhai,
                        "script.http_call",
                        serde_json::json!({
                            "url_normalized": resp.url_normalized,
                            "status": resp.status,
                            "duration_ms": resp.duration_ms,
                            "truncated": resp.truncated,
                        }),
                        caused_by,
                    ));
                }
                result
                    .map(http_response_to_rhai_map)
                    .map_err(http_error_to_rhai_error)
            },
        );
    }

    {
        let publisher = Arc::clone(&publisher);
        m.set_native_fn(
            "post",
            move |url: ImmutableString,
                  body: ImmutableString|
                  -> Result<rhai::Map, Box<EvalAltResult>> {
                let result = client.post(url.as_str(), body.as_str(), &counter);
                if let Ok(ref resp) = result {
                    publisher.publish(Event::caused_by(
                        EventSource::Rhai,
                        "script.http_call",
                        serde_json::json!({
                            "url_normalized": resp.url_normalized,
                            "status": resp.status,
                            "duration_ms": resp.duration_ms,
                            "truncated": resp.truncated,
                        }),
                        caused_by,
                    ));
                }
                result
                    .map(http_response_to_rhai_map)
                    .map_err(http_error_to_rhai_error)
            },
        );
    }

    m
}

fn http_response_to_rhai_map(resp: HttpResponse) -> rhai::Map {
    let mut map = rhai::Map::new();
    map.insert("status".into(), rhai::Dynamic::from(resp.status as i64));
    map.insert("body".into(), rhai::Dynamic::from(resp.body));
    map.insert("truncated".into(), rhai::Dynamic::from(resp.truncated));
    map.insert(
        "duration_ms".into(),
        rhai::Dynamic::from(resp.duration_ms as i64),
    );
    let mut headers_map = rhai::Map::new();
    for (k, v) in resp.headers {
        headers_map.insert(k.into(), rhai::Dynamic::from(v));
    }
    map.insert("headers".into(), rhai::Dynamic::from(headers_map));
    map
}

fn http_error_to_rhai_error(e: HttpError) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        e.to_string().into(),
        Position::NONE,
    ))
}

fn build_time_module() -> Module {
    use time::format_description::well_known::Rfc3339;
    let mut m = Module::new();

    m.set_native_fn("now", || -> Result<ImmutableString, Box<EvalAltResult>> {
        let now = time::OffsetDateTime::now_utc();
        let s = now
            .format(&Rfc3339)
            .map_err(|e| -> Box<EvalAltResult> { e.to_string().into() })?;
        Ok(s.into())
    });

    m.set_native_fn("unix", || -> Result<i64, Box<EvalAltResult>> {
        Ok(time::OffsetDateTime::now_utc().unix_timestamp())
    });

    m
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
    globals: Arc<dyn GlobalsRepo>,
) -> Module {
    let mut m = Module::new();

    let globals_get = Arc::clone(&globals);
    m.set_native_fn(
        "get",
        move |key: ImmutableString| -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            match Handle::current().block_on(globals_get.get(key.as_str())) {
                Ok(Some(v)) => Ok(variant_to_dynamic(v)),
                Ok(None) => {
                    // Never silent: a script may hold a name that no longer
                    // resolves because the global was renamed or deleted.
                    tracing::warn!(
                        global_name = key.as_str(),
                        "script read an unknown global; it may have been renamed or deleted"
                    );
                    Ok(rhai::Dynamic::UNIT)
                }
                Err(e) => Err(e.to_string().into()),
            }
        },
    );

    let globals_set = Arc::clone(&globals);
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
                .block_on(globals_set.set(key_str, variant, persisted))
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

    let globals_incr = Arc::clone(&globals);
    let pub_incr = Arc::clone(&publisher);
    m.set_native_fn(
        "incr",
        move |key: ImmutableString, amount: i64| -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            let key_str = key.as_str();
            let new_val = Handle::current()
                .block_on(globals_incr.incr(key_str, amount))
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

    let globals_del = globals;
    let pub_del = publisher;
    m.set_native_fn(
        "del",
        move |key: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            let key_str = key.as_str();
            let existed = Handle::current()
                .block_on(globals_del.delete(key_str))
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
    use crate::http_client::new_without_tls_enforcement;
    use crate::http_config::ScriptHttpConfig;
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
            dp as Arc<dyn GlobalsRepo>,
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

    // Build engine with an http client entirely inside a spawn_blocking closure to avoid
    // dropping a reqwest::blocking::Client from within a tokio async context.
    fn build_engine_with_http_in_blocking(
        dp: Arc<SqliteBackend>,
        captured: Arc<Mutex<Vec<Event>>>,
        config: Arc<ScriptHttpConfig>,
    ) -> Engine {
        let caused_by = EventId::new();
        let http_client = Arc::new(new_without_tls_enforcement(config).unwrap());
        let api = ForgeApi::new(
            Arc::new(CapturingPublisher(captured)),
            dp as Arc<dyn GlobalsRepo>,
            caused_by,
            Instant::now() + std::time::Duration::from_secs(10),
        )
        .with_http(http_client);
        Engine::with_api(EngineConfig::default(), api)
    }

    #[tokio::test]
    async fn http_get_registered_under_forge_http_namespace() {
        let dp = open_dp().await;
        let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        // allowlist is empty → DomainNotAllowed fires without any network access
        let config = Arc::new(ScriptHttpConfig::default());

        let result = tokio::task::spawn_blocking(move || {
            let engine = build_engine_with_http_in_blocking(dp, captured, config);
            engine.eval_script(r#"forge::http::get("https://example.com/")"#)
        })
        .await
        .unwrap();

        let err_str = result.unwrap_err().to_string();
        assert!(
            !err_str.contains("not found"),
            "forge::http::get must be registered; got: {err_str}"
        );
        assert!(
            err_str.contains("http:"),
            "error must come from http sandbox; got: {err_str}"
        );
    }

    #[tokio::test]
    async fn http_get_returns_map_with_expected_keys() {
        use wiremock::matchers::any;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_string("pong"))
            .mount(&server)
            .await;

        let server_url = server.uri();
        let parsed = reqwest::Url::parse(&server_url).unwrap();
        let host = parsed.host_str().unwrap().to_string();
        let dp = open_dp().await;
        let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let config = Arc::new(ScriptHttpConfig {
            allowed_domains: vec![host],
            allow_local: true,
            ..ScriptHttpConfig::default()
        });
        let script = format!(r#"let r = forge::http::get("{server_url}/ping"); r"#);

        let result = tokio::task::spawn_blocking(move || {
            let engine = build_engine_with_http_in_blocking(dp, captured, config);
            engine.eval_script(&script)
        })
        .await
        .unwrap()
        .unwrap();

        let map = result.try_cast::<rhai::Map>().unwrap();
        assert!(map.contains_key("status"), "must have status");
        assert!(map.contains_key("body"), "must have body");
        assert!(map.contains_key("headers"), "must have headers");
        assert!(map.contains_key("truncated"), "must have truncated");
        assert!(map.contains_key("duration_ms"), "must have duration_ms");
        assert_eq!(map["status"].clone().as_int().unwrap(), 200);
    }

    #[test]
    fn http_error_display_does_not_contain_url() {
        // Verify HttpError variants that carry sanitized strings do not include URLs.
        // The Network variant uses reqwest's without_url() so only the status/reason appears.
        let no_url = HttpError::Network("connection refused".into());
        let msg = no_url.to_string();
        assert!(
            !msg.contains("http://"),
            "URL scheme must not appear in: {msg}"
        );
        assert!(
            !msg.contains("https://"),
            "URL scheme must not appear in: {msg}"
        );
        assert!(
            !msg.contains("token="),
            "query params must not appear in: {msg}"
        );

        assert_eq!(
            HttpError::DomainNotAllowed.to_string(),
            "http: domain not allowed"
        );
        assert_eq!(HttpError::HttpsRequired.to_string(), "http: HTTPS required");
        assert_eq!(
            HttpError::PrivateAddress.to_string(),
            "http: local addresses blocked"
        );
        assert_eq!(
            HttpError::RateLimitExceeded.to_string(),
            "http: rate limit exceeded"
        );
        assert_eq!(HttpError::Timeout.to_string(), "http: timeout");
    }
}
