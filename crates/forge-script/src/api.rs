use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use forge_events::{
    ChatSendRequest, EventPublisher, EventSource, ScriptHttpCall, ScriptHttpMethod, ScriptLogLevel,
    ScriptLogged, TypedEvent,
};
use forge_platform_core::{SenderPreference, unknown_sender_reason};
use forge_types::{
    EventId, IntegrationAvailability, IntegrationId, LatestScope, LatestValueReader,
    NO_CHAT_PLATFORM_ENABLED_REASON, NO_WHISPER_PLATFORM_ENABLED_REASON, SCRIPT_LOG_TARGET,
    ScriptId, integration_disabled_reason, resolve_chat_target, whispers_unsupported_reason,
};
use rhai::{EvalAltResult, ImmutableString, Module, Position};
use tokio::runtime::Handle;

use crate::catalog::MethodDescriptor;
use crate::convert::{dynamic_to_variant, variant_to_dynamic};
use crate::globals_port::{GlobalDeletion, ScriptGlobals};
use crate::http_client::{HttpError, HttpResponse, ScriptHttpClient};
use crate::registration::{ApiModule, ScriptFn};
use crate::schedule::ActionScheduler;
use crate::schedule_api::build_schedule_module;

const SENDER_DOC: &str =
    "sender is \"default\", \"streamer\" or \"bot\"; any other value is a script error.";
const TARGET_DOC: &str =
    "target is a platform name; an empty target sends to every enabled platform.";
const SENDER_TARGET_DOC: &str = "sender is \"default\", \"streamer\" or \"bot\"; target is a platform name or empty for every enabled platform.";

const SPEAK_UNAVAILABLE_REASON: &str = "text-to-speech is not available where this script runs";
const HTTP_UNAVAILABLE_REASON: &str = "HTTP requests are not available where this script runs";
const LATEST_UNAVAILABLE_REASON: &str = "latest values are not available where this script runs";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScriptSpeakError {
    #[error("{0}")]
    UnknownVoice(String),
    #[error("the speak queue is unavailable: {0}")]
    QueueUnavailable(String),
}

#[async_trait::async_trait]
pub trait SpeakRequester: Send + Sync {
    async fn speak(&self, text: String, voice: Option<String>) -> Result<(), ScriptSpeakError>;
    async fn skip(&self);
    async fn clear(&self);
}

pub struct ForgeApi {
    publisher: Arc<dyn EventPublisher>,
    globals: Arc<dyn ScriptGlobals>,
    caused_by: EventId,
    script_id: Option<ScriptId>,
    error_count: Arc<AtomicU32>,
    speak: Option<Arc<dyn SpeakRequester>>,
    http: Option<Arc<ScriptHttpClient>>,
    integrations: Option<Arc<dyn IntegrationAvailability>>,
    latest: Option<Arc<dyn LatestValueReader>>,
    scheduler: Option<Arc<dyn ActionScheduler>>,
    pub deadline: Instant,
}

impl ForgeApi {
    pub fn new(
        publisher: Arc<dyn EventPublisher>,
        globals: Arc<dyn ScriptGlobals>,
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
            scheduler: None,
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

    pub fn with_action_scheduler(mut self, scheduler: Arc<dyn ActionScheduler>) -> Self {
        self.scheduler = Some(scheduler);
        self
    }

    pub fn into_module(self) -> Arc<Module> {
        self.build().into_module()
    }

    pub(crate) fn describe(self) -> Vec<MethodDescriptor> {
        self.build().into_methods()
    }

    fn build(self) -> ApiModule {
        let mut root = ApiModule::default();

        let caused_by = self.caused_by;
        let script_id = self.script_id;

        let log_pub = Arc::clone(&self.publisher);
        root.register(
            ScriptFn {
                name: "log",
                params: ["msg"],
                doc: None,
            },
            move |msg: ImmutableString| -> Result<(), Box<EvalAltResult>> {
                tracing::info!(target: SCRIPT_LOG_TARGET, caused_by = %caused_by, message = %msg);
                publish_script_log(
                    log_pub.as_ref(),
                    ScriptLogLevel::Info,
                    &msg,
                    script_id,
                    caused_by,
                );
                Ok(())
            },
        );
        let warn_pub = Arc::clone(&self.publisher);
        root.register(
            ScriptFn {
                name: "warn",
                params: ["msg"],
                doc: None,
            },
            move |msg: ImmutableString| -> Result<(), Box<EvalAltResult>> {
                tracing::warn!(target: SCRIPT_LOG_TARGET, caused_by = %caused_by, message = %msg);
                publish_script_log(
                    warn_pub.as_ref(),
                    ScriptLogLevel::Warn,
                    &msg,
                    script_id,
                    caused_by,
                );
                Ok(())
            },
        );
        let error_pub = Arc::clone(&self.publisher);
        let error_counter = Arc::clone(&self.error_count);
        root.register(
            ScriptFn {
                name: "error",
                params: ["msg"],
                doc: None,
            },
            move |msg: ImmutableString| -> Result<(), Box<EvalAltResult>> {
                tracing::error!(target: SCRIPT_LOG_TARGET, caused_by = %caused_by, message = %msg);
                error_counter.fetch_add(1, Ordering::Relaxed);
                publish_script_log(
                    error_pub.as_ref(),
                    ScriptLogLevel::Error,
                    &msg,
                    script_id,
                    caused_by,
                );
                Ok(())
            },
        );

        let deadline = self.deadline;
        root.register(
            ScriptFn {
                name: "sleep",
                params: ["ms"],
                doc: Some("Clamped to the script's remaining wall-time budget."),
            },
            move |ms: i64| -> Result<(), Box<EvalAltResult>> {
                let now = Instant::now();
                if now >= deadline {
                    return Err("script execution deadline exceeded".into());
                }
                let remaining_ms = (deadline - now).as_millis() as u64;
                let clamped = (ms.max(0) as u64).min(5_000).min(remaining_ms);
                std::thread::sleep(Duration::from_millis(clamped));
                Ok(())
            },
        );

        root.mount(
            "chat",
            build_chat_module(
                Arc::clone(&self.publisher),
                self.caused_by,
                self.integrations,
            ),
        );
        root.mount(
            "globals",
            build_globals_module(self.caused_by, self.globals),
        );
        root.mount("tts", build_tts_module(self.speak));
        root.mount("time", build_time_module());
        root.mount(
            "http",
            build_http_module(self.http, Arc::clone(&self.publisher), self.caused_by),
        );
        root.mount("latest", build_latest_module(self.latest));
        root.mount("schedule", build_schedule_module(self.scheduler));

        root
    }
}

fn publish_script_log(
    publisher: &dyn EventPublisher,
    level: ScriptLogLevel,
    message: &str,
    script_id: Option<ScriptId>,
    caused_by: EventId,
) {
    ScriptLogged {
        level,
        message: message.to_owned(),
        script_id,
    }
    .publish(publisher, EventSource::Rhai, Some(caused_by));
}

fn publish_http_call(
    publisher: &dyn EventPublisher,
    method: ScriptHttpMethod,
    response: &HttpResponse,
    caused_by: EventId,
) {
    ScriptHttpCall {
        method,
        url_normalized: response.url_normalized.clone(),
        status: response.status,
        duration_ms: response.duration_ms,
        truncated: response.truncated,
    }
    .publish(publisher, EventSource::Rhai, Some(caused_by));
}

fn available<'a, T: ?Sized>(
    dependency: &'a Option<Arc<T>>,
    unavailable_reason: &'static str,
) -> Result<&'a Arc<T>, Box<EvalAltResult>> {
    dependency.as_ref().ok_or_else(|| unavailable_reason.into())
}

