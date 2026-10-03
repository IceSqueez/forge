use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use forge_events::{Event, EventPublisher, EventSource};
use forge_storage::GlobalsRepo;
use forge_types::{
    EventId, IntegrationAvailability, IntegrationId, LatestScope, LatestValueReader,
    NO_CHAT_PLATFORM_ENABLED_REASON, NO_WHISPER_PLATFORM_ENABLED_REASON, PlatformId,
    REPLY_PARENT_FIELD, SCRIPT_LOG_TARGET, ScriptId, Variant, WHISPER_RECIPIENT_FIELD,
    integration_disabled_reason, requested_chat_target, whispers_unsupported_reason,
};
use rhai::{EvalAltResult, ImmutableString, Module, Position};
use tokio::runtime::Handle;

use crate::convert::{dynamic_to_variant, variant_to_dynamic};
use crate::http_client::{HttpError, HttpResponse, ScriptHttpClient};

pub const ENGINE_BOUND_NAMES: [&str; 12] = [
    "log", "warn", "error", "sleep", "chat", "globals", "audio", "http", "tts", "time", "obs",
    "latest",
];

pub fn is_engine_bound_name(name: &str) -> bool {
    ENGINE_BOUND_NAMES.contains(&name)
}

#[async_trait::async_trait]
pub trait SpeakRequester: Send + Sync {
    async fn speak(&self, text: String, voice_id_override: Option<String>);
    async fn skip(&self);
    async fn clear(&self);
}

pub struct ForgeApi {
    publisher: Arc<dyn EventPublisher>,
    globals: Arc<dyn GlobalsRepo>,
    caused_by: EventId,
    script_id: Option<ScriptId>,
    error_count: Arc<AtomicU32>,
    speak: Option<Arc<dyn SpeakRequester>>,
    http: Option<Arc<ScriptHttpClient>>,
    integrations: Option<Arc<dyn IntegrationAvailability>>,
    latest: Option<Arc<dyn LatestValueReader>>,
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
            integrations: None,
            latest: None,
            deadline,
        }
    }

    pub fn with_script_id(mut self, script_id: ScriptId) -> Self {
        self.script_id = Some(script_id);
        self
    }

    pub fn error_count_handle(&self) -> Arc<AtomicU32> {
        Arc::clone(&self.error_count)
    }

    pub fn with_speak_requester(mut self, requester: Arc<dyn SpeakRequester>) -> Self {
        self.speak = Some(requester);
        self
    }

    pub fn with_http(mut self, client: Arc<ScriptHttpClient>) -> Self {
        self.http = Some(client);
        self
    }

    pub fn with_integration_availability(
        mut self,
        integrations: Arc<dyn IntegrationAvailability>,
    ) -> Self {
        self.integrations = Some(integrations);
        self
    }

    pub fn with_latest_values(mut self, latest: Arc<dyn LatestValueReader>) -> Self {
        self.latest = Some(latest);
        self
    }

    pub fn into_module(self) -> Arc<Module> {
        let mut root = Module::new();

        let caused_by = self.caused_by;
        let script_id_str = self.script_id.map(|id| id.to_string());

        let log_pub = Arc::clone(&self.publisher);
        let log_sid = script_id_str.clone();
        root.set_native_fn(
            "log",
            move |msg: ImmutableString| -> Result<(), Box<EvalAltResult>> {
                tracing::info!(target: SCRIPT_LOG_TARGET, caused_by = %caused_by, message = %msg);
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
                tracing::warn!(target: SCRIPT_LOG_TARGET, caused_by = %caused_by, message = %msg);
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
                tracing::error!(target: SCRIPT_LOG_TARGET, caused_by = %caused_by, message = %msg);
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

        let chat = build_chat_module(
            Arc::clone(&self.publisher),
            self.caused_by,
            self.integrations,
        );
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
        let latest = match self.latest {
            Some(reader) => build_latest_module(reader),
            None => Module::new(),
        };
        root.set_sub_module("latest", latest);

        Arc::new(root)
    }
}

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
                            "method": "get",
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
                            "method": "post",
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

fn build_latest_module(reader: Arc<dyn LatestValueReader>) -> Module {
    let mut m = Module::new();

    let merged = Arc::clone(&reader);
    m.set_native_fn(
        "get",
        move |slot: ImmutableString| -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            Ok(latest_dynamic(
                merged.as_ref(),
                slot.as_str(),
                LatestScope::MostRecentAcrossPlatforms,
            ))
        },
    );

    m.set_native_fn(
        "get",
        move |slot: ImmutableString,
              platform: ImmutableString|
              -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            Ok(latest_dynamic(
                reader.as_ref(),
                slot.as_str(),
                LatestScope::from_platform_filter(platform.as_str()),
            ))
        },
    );

    m
}