fn build_http_module(
    client: Option<Arc<ScriptHttpClient>>,
    publisher: Arc<dyn EventPublisher>,
    caused_by: EventId,
) -> ApiModule {
    let mut m = ApiModule::default();
    let counter = Arc::new(AtomicU32::new(0));

    {
        let client = client.clone();
        let counter = Arc::clone(&counter);
        let publisher = Arc::clone(&publisher);
        m.register(
            ScriptFn {
                name: "get",
                params: ["url"],
                doc: None,
            },
            move |url: ImmutableString| -> Result<rhai::Map, Box<EvalAltResult>> {
                let client = available(&client, HTTP_UNAVAILABLE_REASON)?;
                let result = client.get(url.as_str(), &counter);
                if let Ok(ref resp) = result {
                    publish_http_call(publisher.as_ref(), ScriptHttpMethod::Get, resp, caused_by);
                }
                result
                    .map(http_response_to_rhai_map)
                    .map_err(http_error_to_rhai_error)
            },
        );
    }

    {
        let publisher = Arc::clone(&publisher);
        m.register(
            ScriptFn {
                name: "post",
                params: ["url", "body"],
                doc: None,
            },
            move |url: ImmutableString,
                  body: ImmutableString|
                  -> Result<rhai::Map, Box<EvalAltResult>> {
                let client = available(&client, HTTP_UNAVAILABLE_REASON)?;
                let result = client.post(url.as_str(), body.as_str(), &counter);
                if let Ok(ref resp) = result {
                    publish_http_call(publisher.as_ref(), ScriptHttpMethod::Post, resp, caused_by);
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

fn build_time_module() -> ApiModule {
    let mut m = ApiModule::default();

    m.register(
        ScriptFn {
            name: "now",
            params: [],
            doc: Some("Current UTC time as RFC 3339 text."),
        },
        || -> Result<ImmutableString, Box<EvalAltResult>> {
            Ok(forge_types::datetime_to_text(&time::OffsetDateTime::now_utc()).into())
        },
    );

    m.register(
        ScriptFn {
            name: "unix",
            params: [],
            doc: Some("Current unix time in seconds."),
        },
        || -> Result<i64, Box<EvalAltResult>> {
            Ok(time::OffsetDateTime::now_utc().unix_timestamp())
        },
    );

    m
}

fn build_latest_module(reader: Option<Arc<dyn LatestValueReader>>) -> ApiModule {
    let mut m = ApiModule::default();

    let merged = reader.clone();
    m.register(
        ScriptFn {
            name: "get",
            params: ["slot"],
            doc: Some("Most recent value across platforms; () when the slot is empty."),
        },
        move |slot: ImmutableString| -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            let merged = available(&merged, LATEST_UNAVAILABLE_REASON)?;
            Ok(latest_dynamic(
                merged.as_ref(),
                slot.as_str(),
                LatestScope::MostRecentAcrossPlatforms,
            ))
        },
    );

    m.register(
        ScriptFn {
            name: "get",
            params: ["slot", "platform"],
            doc: Some("Value from one platform or service; () when it has none."),
        },
        move |slot: ImmutableString,
              platform: ImmutableString|
              -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            let reader = available(&reader, LATEST_UNAVAILABLE_REASON)?;
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

fn build_tts_module(requester: Option<Arc<dyn SpeakRequester>>) -> ApiModule {
    let mut m = ApiModule::default();

    let r_speak = requester.clone();
    m.register(
        ScriptFn {
            name: "speak",
            params: ["text"],
            doc: None,
        },
        move |text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let r = Arc::clone(available(&r_speak, SPEAK_UNAVAILABLE_REASON)?);
            let text_owned = text.to_string();
            Handle::current()
                .block_on(async move { r.speak(text_owned, None).await })
                .map_err(|e| -> Box<EvalAltResult> { format!("tts::speak: {e}").into() })
        },
    );

    let r_speak_as = requester.clone();
    m.register(
        ScriptFn {
            name: "speak_as",
            params: ["voice_id", "text"],
            doc: None,
        },
        move |voice: ImmutableString, text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let r = Arc::clone(available(&r_speak_as, SPEAK_UNAVAILABLE_REASON)?);
            let voice_owned = voice.to_string();
            let text_owned = text.to_string();
            Handle::current()
                .block_on(async move { r.speak(text_owned, Some(voice_owned)).await })
                .map_err(|e| -> Box<EvalAltResult> { format!("tts::speak_as: {e}").into() })
        },
    );

    let r_skip = requester.clone();
    m.register(
        ScriptFn {
            name: "skip",
            params: [],
            doc: None,
        },
        move || -> Result<(), Box<EvalAltResult>> {
            let r = Arc::clone(available(&r_skip, SPEAK_UNAVAILABLE_REASON)?);
            Handle::current().block_on(async move { r.skip().await });
            Ok(())
        },
    );

    let r_clear = requester;
    m.register(
        ScriptFn {
            name: "clear",
            params: [],
            doc: None,
        },
        move || -> Result<(), Box<EvalAltResult>> {
            let r = Arc::clone(available(&r_clear, SPEAK_UNAVAILABLE_REASON)?);
            Handle::current().block_on(async move { r.clear().await });
            Ok(())
        },
    );

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

fn admit_chat_target(
    integrations: Option<&dyn IntegrationAvailability>,
    raw: &str,
) -> Result<Option<&'static str>, Box<EvalAltResult>> {
    let Some(platform) = resolve_chat_target(raw)? else {
        admit_broadcast(integrations)?;
        return Ok(None);
    };
    let integration = IntegrationId::from_static(platform.as_str());
    if integrations.is_some_and(|integrations| integrations.is_disabled(&integration)) {
        return Err(integration_disabled_reason(&integration).into());
    }
    Ok(Some(platform.as_str()))
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

fn admit_whisper_target(
    integrations: Option<&dyn IntegrationAvailability>,
    raw: &str,
) -> Result<Option<&'static str>, Box<EvalAltResult>> {
    let Some(platform) = resolve_chat_target(raw)? else {
        admit_whisper_broadcast(integrations)?;
        return Ok(None);
    };
    if !platform.supports_whispers() {
        return Err(whispers_unsupported_reason(platform.as_str()).into());
    }
    admit_chat_target(integrations, platform.as_str())
}

fn script_sender(raw: &str) -> Result<SenderPreference, Box<EvalAltResult>> {
    SenderPreference::parse(raw).ok_or_else(|| unknown_sender_reason(raw).into())
}

fn chat_text(text: &str) -> ChatSendRequest {
    ChatSendRequest {
        message: text.to_owned(),
        ..ChatSendRequest::default()
    }
}

fn chat_reply(to: &str, text: &str) -> ChatSendRequest {
    ChatSendRequest {
        reply_to_message_id: Some(to.to_owned()),
        ..chat_text(text)
    }
}

fn chat_whisper(user: &str, text: &str) -> ChatSendRequest {
    ChatSendRequest {
        whisper_to_login: Some(user.to_owned()),
        ..chat_text(text)
    }
}

fn aimed(target: Option<&'static str>, request: ChatSendRequest) -> ChatSendRequest {
    ChatSendRequest {
        target: target.map(str::to_owned),
        ..request
    }
}

fn sent_by(sender: SenderPreference, request: ChatSendRequest) -> ChatSendRequest {
    ChatSendRequest {
        sender: (sender != SenderPreference::Default).then(|| sender.as_str().to_owned()),
        ..request
    }
}

#[derive(Clone)]
struct ChatRequester {
    publisher: Arc<dyn EventPublisher>,
    caused_by: EventId,
    integrations: Option<Arc<dyn IntegrationAvailability>>,
}

impl ChatRequester {
    fn integrations(&self) -> Option<&dyn IntegrationAvailability> {
        self.integrations.as_deref()
    }

    fn request(&self, request: ChatSendRequest) -> Result<(), Box<EvalAltResult>> {
        request.publish(
            self.publisher.as_ref(),
            EventSource::Rhai,
            Some(self.caused_by),
        );
        Ok(())
    }
}

fn build_chat_module(
    publisher: Arc<dyn EventPublisher>,
    caused_by: EventId,
    integrations: Option<Arc<dyn IntegrationAvailability>>,
) -> ApiModule {
    let mut m = ApiModule::default();
    let chat = ChatRequester {
        publisher,
        caused_by,
        integrations,
    };

    let send = chat.clone();
    m.register(
        ScriptFn {
            name: "send",
            params: ["text"],
            doc: None,
        },
        move |text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            admit_broadcast(send.integrations())?;
            send.request(chat_text(&text))
        },
    );

    let send = chat.clone();
    m.register(
        ScriptFn {
            name: "send",
            params: ["target", "text"],
            doc: Some(TARGET_DOC),
        },
        move |target: ImmutableString, text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let target = admit_chat_target(send.integrations(), &target)?;
            send.request(aimed(target, chat_text(&text)))
        },
    );

    let reply = chat.clone();
    m.register(
        ScriptFn {
            name: "reply",
            params: ["to", "text"],
            doc: None,
        },
        move |to: ImmutableString, text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            admit_broadcast(reply.integrations())?;
            reply.request(chat_reply(&to, &text))
        },
    );

    let reply = chat.clone();
    m.register(
        ScriptFn {
            name: "reply",
            params: ["target", "to", "text"],
            doc: Some(TARGET_DOC),
        },
        move |target: ImmutableString,
              to: ImmutableString,
              text: ImmutableString|
              -> Result<(), Box<EvalAltResult>> {
            let target = admit_chat_target(reply.integrations(), &target)?;
            reply.request(aimed(target, chat_reply(&to, &text)))
        },
    );

    let whisper = chat.clone();
    m.register(
        ScriptFn {
            name: "whisper",
            params: ["user", "text"],
            doc: None,
        },
        move |user: ImmutableString, text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            admit_whisper_broadcast(whisper.integrations())?;
            whisper.request(chat_whisper(&user, &text))
        },
    );

    let whisper = chat.clone();
    m.register(
        ScriptFn {
            name: "whisper",
            params: ["target", "user", "text"],
            doc: Some(TARGET_DOC),
        },
        move |target: ImmutableString,
              user: ImmutableString,
              text: ImmutableString|
              -> Result<(), Box<EvalAltResult>> {
            let target = admit_whisper_target(whisper.integrations(), &target)?;
            whisper.request(aimed(target, chat_whisper(&user, &text)))
        },
    );

    register_sender_chat_fns(&mut m, chat);

    m
}

fn register_sender_chat_fns(m: &mut ApiModule, chat: ChatRequester) {
    let send = chat.clone();
    m.register(
        ScriptFn {
            name: "send_as",
            params: ["sender", "text"],
            doc: Some(SENDER_DOC),
        },
        move |sender: ImmutableString, text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let sender = script_sender(&sender)?;
            admit_broadcast(send.integrations())?;
            send.request(sent_by(sender, chat_text(&text)))
        },
    );

    let send = chat.clone();
    m.register(
        ScriptFn {
            name: "send_as",
            params: ["sender", "target", "text"],
            doc: Some(SENDER_TARGET_DOC),
        },
        move |sender: ImmutableString,
              target: ImmutableString,
              text: ImmutableString|
              -> Result<(), Box<EvalAltResult>> {
            let sender = script_sender(&sender)?;
            let target = admit_chat_target(send.integrations(), &target)?;
            send.request(sent_by(sender, aimed(target, chat_text(&text))))
        },
    );

    let reply = chat.clone();
    m.register(
        ScriptFn {
            name: "reply_as",
            params: ["sender", "to", "text"],
            doc: Some(SENDER_DOC),
        },
        move |sender: ImmutableString,
              to: ImmutableString,
              text: ImmutableString|
              -> Result<(), Box<EvalAltResult>> {
            let sender = script_sender(&sender)?;
            admit_broadcast(reply.integrations())?;
            reply.request(sent_by(sender, chat_reply(&to, &text)))
        },
    );

    let reply = chat.clone();
    m.register(
        ScriptFn {
            name: "reply_as",
            params: ["sender", "target", "to", "text"],
            doc: Some(SENDER_TARGET_DOC),
        },
        move |sender: ImmutableString,
              target: ImmutableString,
              to: ImmutableString,
              text: ImmutableString|
              -> Result<(), Box<EvalAltResult>> {
            let sender = script_sender(&sender)?;
            let target = admit_chat_target(reply.integrations(), &target)?;
            reply.request(sent_by(sender, aimed(target, chat_reply(&to, &text))))
        },
    );

    let whisper = chat.clone();
    m.register(
        ScriptFn {
            name: "whisper_as",
            params: ["sender", "user", "text"],
            doc: Some(SENDER_DOC),
        },
        move |sender: ImmutableString,
              user: ImmutableString,
              text: ImmutableString|
              -> Result<(), Box<EvalAltResult>> {
            let sender = script_sender(&sender)?;
            admit_whisper_broadcast(whisper.integrations())?;
            whisper.request(sent_by(sender, chat_whisper(&user, &text)))
        },
    );

    let whisper = chat;
    m.register(
        ScriptFn {
            name: "whisper_as",
            params: ["sender", "target", "user", "text"],
            doc: Some(SENDER_TARGET_DOC),
        },
        move |sender: ImmutableString,
              target: ImmutableString,
              user: ImmutableString,
              text: ImmutableString|
              -> Result<(), Box<EvalAltResult>> {
            let sender = script_sender(&sender)?;
            let target = admit_whisper_target(whisper.integrations(), &target)?;
            whisper.request(sent_by(sender, aimed(target, chat_whisper(&user, &text))))
        },
    );
}