fn latest_dynamic(
    reader: &dyn LatestValueReader,
    slot: &str,
    scope: LatestScope<'_>,
) -> rhai::Dynamic {
    reader
        .latest(slot, scope)
        .map(|value| variant_to_dynamic(value.to_variant()))
        .unwrap_or(rhai::Dynamic::UNIT)
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

fn admit_broadcast(
    integrations: Option<&dyn IntegrationAvailability>,
) -> Result<(), Box<EvalAltResult>> {
    match integrations {
        Some(integrations) if !integrations.any_chat_platform_enabled() => {
            Err(NO_CHAT_PLATFORM_ENABLED_REASON.into())
        }
        _ => Ok(()),
    }
}

fn admit_chat_target<'a>(
    integrations: Option<&dyn IntegrationAvailability>,
    raw: &'a str,
) -> Result<Option<&'a str>, Box<EvalAltResult>> {
    let Some(target) = requested_chat_target(raw) else {
        admit_broadcast(integrations)?;
        return Ok(None);
    };
    let integration = IntegrationId::new(target);
    if integrations.is_some_and(|integrations| integrations.is_disabled(&integration)) {
        return Err(integration_disabled_reason(&integration).into());
    }
    Ok(Some(target))
}

fn admit_whisper_broadcast(
    integrations: Option<&dyn IntegrationAvailability>,
) -> Result<(), Box<EvalAltResult>> {
    admit_broadcast(integrations)?;
    match integrations {
        Some(integrations) if !integrations.any_whisper_platform_enabled() => {
            Err(NO_WHISPER_PLATFORM_ENABLED_REASON.into())
        }
        _ => Ok(()),
    }
}

fn admit_whisper_target<'a>(
    integrations: Option<&dyn IntegrationAvailability>,
    raw: &'a str,
) -> Result<Option<&'a str>, Box<EvalAltResult>> {
    let Some(target) = requested_chat_target(raw) else {
        admit_whisper_broadcast(integrations)?;
        return Ok(None);
    };
    if !PlatformId::from_wire(target).is_some_and(PlatformId::supports_whispers) {
        return Err(whispers_unsupported_reason(target).into());
    }
    admit_chat_target(integrations, target)
}

fn chat_request_payload(target: Option<&str>, mut payload: serde_json::Value) -> serde_json::Value {
    if let (Some(target), Some(fields)) = (target, payload.as_object_mut()) {
        fields.insert("target".to_owned(), target.into());
    }
    payload
}