fn build_globals_module(caused_by: EventId, globals: Arc<dyn ScriptGlobals>) -> ApiModule {
    let mut m = ApiModule::default();

    let globals_get = Arc::clone(&globals);
    m.register(
        ScriptFn {
            name: "get",
            params: ["key"],
            doc: Some("Returns () when the key is absent."),
        },
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
    m.register(
        ScriptFn {
            name: "set",
            params: ["key", "value", "persisted"],
            doc: None,
        },
        move |key: ImmutableString,
              val: rhai::Dynamic,
              persisted: bool|
              -> Result<(), Box<EvalAltResult>> {
            let variant =
                dynamic_to_variant(val).map_err(|e| -> Box<EvalAltResult> { e.into() })?;
            Handle::current()
                .block_on(globals_set.set(key.as_str(), variant, persisted, caused_by))
                .map_err(|e| -> Box<EvalAltResult> { e.to_string().into() })?;
            Ok(())
        },
    );

    let globals_incr = Arc::clone(&globals);
    m.register(
        ScriptFn {
            name: "incr",
            params: ["key", "amount"],
            doc: Some("Returns the new value."),
        },
        move |key: ImmutableString, amount: i64| -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            let increment = Handle::current()
                .block_on(globals_incr.increment(key.as_str(), amount, caused_by))
                .map_err(|e| -> Box<EvalAltResult> { e.to_string().into() })?;
            Ok(variant_to_dynamic(increment.current))
        },
    );

    let globals_del = globals;
    m.register(
        ScriptFn {
            name: "del",
            params: ["key"],
            doc: Some("Returns true if the key existed."),
        },
        move |key: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            let deletion = Handle::current()
                .block_on(globals_del.delete(key.as_str(), caused_by))
                .map_err(|e| -> Box<EvalAltResult> { e.to_string().into() })?;
            Ok(matches!(deletion, GlobalDeletion::Deleted { .. }))
        },
    );

    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Engine, EngineConfig};
    use crate::http_config::ScriptHttpConfig;
    use forge_events::Event;
    use forge_storage::GlobalsRepo;
    use forge_storage_sqlite::SqliteBackend;

    use crate::globals_port::ScriptGlobals;
    use crate::test_support::{RepoGlobals, Sandboxed, repo_globals, sandboxed_backend};
    use forge_types::{EventId, Variant, unknown_chat_target_reason};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    fn unhurried() -> EngineConfig {
        EngineConfig {
            wall_time_ms: 30_000,
            ..EngineConfig::default()
        }
    }

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
            repo_globals(dp.clone()),
            caused_by,
            Instant::now() + std::time::Duration::from_secs(30),
        );
        (api, caused_by)
    }

    async fn eval_against_recorded_globals(
        seeded: &[(&str, Variant)],
        script: &'static str,
    ) -> (rhai::Dynamic, EventId, Arc<RepoGlobals>, Vec<Event>) {
        let dp = open_dp().await;
        for (key, value) in seeded {
            GlobalsRepo::set(dp.as_ref(), key, value.clone(), false)
                .await
                .unwrap();
        }
        let port = Arc::new(RepoGlobals::new(Arc::clone(&dp) as Arc<dyn GlobalsRepo>));
        let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let caused_by = EventId::new();
        let api = ForgeApi::new(
            Arc::new(CapturingPublisher(Arc::clone(&captured))),
            Arc::clone(&port) as Arc<dyn ScriptGlobals>,
            caused_by,
            Instant::now() + std::time::Duration::from_secs(30),
        );
        let engine = Engine::with_api(unhurried(), api);
        let result = tokio::task::spawn_blocking(move || engine.eval_script(script).unwrap())
            .await
            .unwrap();
        let events = captured.lock().unwrap().clone();
        (result, caused_by, port, events)
    }

    fn seeded_hits_and_temp() -> Vec<(&'static str, Variant)> {
        vec![("hits", Variant::Int(10)), ("temp", Variant::Int(1))]
    }

    #[tokio::test]
    async fn global_mutations_reach_the_port_carrying_the_run_event_as_cause() {
        for script in [
            r#"forge::globals::set("score", 77, false)"#,
            r#"forge::globals::incr("hits", 3)"#,
            r#"forge::globals::del("temp")"#,
        ] {
            let (_, caused_by, port, _) =
                eval_against_recorded_globals(&seeded_hits_and_temp(), script).await;

            assert_eq!(port.causes(), [caused_by], "{script}");
        }
    }

    #[tokio::test]
    async fn global_mutations_publish_nothing_through_the_script_publisher() {
        for script in [
            r#"forge::globals::set("score", 77, false)"#,
            r#"forge::globals::incr("hits", 3)"#,
            r#"forge::globals::del("temp")"#,
        ] {
            let (_, _, _, events) =
                eval_against_recorded_globals(&seeded_hits_and_temp(), script).await;

            assert!(events.is_empty(), "{script}: {events:?}");
        }
    }

    #[tokio::test]
    async fn del_returns_whether_the_key_existed() {
        for (seeded, expected) in [(seeded_hits_and_temp(), true), (Vec::new(), false)] {
            let (result, _, _, _) =
                eval_against_recorded_globals(&seeded, r#"forge::globals::del("temp")"#).await;

            assert_eq!(result.as_bool(), Ok(expected), "seeded: {expected}");
        }
    }

    #[tokio::test]
    async fn incr_returns_the_value_after_the_increment() {
        let (result, _, _, _) = eval_against_recorded_globals(
            &seeded_hits_and_temp(),
            r#"forge::globals::incr("hits", 3)"#,
        )
        .await;

        assert_eq!(result.as_int(), Ok(13));
    }

    fn build_engine_with_http_in_blocking(
        dp: Arc<SqliteBackend>,
        captured: Arc<Mutex<Vec<Event>>>,
        config: Arc<ScriptHttpConfig>,
    ) -> Engine {
        let caused_by = EventId::new();
        let http_client = Arc::new(ScriptHttpClient::new(config).unwrap());
        let api = ForgeApi::new(
            Arc::new(CapturingPublisher(captured)),
            repo_globals(dp.clone()),
            caused_by,
            Instant::now() + std::time::Duration::from_secs(30),
        )
        .with_http(http_client);
        Engine::with_api(unhurried(), api)
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
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-trace", "t-9")
                    .set_body_string("pong"),
            )
            .mount(&server)
            .await;

        let server_url = server.uri();
        let host = server.address().ip().to_string();
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
        let headers = map["headers"].clone().try_cast::<rhai::Map>().unwrap();
        assert_eq!(
            (
                map["status"].clone().as_int().unwrap(),
                map["body"].clone().into_string().unwrap(),
                map["truncated"].clone().as_bool().unwrap(),
                map["duration_ms"].clone().as_int().is_ok(),
                headers["x-trace"].clone().into_string().unwrap(),
            ),
            (200, "pong".to_owned(), false, true, "t-9".to_owned())
        );
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
                repo_globals(dp.clone()),
                caused_by,
                Instant::now() + std::time::Duration::from_secs(30),
            )
            .with_script_id(script_id);
            let engine = Engine::with_api(unhurried(), api);

            tokio::task::spawn_blocking(move || {
                drop(engine.eval_script(call).unwrap());
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
            repo_globals(dp.clone()),
            caused_by,
            Instant::now() + std::time::Duration::from_secs(30),
        );
        let counter = api.error_count_handle();
        let engine = Engine::with_api(unhurried(), api);

        tokio::task::spawn_blocking(move || {
            drop(engine
                .eval_script(
                    r#"forge::log("a"); forge::warn("b"); forge::error("c"); forge::error("d");"#,
                )
                .unwrap());
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
        let engine = Engine::with_api(unhurried(), api);

        tokio::task::spawn_blocking(move || {
            drop(engine.eval_script(r#"forge::log("x")"#).unwrap());
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
        let host = server.address().ip().to_string();

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
                drop(engine.eval_script(&script).unwrap());
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
    async fn http_call_event_reports_the_outcome_without_the_query_and_only_for_sent_calls() {
        use wiremock::matchers::any;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(404).set_body_string("missing"))
            .mount(&server)
            .await;
        let server_url = server.uri();
        let host = server.address().ip().to_string();
        let dp = open_dp().await;
        let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let config = Arc::new(ScriptHttpConfig {
            allowed_domains: vec![host.clone()],
            allow_local: true,
            ..ScriptHttpConfig::default()
        });
        let script = format!(
            r#"forge::http::get("{server_url}/p?token=QUERY-SECRET"); forge::http::get("https://not-allowed.example/");"#
        );
        let cap = Arc::clone(&captured);

        tokio::task::spawn_blocking(move || {
            let engine = build_engine_with_http_in_blocking(dp.clone(), cap, config);
            assert!(
                engine.eval_script(&script).is_err(),
                "the disallowed second call aborts the script"
            );
        })
        .await
        .unwrap();

        let events = captured.lock().unwrap();
        let calls: Vec<&Event> = events
            .iter()
            .filter(|e| e.kind == "script.http_call")
            .collect();
        assert_eq!(calls.len(), 1, "only the sent call emits script.http_call");
        let payload = &calls[0].payload;
        assert_eq!(
            (
                payload["status"].as_i64(),
                payload["truncated"].as_bool(),
                payload["url_normalized"].as_str(),
                calls[0].caused_by.is_some(),
            ),
            (
                Some(404),
                Some(false),
                Some(format!("http://{host}/p").as_str()),
                true
            )
        );
        assert!(!payload.to_string().contains("QUERY-SECRET"), "{payload}");
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
            let engine = Engine::with_api(unhurried(), api);

            tokio::task::spawn_blocking(move || {
                drop(engine.eval_script(call).unwrap());
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
            let engine = Engine::with_api(unhurried(), api);

            tokio::task::spawn_blocking(move || {
                drop(engine.eval_script(call).unwrap());
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

    #[tokio::test]
    async fn sender_taking_chat_calls_publish_the_sender_beside_the_usual_payload() {
        let hello = |target: Option<&str>, sender: Option<&str>| ChatSendRequest {
            message: "hello".to_owned(),
            target: target.map(str::to_owned),
            sender: sender.map(str::to_owned),
            ..ChatSendRequest::default()
        };
        let reply = |target, sender| ChatSendRequest {
            reply_to_message_id: Some("msg-1".to_owned()),
            ..hello(target, sender)
        };
        let whisper = |target, sender| ChatSendRequest {
            whisper_to_login: Some("viewer".to_owned()),
            ..hello(target, sender)
        };
        for (call, expected) in [
            (
                r#"forge::chat::send_as("bot", "hello")"#,
                hello(None, Some("bot")),
            ),
            (
                r#"forge::chat::send_as("streamer", "twitch", "hello")"#,
                hello(Some("twitch"), Some("streamer")),
            ),
            (
                r#"forge::chat::reply_as("bot", "msg-1", "hello")"#,
                reply(None, Some("bot")),
            ),
            (
                r#"forge::chat::reply_as("streamer", "Twitch", "msg-1", "hello")"#,
                reply(Some("twitch"), Some("streamer")),
            ),
            (
                r#"forge::chat::whisper_as("bot", "viewer", "hello")"#,
                whisper(None, Some("bot")),
            ),
            (
                r#"forge::chat::whisper_as("bot", "twitch", "viewer", "hello")"#,
                whisper(Some("twitch"), Some("bot")),
            ),
            (
                r#"forge::chat::send_as("default", "twitch", "hello")"#,
                hello(Some("twitch"), None),
            ),
            (r#"forge::chat::send("hello")"#, hello(None, None)),
        ] {
            let (result, sent) = eval_gated(SwitchableAvailability::default(), call).await;

            assert!(result.is_ok(), "{call} failed: {result:?}");
            let published: Vec<_> = sent.iter().map(ChatSendRequest::from_event).collect();
            assert_eq!(published, vec![Some(expected)], "{call}");
        }
    }

    #[tokio::test]
    async fn an_unknown_sender_is_a_script_error_and_publishes_nothing() {
        for (call, raw) in [
            (r#"forge::chat::send_as("Bot", "hello")"#, "Bot"),
            (r#"forge::chat::send_as("", "twitch", "hello")"#, ""),
            (
                r#"forge::chat::reply_as("bot_only", "msg-1", "hello")"#,
                "bot_only",
            ),
            (
                r#"forge::chat::reply_as("me", "twitch", "msg-1", "hello")"#,
                "me",
            ),
            (
                r#"forge::chat::whisper_as(" bot", "viewer", "hello")"#,
                " bot",
            ),
            (
                r#"forge::chat::whisper_as("BOT", "twitch", "viewer", "hello")"#,
                "BOT",
            ),
        ] {
            let (result, sent) = eval_gated(SwitchableAvailability::default(), call).await;

            assert_runtime_error_mentions(call, result, &unknown_sender_reason(raw));
            assert!(sent.is_empty(), "{call} published {sent:?}");
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
        (Engine::with_api(unhurried(), api), dp)
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
            r#"forge::chat::send_as("bot", "twitch", "hello")"#,
            r#"forge::chat::reply_as("bot", "twitch", "msg-1", "hello")"#,
            r#"forge::chat::whisper_as("bot", "twitch", "viewer", "hello")"#,
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

    const UNTARGETED_CHAT_CALLS: [&str; 12] = [
        r#"forge::chat::send("hello")"#,
        r#"forge::chat::reply("msg-1", "hello")"#,
        r#"forge::chat::whisper("viewer", "hello")"#,
        r#"forge::chat::send("", "hello")"#,
        r#"forge::chat::send("  ", "hello")"#,
        r#"forge::chat::reply(" ", "msg-1", "hello")"#,
        r#"forge::chat::whisper("\t", "viewer", "hello")"#,
        r#"forge::chat::send_as("bot", "hello")"#,
        r#"forge::chat::send_as("bot", " ", "hello")"#,
        r#"forge::chat::reply_as("bot", "msg-1", "hello")"#,
        r#"forge::chat::whisper_as("bot", "viewer", "hello")"#,
        r#"forge::chat::whisper_as("bot", "\t", "viewer", "hello")"#,
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

    const UNTARGETED_WHISPER_CALLS: [&str; 4] = [
        r#"forge::chat::whisper("viewer", "hello")"#,
        r#"forge::chat::whisper("\t", "viewer", "hello")"#,
        r#"forge::chat::whisper_as("bot", "viewer", "hello")"#,
        r#"forge::chat::whisper_as("bot", "\t", "viewer", "hello")"#,
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
            assert_eq!(
                ChatSendRequest::from_event(&sent[0]).and_then(|request| request.whisper_to_login),
                Some("viewer".to_owned()),
                "{call}"
            );
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
                r#"forge::chat::whisper("YouTube", "viewer", "hello")"#,
                "youtube",
            ),
            (r#"forge::chat::whisper("Kick", "viewer", "hello")"#, "kick"),
        ] {
            let (result, sent) = eval_gated(SwitchableAvailability::default(), call).await;

            assert_runtime_error_mentions(call, result, &whispers_unsupported_reason(target));
            assert!(sent.is_empty(), "{call} published {sent:?}");
        }
    }

    #[tokio::test]
    async fn a_mixed_case_chat_target_is_published_normalized_for_send_reply_and_whisper() {
        for call in [
            r#"forge::chat::send("Twitch", "hello")"#,
            r#"forge::chat::reply(" TWITCH ", "msg-1", "hello")"#,
            r#"forge::chat::whisper("Twitch", "viewer", "hello")"#,
        ] {
            let (result, sent) = eval_gated(SwitchableAvailability::default(), call).await;

            assert!(result.is_ok(), "{call} failed: {result:?}");
            assert_eq!(sent.len(), 1, "{call}");
            assert_eq!(sent[0].payload["target"].as_str(), Some("twitch"), "{call}");
        }
    }

    #[tokio::test]
    async fn an_unknown_chat_target_fails_unsent_naming_the_valid_targets() {
        for call in [
            r#"forge::chat::send("typo", "hello")"#,
            r#"forge::chat::reply("typo", "msg-1", "hello")"#,
            r#"forge::chat::whisper("typo", "viewer", "hello")"#,
        ] {
            let (result, sent) = eval_gated(SwitchableAvailability::default(), call).await;

            assert_runtime_error_mentions(call, result, &unknown_chat_target_reason("typo"));
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
            unhurried(),
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

    struct ScriptedRequester {
        outcome: Result<(), ScriptSpeakError>,
        asked: Mutex<Vec<(String, Option<String>)>>,
    }

    #[async_trait::async_trait]
    impl SpeakRequester for ScriptedRequester {
        async fn speak(&self, text: String, voice: Option<String>) -> Result<(), ScriptSpeakError> {
            self.asked.lock().unwrap().push((text, voice));
            self.outcome.clone()
        }

        async fn skip(&self) {}

        async fn clear(&self) {}
    }

    async fn eval_speaking(
        outcome: Result<(), ScriptSpeakError>,
        script: &'static str,
    ) -> (
        Result<rhai::Dynamic, crate::ScriptError>,
        Vec<(String, Option<String>)>,
    ) {
        let requester = Arc::new(ScriptedRequester {
            outcome,
            asked: Mutex::new(Vec::new()),
        });
        let dp = open_dp().await;
        let (api, _) = make_api_with_publisher(Arc::clone(&dp), Arc::new(Mutex::new(Vec::new())));
        let engine = Engine::with_api(
            unhurried(),
            api.with_speak_requester(Arc::clone(&requester) as Arc<dyn SpeakRequester>),
        );
        let result = tokio::task::spawn_blocking(move || engine.eval_script(script))
            .await
            .unwrap();
        let asked = requester.asked.lock().unwrap().clone();
        (result, asked)
    }

    #[tokio::test]
    async fn speak_as_requests_the_named_voice_and_speak_the_default_voice() {
        for (script, expected) in [
            (
                r#"forge::tts::speak_as("piper/en/amy", "hi")"#,
                Some("piper/en/amy"),
            ),
            (r#"forge::tts::speak("hi")"#, None),
        ] {
            let (result, asked) = eval_speaking(Ok(()), script).await;

            assert!(result.is_ok(), "{script} failed: {result:?}");
            assert_eq!(
                asked,
                vec![("hi".to_owned(), expected.map(str::to_owned))],
                "{script}"
            );
        }
    }

    #[tokio::test]
    async fn a_refused_speak_request_throws_naming_the_call_and_the_reason() {
        for (script, refusal, expected) in [
            (
                r#"forge::tts::speak_as("piper/ghost", "hi")"#,
                ScriptSpeakError::UnknownVoice(
                    "voice \"ghost\" of engine \"piper\" is not installed".to_owned(),
                ),
                "tts::speak_as: voice \"ghost\" of engine \"piper\" is not installed",
            ),
            (
                r#"forge::tts::speak("hi")"#,
                ScriptSpeakError::QueueUnavailable("the queue actor is gone".to_owned()),
                "tts::speak: the speak queue is unavailable: the queue actor is gone",
            ),
            (
                r#"forge::tts::speak_as("piper/amy", "hi")"#,
                ScriptSpeakError::QueueUnavailable("the queue actor is gone".to_owned()),
                "tts::speak_as: the speak queue is unavailable",
            ),
        ] {
            let (result, _) = eval_speaking(Err(refusal), script).await;

            assert_runtime_error_mentions(script, result, expected);
        }
    }

    fn golden_keys_sorted(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(fields) => {
                let sorted: std::collections::BTreeMap<String, serde_json::Value> = fields
                    .into_iter()
                    .map(|(key, value)| (key, golden_keys_sorted(value)))
                    .collect();
                serde_json::Value::Object(sorted.into_iter().collect())
            }
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.into_iter().map(golden_keys_sorted).collect())
            }
            other => other,
        }
    }

    fn assert_matches_golden(events: &[Event], fixture: &str) {
        let emitted: Vec<serde_json::Value> = events
            .iter()
            .map(|event| {
                let mut payload = event.payload.clone();
                if let Some(duration) = payload.get_mut("duration_ms") {
                    assert!(
                        duration.is_u64(),
                        "duration_ms must stay an unsigned integer"
                    );
                    *duration = serde_json::json!(0);
                }
                serde_json::json!({ "source": event.source, "payload": payload })
            })
            .collect();
        let actual = format!(
            "{}\n",
            serde_json::to_string_pretty(&golden_keys_sorted(serde_json::Value::Array(emitted)))
                .unwrap()
        );
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/observability")
            .join(fixture);
        let golden = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            golden == actual,
            "{fixture} drifted:\n--- golden\n{golden}--- emitted\n{actual}"
        );
    }

    #[tokio::test]
    async fn rhai_chat_request_payloads_match_the_golden() {
        let (result, sent) = eval_gated(
            SwitchableAvailability::default(),
            r#"
                forge::chat::send("golden broadcast");
                forge::chat::reply("Twitch", "msg-1", "golden reply");
                forge::chat::whisper_as("bot", "twitch", "viewer", "golden whisper");
            "#,
        )
        .await;

        assert!(result.is_ok(), "{result:?}");
        assert_matches_golden(&sent, "chat.send.request.json");
    }

    #[tokio::test]
    async fn http_call_payloads_match_the_golden() {
        use wiremock::matchers::any;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(201).set_body_string("ok"))
            .mount(&server)
            .await;
        let server_url = server.uri();
        let dp = open_dp().await;
        let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let config = Arc::new(ScriptHttpConfig {
            allowed_domains: vec![server.address().ip().to_string()],
            allow_local: true,
            ..ScriptHttpConfig::default()
        });
        let script = format!(
            r#"forge::http::get("{server_url}/golden?q=1"); forge::http::post("{server_url}/golden", "body");"#
        );
        let cap = Arc::clone(&captured);

        tokio::task::spawn_blocking(move || {
            let engine = build_engine_with_http_in_blocking(dp.clone(), cap, config);
            drop(engine.eval_script(&script).unwrap());
        })
        .await
        .unwrap();

        let calls: Vec<Event> = captured
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event.kind == "script.http_call")
            .cloned()
            .collect();
        assert_matches_golden(&calls, "script.http_call.json");
    }
}