fn build_chat_module(
    publisher: Arc<dyn EventPublisher>,
    caused_by: EventId,
    integrations: Option<Arc<dyn IntegrationAvailability>>,
) -> Module {
    let mut m = Module::new();

    let pub_send = Arc::clone(&publisher);
    let broadcast_send_integrations = integrations.clone();
    m.set_native_fn(
        "send",
        move |text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            admit_broadcast(broadcast_send_integrations.as_deref())?;
            pub_send.publish(Event::caused_by(
                EventSource::Rhai,
                "chat.send.request",
                serde_json::json!({"message": text.as_str()}),
                caused_by,
            ));
            Ok(())
        },
    );

    let pub_send_targeted = Arc::clone(&publisher);
    let send_integrations = integrations.clone();
    m.set_native_fn(
        "send",
        move |target: ImmutableString, text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let target = admit_chat_target(send_integrations.as_deref(), target.as_str())?;
            pub_send_targeted.publish(Event::caused_by(
                EventSource::Rhai,
                "chat.send.request",
                chat_request_payload(target, serde_json::json!({"message": text.as_str()})),
                caused_by,
            ));
            Ok(())
        },
    );

    let pub_reply = Arc::clone(&publisher);
    let broadcast_reply_integrations = integrations.clone();
    m.set_native_fn(
        "reply",
        move |to: ImmutableString, text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            admit_broadcast(broadcast_reply_integrations.as_deref())?;
            pub_reply.publish(Event::caused_by(
                EventSource::Rhai,
                "chat.send.request",
                serde_json::json!({"message": text.as_str(), REPLY_PARENT_FIELD: to.as_str()}),
                caused_by,
            ));
            Ok(())
        },
    );

    let pub_reply_targeted = Arc::clone(&publisher);
    let reply_integrations = integrations.clone();
    m.set_native_fn(
        "reply",
        move |target: ImmutableString,
              to: ImmutableString,
              text: ImmutableString|
              -> Result<(), Box<EvalAltResult>> {
            let target = admit_chat_target(reply_integrations.as_deref(), target.as_str())?;
            pub_reply_targeted.publish(Event::caused_by(
                EventSource::Rhai,
                "chat.send.request",
                chat_request_payload(
                    target,
                    serde_json::json!({"message": text.as_str(), REPLY_PARENT_FIELD: to.as_str()}),
                ),
                caused_by,
            ));
            Ok(())
        },
    );

    let pub_whisper = Arc::clone(&publisher);
    let broadcast_whisper_integrations = integrations.clone();
    m.set_native_fn(
        "whisper",
        move |user: ImmutableString, text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            admit_whisper_broadcast(broadcast_whisper_integrations.as_deref())?;
            pub_whisper.publish(Event::caused_by(
                EventSource::Rhai,
                "chat.send.request",
                serde_json::json!({"message": text.as_str(), WHISPER_RECIPIENT_FIELD: user.as_str()}),
                caused_by,
            ));
            Ok(())
        },
    );

    let pub_whisper_targeted = publisher;
    m.set_native_fn(
        "whisper",
        move |target: ImmutableString,
              user: ImmutableString,
              text: ImmutableString|
              -> Result<(), Box<EvalAltResult>> {
            let target = admit_whisper_target(integrations.as_deref(), target.as_str())?;
            pub_whisper_targeted.publish(Event::caused_by(
                EventSource::Rhai,
                "chat.send.request",
                chat_request_payload(
                    target,
                    serde_json::json!({"message": text.as_str(), WHISPER_RECIPIENT_FIELD: user.as_str()}),
                ),
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
            let new_value_json = variant.to_plain_json();
            Handle::current()
                .block_on(globals_set.set(key_str, variant, persisted))
                .map_err(|e| -> Box<EvalAltResult> { e.to_string().into() })?;
            pub_set.publish(Event::caused_by(
                EventSource::Core,
                "global.set",
                serde_json::json!({
                    "key": key_str,
                    "new_value": new_value_json,
                    "prev_value": serde_json::Value::Null,
                }),
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
                "global.incremented",
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
                "global.deleted",
                serde_json::json!({ "key": key_str }),
                caused_by,
            ));
            Ok(existed)
        },
    );

    m
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::engine::{Engine, EngineConfig};
    use crate::http_client::new_without_tls_enforcement;
    use crate::http_config::ScriptHttpConfig;
    use forge_events::Event;
    use forge_storage::GlobalsRepo;
    use forge_storage_sqlite::SqliteBackend;

    use crate::test_support::{Sandboxed, sandboxed_backend};
    use forge_types::{EventId, Variant};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    struct CapturingPublisher(Arc<Mutex<Vec<Event>>>);

    impl EventPublisher for CapturingPublisher {
        fn publish(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    async fn open_dp() -> Sandboxed<Arc<SqliteBackend>> {
        sandboxed_backend([0xab; 32]).await.map(Arc::new)
    }

    fn make_api_with_publisher(
        dp: Arc<SqliteBackend>,
        captured: Arc<Mutex<Vec<Event>>>,
    ) -> (ForgeApi, EventId) {
        let caused_by = EventId::new();
        let api = ForgeApi::new(
            Arc::new(CapturingPublisher(captured)),
            dp.clone() as Arc<dyn GlobalsRepo>,
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
        assert_eq!(ev.payload["new_value"].as_i64(), Some(77));
        assert!(ev.payload["prev_value"].is_null());
    }

    #[tokio::test]
    async fn forge_globals_incr_emits_global_incremented_event() {
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
            events.iter().any(|e| e.kind == "global.incremented"),
            "global.incremented must be emitted"
        );
        let ev = events
            .iter()
            .find(|e| e.kind == "global.incremented")
            .unwrap();
        assert_eq!(ev.caused_by, Some(caused_by));
        assert_eq!(ev.payload["key"].as_str(), Some("hits"));
        assert_eq!(ev.payload["delta"].as_i64(), Some(3));
        assert_eq!(ev.payload["new_value"].as_i64(), Some(13));
    }

    #[tokio::test]
    async fn forge_globals_del_emits_global_deleted_event() {
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
            events.iter().any(|e| e.kind == "global.deleted"),
            "global.deleted must be emitted"
        );
        let ev = events.iter().find(|e| e.kind == "global.deleted").unwrap();
        assert_eq!(ev.caused_by, Some(caused_by));
        assert_eq!(ev.payload["key"].as_str(), Some("temp"));
    }

    fn build_engine_with_http_in_blocking(
        dp: Arc<SqliteBackend>,
        captured: Arc<Mutex<Vec<Event>>>,
        config: Arc<ScriptHttpConfig>,
    ) -> Engine {
        let caused_by = EventId::new();
        let http_client = Arc::new(new_without_tls_enforcement(config).unwrap());
        let api = ForgeApi::new(
            Arc::new(CapturingPublisher(captured)),
            dp.clone() as Arc<dyn GlobalsRepo>,
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
        let config = Arc::new(ScriptHttpConfig::default());

        let result = tokio::task::spawn_blocking(move || {
            let engine = build_engine_with_http_in_blocking(dp.clone(), captured, config);
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
            let engine = build_engine_with_http_in_blocking(dp.clone(), captured, config);
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

    #[tokio::test]
    async fn forge_log_warn_error_emit_script_log_events_with_matching_level_and_script_id() {
        for (call, level) in [
            (r#"forge::log("hi")"#, "info"),
            (r#"forge::warn("hi")"#, "warn"),
            (r#"forge::error("hi")"#, "error"),
        ] {
            let dp = open_dp().await;
            let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
            let caused_by = EventId::new();
            let script_id = ScriptId::new();
            let api = ForgeApi::new(
                Arc::new(CapturingPublisher(Arc::clone(&captured))),
                dp.clone() as Arc<dyn GlobalsRepo>,
                caused_by,
                Instant::now() + std::time::Duration::from_secs(10),
            )
            .with_script_id(script_id);
            let engine = Engine::with_api(EngineConfig::default(), api);

            tokio::task::spawn_blocking(move || {
                let _ = engine.eval_script(call).unwrap();
            })
            .await
            .unwrap();

            let events = captured.lock().unwrap();
            let logs: Vec<&Event> = events.iter().filter(|e| e.kind == "script.log").collect();
            assert_eq!(logs.len(), 1, "exactly one script.log for {call}");
            let ev = logs[0];
            assert_eq!(
                ev.payload["level"].as_str(),
                Some(level),
                "level for {call}"
            );
            assert_eq!(
                ev.payload["message"].as_str(),
                Some("hi"),
                "message for {call}"
            );
            assert_eq!(
                ev.payload["script_id"].as_str(),
                Some(script_id.to_string().as_str()),
                "script_id tag for {call}",
            );
            assert_eq!(ev.caused_by, Some(caused_by));
        }
    }

    #[tokio::test]
    async fn error_count_reflects_only_forge_error_calls() {
        let dp = open_dp().await;
        let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let caused_by = EventId::new();
        let api = ForgeApi::new(
            Arc::new(CapturingPublisher(captured)),
            dp.clone() as Arc<dyn GlobalsRepo>,
            caused_by,
            Instant::now() + std::time::Duration::from_secs(10),
        );
        let counter = api.error_count_handle();
        let engine = Engine::with_api(EngineConfig::default(), api);

        tokio::task::spawn_blocking(move || {
            let _ = engine
                .eval_script(
                    r#"forge::log("a"); forge::warn("b"); forge::error("c"); forge::error("d");"#,
                )
                .unwrap();
        })
        .await
        .unwrap();

        assert_eq!(
            counter.load(Ordering::Relaxed),
            2,
            "only the two forge::error calls count; log and warn do not",
        );
    }

    #[tokio::test]
    async fn script_log_script_id_is_null_when_api_has_no_script_id() {
        let dp = open_dp().await;
        let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let (api, _caused_by) = make_api_with_publisher(Arc::clone(&dp), Arc::clone(&captured));
        let engine = Engine::with_api(EngineConfig::default(), api);

        tokio::task::spawn_blocking(move || {
            let _ = engine.eval_script(r#"forge::log("x")"#).unwrap();
        })
        .await
        .unwrap();

        let events = captured.lock().unwrap();
        let ev = events.iter().find(|e| e.kind == "script.log").unwrap();
        assert!(
            ev.payload["script_id"].is_null(),
            "a run with no owning script must emit a null script_id",
        );
    }

    #[tokio::test]
    async fn http_call_event_tags_the_request_method() {
        use wiremock::matchers::any;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;
        let server_url = server.uri();
        let host = reqwest::Url::parse(&server_url)
            .unwrap()
            .host_str()
            .unwrap()
            .to_string();

        for (script, expected_method) in [
            (format!(r#"forge::http::get("{server_url}/p")"#), "get"),
            (
                format!(r#"forge::http::post("{server_url}/p", "payload")"#),
                "post",
            ),
        ] {
            let dp = open_dp().await;
            let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
            let config = Arc::new(ScriptHttpConfig {
                allowed_domains: vec![host.clone()],
                allow_local: true,
                ..ScriptHttpConfig::default()
            });
            let cap = Arc::clone(&captured);
            tokio::task::spawn_blocking(move || {
                let engine = build_engine_with_http_in_blocking(dp.clone(), cap, config);
                let _ = engine.eval_script(&script);
            })
            .await
            .unwrap();

            let events = captured.lock().unwrap();
            let ev = events
                .iter()
                .find(|e| e.kind == "script.http_call")
                .unwrap_or_else(|| {
                    panic!("script.http_call must be emitted for {expected_method}")
                });
            assert_eq!(
                ev.payload["method"].as_str(),
                Some(expected_method),
                "http_call must tag method for {expected_method}"
            );
        }
    }

    #[tokio::test]
    async fn chat_broadcast_arities_omit_target_and_tag_rhai_source() {
        for (call, recipient_key, recipient, message) in [
            (r#"forge::chat::send("hello")"#, None, None, "hello"),
            (
                r#"forge::chat::reply("msg-1", "hello")"#,
                Some("reply_to_message_id"),
                Some("msg-1"),
                "hello",
            ),
            (
                r#"forge::chat::whisper("viewer", "hello")"#,
                Some("whisper_to_login"),
                Some("viewer"),
                "hello",
            ),
        ] {
            let dp = open_dp().await;
            let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
            let (api, caused_by) = make_api_with_publisher(Arc::clone(&dp), Arc::clone(&captured));
            let engine = Engine::with_api(EngineConfig::default(), api);

            tokio::task::spawn_blocking(move || {
                let _ = engine.eval_script(call).unwrap();
            })
            .await
            .unwrap();

            let events = captured.lock().unwrap();
            let ev = events
                .iter()
                .find(|e| e.kind == "chat.send.request")
                .unwrap_or_else(|| panic!("chat.send.request must be emitted for {call}"));
            assert_eq!(ev.source, EventSource::Rhai, "source for {call}");
            assert!(
                ev.payload.get("target").is_none(),
                "broadcast arity must omit the target key entirely for {call}"
            );
            assert_eq!(
                ev.payload["message"].as_str(),
                Some(message),
                "message for {call}"
            );
            if let Some(key) = recipient_key {
                assert_eq!(
                    ev.payload[key].as_str(),
                    recipient,
                    "{call} must carry canonical recipient key"
                );
            }
            assert_eq!(ev.caused_by, Some(caused_by));
        }
    }

    #[tokio::test]
    async fn chat_targeted_arities_include_target_and_recipient_keys() {
        for (call, target, recipient_key, recipient, message) in [
            (
                r#"forge::chat::send("twitch", "hello")"#,
                "twitch",
                None,
                None,
                "hello",
            ),
            (
                r#"forge::chat::reply("kick", "msg-1", "hello")"#,
                "kick",
                Some("reply_to_message_id"),
                Some("msg-1"),
                "hello",
            ),
            (
                r#"forge::chat::whisper("twitch", "viewer", "hello")"#,
                "twitch",
                Some("whisper_to_login"),
                Some("viewer"),
                "hello",
            ),
        ] {
            let dp = open_dp().await;
            let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
            let (api, caused_by) = make_api_with_publisher(Arc::clone(&dp), Arc::clone(&captured));
            let engine = Engine::with_api(EngineConfig::default(), api);

            tokio::task::spawn_blocking(move || {
                let _ = engine.eval_script(call).unwrap();
            })
            .await
            .unwrap();

            let events = captured.lock().unwrap();
            let ev = events
                .iter()
                .find(|e| e.kind == "chat.send.request")
                .unwrap_or_else(|| panic!("chat.send.request must be emitted for {call}"));
            assert_eq!(ev.source, EventSource::Rhai, "source for {call}");
            assert_eq!(
                ev.payload["target"].as_str(),
                Some(target),
                "targeted arity must carry the target for {call}"
            );
            assert_eq!(
                ev.payload["message"].as_str(),
                Some(message),
                "message for {call}"
            );
            if let Some(key) = recipient_key {
                assert_eq!(
                    ev.payload[key].as_str(),
                    recipient,
                    "{call} must carry canonical recipient key"
                );
            }
            assert_eq!(ev.caused_by, Some(caused_by));
        }
    }

    #[derive(Clone, Default)]
    struct SwitchableAvailability(Arc<Mutex<std::collections::HashSet<IntegrationId>>>);

    impl SwitchableAvailability {
        fn disabling(id: &'static str) -> Self {
            let this = Self::default();
            this.set_disabled(id, true);
            this
        }

        fn set_disabled(&self, id: &'static str, disabled: bool) {
            let mut set = self.0.lock().unwrap();
            if disabled {
                set.insert(IntegrationId::from_static(id));
            } else {
                set.remove(&IntegrationId::from_static(id));
            }
        }
    }

    impl IntegrationAvailability for SwitchableAvailability {
        fn is_disabled(&self, integration: &IntegrationId) -> bool {
            self.0.lock().unwrap().contains(integration)
        }
    }

    async fn gated_engine(
        availability: SwitchableAvailability,
        captured: Arc<Mutex<Vec<Event>>>,
    ) -> (Engine, Sandboxed<Arc<SqliteBackend>>) {
        let dp = open_dp().await;
        let (api, _) = make_api_with_publisher(Arc::clone(&dp), captured);
        let api = api.with_integration_availability(Arc::new(availability));
        (Engine::with_api(EngineConfig::default(), api), dp)
    }

    fn sent_requests(captured: &Mutex<Vec<Event>>) -> usize {
        captured
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.kind == "chat.send.request")
            .count()
    }

    #[tokio::test]
    async fn targeted_chat_calls_to_a_disabled_integration_fail_naming_it_and_publish_nothing() {
        for call in [
            r#"forge::chat::send("twitch", "hello")"#,
            r#"forge::chat::reply("twitch", "msg-1", "hello")"#,
            r#"forge::chat::whisper("twitch", "viewer", "hello")"#,
        ] {
            let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
            let (engine, _dp) = gated_engine(
                SwitchableAvailability::disabling("twitch"),
                Arc::clone(&captured),
            )
            .await;

            let result = tokio::task::spawn_blocking(move || engine.eval_script(call))
                .await
                .unwrap();

            match result {
                Err(crate::ScriptError::Runtime { reason, .. }) => assert!(
                    reason.contains("integration disabled: twitch"),
                    "{call} must fail naming the disabled integration, got: {reason}"
                ),
                other => panic!("{call} must fail as a runtime error, got {other:?}"),
            }
            assert_eq!(sent_requests(&captured), 0, "{call} must publish nothing");
        }
    }

    #[tokio::test]
    async fn targeted_chat_send_to_an_enabled_integration_publishes_while_another_is_disabled() {
        let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let (engine, _dp) = gated_engine(
            SwitchableAvailability::disabling("kick"),
            Arc::clone(&captured),
        )
        .await;

        let result = tokio::task::spawn_blocking(move || {
            engine.eval_script(r#"forge::chat::send("twitch", "hello")"#)
        })
        .await
        .unwrap();

        assert!(
            result.is_ok(),
            "send to an enabled integration failed: {result:?}"
        );
        assert_eq!(sent_requests(&captured), 1);
    }

    #[tokio::test]
    async fn re_enabling_an_integration_lets_the_same_engine_send_to_it_again() {
        let availability = SwitchableAvailability::disabling("twitch");
        let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let (engine, _dp) = gated_engine(availability.clone(), Arc::clone(&captured)).await;
        let engine = Arc::new(engine);
        let send = r#"forge::chat::send("twitch", "hello")"#;

        let first = {
            let engine = Arc::clone(&engine);
            tokio::task::spawn_blocking(move || engine.eval_script(send).is_ok())
                .await
                .unwrap()
        };
        availability.set_disabled("twitch", false);
        let second = tokio::task::spawn_blocking(move || engine.eval_script(send).is_ok())
            .await
            .unwrap();

        assert!(!first, "the send while disabled must fail");
        assert!(second, "the send after re-enabling must succeed");
        assert_eq!(sent_requests(&captured), 1);
    }

    const UNTARGETED_CHAT_CALLS: [&str; 7] = [
        r#"forge::chat::send("hello")"#,
        r#"forge::chat::reply("msg-1", "hello")"#,
        r#"forge::chat::whisper("viewer", "hello")"#,
        r#"forge::chat::send("", "hello")"#,
        r#"forge::chat::send("  ", "hello")"#,
        r#"forge::chat::reply(" ", "msg-1", "hello")"#,
        r#"forge::chat::whisper("\t", "viewer", "hello")"#,
    ];

    fn availability_disabling(ids: &[&'static str]) -> SwitchableAvailability {
        let availability = SwitchableAvailability::default();
        for id in ids {
            availability.set_disabled(id, true);
        }
        availability
    }

    async fn eval_gated(
        availability: SwitchableAvailability,
        call: &'static str,
    ) -> (Result<rhai::Dynamic, crate::ScriptError>, Vec<Event>) {
        let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let (engine, _dp) = gated_engine(availability, Arc::clone(&captured)).await;
        let result = tokio::task::spawn_blocking(move || engine.eval_script(call))
            .await
            .unwrap();
        let sent = captured
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.kind == "chat.send.request")
            .cloned()
            .collect();
        (result, sent)
    }

    #[tokio::test]
    async fn untargeted_chat_calls_fail_unsent_when_every_chat_platform_is_disabled() {
        for call in UNTARGETED_CHAT_CALLS {
            let (result, sent) =
                eval_gated(availability_disabling(&["twitch", "youtube", "kick"]), call).await;

            match result {
                Err(crate::ScriptError::Runtime { reason, .. }) => assert!(
                    reason.contains(NO_CHAT_PLATFORM_ENABLED_REASON),
                    "{call} failed for the wrong reason: {reason}"
                ),
                other => panic!("{call} must fail as a runtime error, got {other:?}"),
            }
            assert!(sent.is_empty(), "{call} published {sent:?}");
        }
    }

    const UNTARGETED_WHISPER_CALLS: [&str; 2] = [
        r#"forge::chat::whisper("viewer", "hello")"#,
        r#"forge::chat::whisper("\t", "viewer", "hello")"#,
    ];

    fn assert_runtime_error_mentions(
        call: &str,
        result: Result<rhai::Dynamic, crate::ScriptError>,
        expected: &str,
    ) {
        match result {
            Err(crate::ScriptError::Runtime { reason, .. }) => assert!(
                reason.contains(expected),
                "{call} failed for the wrong reason: {reason}"
            ),
            other => panic!("{call} must fail as a runtime error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn untargeted_public_chat_calls_broadcast_without_a_target_while_one_chat_platform_is_enabled()
     {
        for call in UNTARGETED_CHAT_CALLS
            .into_iter()
            .filter(|call| !UNTARGETED_WHISPER_CALLS.contains(call))
        {
            let (result, sent) =
                eval_gated(availability_disabling(&["twitch", "youtube"]), call).await;

            assert!(result.is_ok(), "{call} failed: {result:?}");
            assert_eq!(sent.len(), 1, "{call}");
            assert!(
                sent[0].payload.get("target").is_none(),
                "{call} published {}",
                sent[0].payload
            );
        }
    }

    #[tokio::test]
    async fn untargeted_whisper_fails_unsent_when_only_platforms_without_whispers_are_enabled() {
        for call in UNTARGETED_WHISPER_CALLS {
            let (result, sent) = eval_gated(availability_disabling(&["twitch"]), call).await;

            assert_runtime_error_mentions(call, result, NO_WHISPER_PLATFORM_ENABLED_REASON);
            assert!(sent.is_empty(), "{call} published {sent:?}");
        }
    }

    #[tokio::test]
    async fn untargeted_whisper_broadcasts_without_a_target_while_twitch_is_enabled() {
        for call in UNTARGETED_WHISPER_CALLS {
            let (result, sent) =
                eval_gated(availability_disabling(&["youtube", "kick"]), call).await;

            assert!(result.is_ok(), "{call} failed: {result:?}");
            assert_eq!(sent.len(), 1, "{call}");
            assert!(
                sent[0].payload.get("target").is_none(),
                "{call} published {}",
                sent[0].payload
            );
            assert_eq!(sent[0].payload[WHISPER_RECIPIENT_FIELD], "viewer", "{call}");
        }
    }

    #[tokio::test]
    async fn targeted_whisper_to_a_platform_without_whispers_fails_unsent() {
        for (call, target) in [
            (r#"forge::chat::whisper("kick", "viewer", "hello")"#, "kick"),
            (
                r#"forge::chat::whisper("youtube", "viewer", "hello")"#,
                "youtube",
            ),
            (
                r#"forge::chat::whisper(" kick ", "viewer", "hello")"#,
                "kick",
            ),
            (
                r#"forge::chat::whisper("myspace", "viewer", "hello")"#,
                "myspace",
            ),
        ] {
            let (result, sent) = eval_gated(SwitchableAvailability::default(), call).await;

            assert_runtime_error_mentions(call, result, &whispers_unsupported_reason(target));
            assert!(sent.is_empty(), "{call} published {sent:?}");
        }
    }

    #[tokio::test]
    async fn a_padded_chat_target_is_published_trimmed() {
        let (result, sent) = eval_gated(
            SwitchableAvailability::default(),
            r#"forge::chat::send(" twitch ", "hello")"#,
        )
        .await;

        assert!(result.is_ok(), "{result:?}");
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].payload["target"].as_str(), Some("twitch"));
    }

    #[tokio::test]
    async fn a_padded_chat_target_naming_a_disabled_platform_is_refused_unsent() {
        let (result, sent) = eval_gated(
            SwitchableAvailability::disabling("twitch"),
            r#"forge::chat::send(" twitch ", "hello")"#,
        )
        .await;

        match result {
            Err(crate::ScriptError::Runtime { reason, .. }) => assert!(
                reason.contains("integration disabled: twitch"),
                "failed for the wrong reason: {reason}"
            ),
            other => panic!("must fail as a runtime error, got {other:?}"),
        }
        assert!(sent.is_empty(), "published {sent:?}");
    }

    #[derive(Default)]
    struct LatestReader {
        filled: bool,
        asked: Mutex<Vec<Option<String>>>,
    }

    impl LatestValueReader for LatestReader {
        fn latest(&self, slot: &str, scope: LatestScope<'_>) -> Option<forge_types::LatestValue> {
            let platform = match scope {
                LatestScope::MostRecentAcrossPlatforms => None,
                LatestScope::Platform(platform) => Some(platform.to_owned()),
            };
            self.asked.lock().unwrap().push(platform.clone());
            self.filled.then(|| {
                forge_types::LatestValue::new(
                    platform.unwrap_or_else(|| "merged".to_owned()),
                    time::OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap(),
                    std::collections::BTreeMap::from([(
                        "slot_asked".to_owned(),
                        Variant::String(slot.to_owned()),
                    )]),
                )
            })
        }
    }

    async fn eval_latest(reader: Arc<LatestReader>, script: &'static str) -> rhai::Dynamic {
        let dp = open_dp().await;
        let (api, _) = make_api_with_publisher(Arc::clone(&dp), Arc::new(Mutex::new(Vec::new())));
        let engine = Engine::with_api(
            EngineConfig::default(),
            api.with_latest_values(reader as Arc<dyn LatestValueReader>),
        );
        tokio::task::spawn_blocking(move || engine.eval_script(script).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn latest_get_reads_merged_or_one_platform_by_arity() {
        for (script, asked, platform) in [
            (r#"forge::latest::get("donation").platform"#, None, "merged"),
            (
                r#"forge::latest::get("donation", "monobank").platform"#,
                Some("monobank".to_owned()),
                "monobank",
            ),
            (
                r#"forge::latest::get("donation", "").platform"#,
                None,
                "merged",
            ),
        ] {
            let reader = Arc::new(LatestReader {
                filled: true,
                ..LatestReader::default()
            });

            let read = eval_latest(Arc::clone(&reader), script).await;

            assert_eq!(*reader.asked.lock().unwrap(), [asked], "{script}");
            assert_eq!(read.into_string().unwrap(), platform, "{script}");
        }
    }

    #[tokio::test]
    async fn latest_get_hands_the_script_the_whole_value_as_a_map() {
        let reader = Arc::new(LatestReader {
            filled: true,
            ..LatestReader::default()
        });

        let read = eval_latest(reader, r#"forge::latest::get("now_playing").slot_asked"#).await;

        assert_eq!(read.into_string().unwrap(), "now_playing");
    }

    #[tokio::test]
    async fn latest_get_on_an_empty_slot_returns_unit_for_both_arities() {
        for script in [
            r#"forge::latest::get("donation")"#,
            r#"forge::latest::get("donation", "twitch")"#,
        ] {
            let read = eval_latest(Arc::new(LatestReader::default()), script).await;

            assert!(read.is_unit(), "{script}");
        }
    }
}
