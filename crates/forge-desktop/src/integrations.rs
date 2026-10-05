use std::collections::HashMap;
use std::sync::Arc;

use forge_events::{Event, EventPublisher, EventSource, EventStream, EventsError};
use forge_platform_core::{
    BuiltinCollections, BuiltinContent, BuiltinControl, BuiltinHealth, BuiltinStatus, ChatPlatform,
    PlatformEndpoints, QuickActions, SectionIcon,
};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{DonationIngest, EventBus};
use forge_storage::{CredentialsRepo, DataProvider};
use forge_types::{
    EventId, IntegrationId, PlatformId, REPLY_PARENT_FIELD, WHISPER_RECIPIENT_FIELD,
    requested_chat_target, whispers_unsupported_reason,
};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::donation_factories::{wire_donatello, wire_monobank};
use crate::donation_services::{DonationServices, restore_donatello_poll_interval};
use crate::hotkey_sync::HotkeyReconciler;
use crate::integration_factories::{
    wire_discord, wire_hotkey, wire_kick, wire_midi, wire_obs, wire_twitch, wire_vtube,
    wire_youtube,
};
use crate::integration_supervisor::{IntegrationFactory, TaskGroup};

const CHAT_SEND_QUEUE_CAPACITY: usize = 64;

#[derive(Clone)]
#[allow(dead_code)]
pub struct BuiltinObject {
    pub icon: SectionIcon,
    pub status: Arc<dyn BuiltinStatus>,
    pub health: Arc<dyn BuiltinHealth>,
    pub content: Arc<dyn BuiltinContent>,
    pub quick: Arc<dyn QuickActions>,
    pub control: Option<Arc<dyn BuiltinControl>>,
    pub collections: Option<Arc<dyn BuiltinCollections>>,
    pub obs_client: Option<Arc<forge_obs::ObsClient>>,
    pub vtube_client: Option<Arc<forge_vtube::VTubeClient>>,
}

#[derive(Clone, Default)]
pub struct BuiltinRegistry {
    entries: Arc<std::sync::RwLock<HashMap<IntegrationId, BuiltinObject>>>,
}

impl BuiltinRegistry {
    pub fn get(&self, id: &IntegrationId) -> Option<BuiltinObject> {
        let guard = self.entries.read().unwrap_or_else(|e| e.into_inner());
        guard.get(id).cloned()
    }

    pub fn snapshot(&self) -> Vec<BuiltinObject> {
        let guard = self.entries.read().unwrap_or_else(|e| e.into_inner());
        guard.values().cloned().collect()
    }

    pub fn install(&self, object: BuiltinObject) {
        let id = object.status.id().clone();
        let mut guard = self.entries.write().unwrap_or_else(|e| e.into_inner());
        guard.insert(id, object);
    }

    pub fn remove(&self, id: &IntegrationId) {
        let mut guard = self.entries.write().unwrap_or_else(|e| e.into_inner());
        guard.remove(id);
    }
}

pub struct Integrations {
    pub builtins: BuiltinRegistry,
    pub factories: Vec<Arc<dyn IntegrationFactory>>,
    pub obs_install_seed: ObsInstallSeed,
    pub vtube_install_seed: VTubeInstallSeed,
    pub discord_client: Arc<forge_discord::DiscordClient>,
    pub midi_sink: Arc<forge_midi::SwitchableMidiSink>,
    pub hotkey_client: Option<Arc<forge_hotkey::HotkeyClient>>,
    pub hotkey_reconciler: Option<Arc<HotkeyReconciler>>,
    pub donation_services: DonationServices,
}

#[derive(Clone)]
pub struct ObsInstallSeed {
    sink: Arc<forge_obs::SwitchableObsSink>,
    live: Arc<std::sync::RwLock<Option<Arc<forge_obs::ObsClient>>>>,
}

impl ObsInstallSeed {
    pub(crate) fn new(sink: Arc<forge_obs::SwitchableObsSink>) -> Self {
        Self {
            sink,
            live: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    pub fn stream_output(&self) -> forge_obs::StreamOutputActive {
        self.sink.stream_output()
    }

    pub fn install(&self, client: Arc<forge_obs::ObsClient>) {
        self.sink.install(Arc::clone(&client));
        let mut guard = self.live.write().unwrap_or_else(|e| e.into_inner());
        *guard = Some(client);
    }

    pub fn clear(&self) {
        let mut guard = self.live.write().unwrap_or_else(|e| e.into_inner());
        *guard = None;
    }

    pub async fn disconnect_live(&self) {
        let previous = {
            let mut guard = self.live.write().unwrap_or_else(|e| e.into_inner());
            guard.take()
        };
        if let Some(client) = previous {
            let _ = client.disconnect().await;
        }
    }

    pub fn live(&self) -> Option<Arc<forge_obs::ObsClient>> {
        let guard = self.live.read().unwrap_or_else(|e| e.into_inner());
        guard.clone()
    }
}

#[derive(Clone)]
pub struct VTubeInstallSeed {
    sink: Arc<forge_vtube::SwitchableVTubeSink>,
    live: Arc<std::sync::RwLock<Option<Arc<forge_vtube::VTubeClient>>>>,
}

impl VTubeInstallSeed {
    pub(crate) fn new(sink: Arc<forge_vtube::SwitchableVTubeSink>) -> Self {
        Self {
            sink,
            live: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    pub fn install(&self, client: Arc<forge_vtube::VTubeClient>) {
        self.sink.install(Arc::clone(&client));
        let mut guard = self.live.write().unwrap_or_else(|e| e.into_inner());
        *guard = Some(client);
    }

    pub fn clear(&self) {
        let mut guard = self.live.write().unwrap_or_else(|e| e.into_inner());
        *guard = None;
    }

    pub async fn disconnect_live(&self) {
        let previous = {
            let mut guard = self.live.write().unwrap_or_else(|e| e.into_inner());
            guard.take()
        };
        if let Some(client) = previous {
            let _ = client.disconnect().await;
        }
    }

    pub fn live(&self) -> Option<Arc<forge_vtube::VTubeClient>> {
        let guard = self.live.read().unwrap_or_else(|e| e.into_inner());
        guard.clone()
    }
}

pub fn vtube_builtin_object(client: Arc<forge_vtube::VTubeClient>) -> BuiltinObject {
    BuiltinObject {
        icon: SectionIcon::new("mood-smile"),
        status: client.clone(),
        health: client.clone(),
        content: client.clone(),
        quick: client.clone(),
        control: Some(Arc::clone(&client) as Arc<dyn BuiltinControl>),
        collections: None,
        obs_client: None,
        vtube_client: Some(client),
    }
}

pub fn obs_builtin_object(client: Arc<forge_obs::ObsClient>) -> BuiltinObject {
    BuiltinObject {
        icon: SectionIcon::new("broadcast"),
        status: client.clone(),
        health: client.clone(),
        content: client.clone(),
        quick: client.clone(),
        control: Some(Arc::clone(&client) as Arc<dyn BuiltinControl>),
        collections: None,
        obs_client: Some(client),
        vtube_client: None,
    }
}

pub fn twitch_builtin_object(
    bundle: Arc<forge_platform_twitch::TwitchIntegrationBundle>,
) -> BuiltinObject {
    BuiltinObject {
        icon: SectionIcon::new("brand-twitch"),
        status: bundle.clone(),
        health: bundle.clone(),
        content: bundle.clone(),
        quick: bundle.clone(),
        control: Some(Arc::clone(&bundle) as Arc<dyn BuiltinControl>),
        collections: Some(bundle as Arc<dyn BuiltinCollections>),
        obs_client: None,
        vtube_client: None,
    }
}

pub fn youtube_builtin_object(
    bundle: Arc<forge_platform_youtube::YoutubeIntegrationBundle>,
) -> BuiltinObject {
    BuiltinObject {
        icon: SectionIcon::new("brand-youtube"),
        status: bundle.clone(),
        health: bundle.clone(),
        content: bundle.clone(),
        quick: bundle.clone(),
        control: Some(bundle as Arc<dyn BuiltinControl>),
        collections: None,
        obs_client: None,
        vtube_client: None,
    }
}

pub fn kick_builtin_object(
    bundle: Arc<forge_platform_kick::KickIntegrationBundle>,
) -> BuiltinObject {
    BuiltinObject {
        icon: SectionIcon::new("brand-kick"),
        status: bundle.clone(),
        health: bundle.clone(),
        content: bundle.clone(),
        quick: bundle.clone(),
        control: Some(bundle as Arc<dyn BuiltinControl>),
        collections: None,
        obs_client: None,
        vtube_client: None,
    }
}

pub async fn build_integrations(
    sub_actions: &mut SubActionRegistry,
    triggers: &mut TriggerRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
    endpoints: &PlatformEndpoints,
    donations: &Arc<DonationIngest>,
    hotkey_main_thread: forge_hotkey::MainThreadLink,
) -> Integrations {
    register_platform_triggers(triggers);
    let midi_kinds = kinds_owned_by(triggers, &forge_midi::MIDI_INTEGRATION.id);

    let mut factories: Vec<Arc<dyn IntegrationFactory>> = Vec::new();
    if let Some(twitch) = wire_twitch(sub_actions, backend, bus, endpoints) {
        factories.push(Arc::new(twitch));
    }
    let obs = wire_obs(sub_actions, backend, bus);
    let obs_install_seed = obs.seed();
    factories.push(Arc::new(obs));
    let vtube = wire_vtube(sub_actions, backend, bus);
    let vtube_install_seed = vtube.seed();
    factories.push(Arc::new(vtube));
    let discord = wire_discord(sub_actions, backend, bus);
    let discord_client = discord.client();
    factories.push(Arc::new(discord));
    let midi = wire_midi(sub_actions, backend, bus, midi_kinds);
    let midi_sink = midi.sink();
    factories.push(Arc::new(midi));
    let (hotkey, hotkey_reconciler) = wire_hotkey(backend, bus, hotkey_main_thread).await;
    let hotkey_client = hotkey.client();
    factories.push(Arc::new(hotkey));
    if let Some(youtube) = wire_youtube(sub_actions, backend, bus, endpoints) {
        factories.push(Arc::new(youtube));
    }
    if let Some(kick) = wire_kick(sub_actions, backend, bus, endpoints) {
        factories.push(Arc::new(kick));
    }
    let mut donation_services = DonationServices::default();
    if let Some(donatello) = wire_donatello(backend, endpoints, donations) {
        let provider = donatello.provider();
        restore_donatello_poll_interval(&provider, backend.as_ref()).await;
        donation_services.donatello = Some(provider);
        factories.push(Arc::new(donatello));
    }
    if let Some(monobank) = wire_monobank(backend, endpoints, donations) {
        donation_services.monobank = Some(monobank.provider());
        factories.push(Arc::new(monobank));
    }

    Integrations {
        builtins: BuiltinRegistry::default(),
        factories,
        obs_install_seed,
        vtube_install_seed,
        discord_client,
        midi_sink,
        hotkey_client: Some(hotkey_client),
        hotkey_reconciler: Some(hotkey_reconciler),
        donation_services,
    }
}

fn kinds_owned_by(triggers: &TriggerRegistry, owner: &IntegrationId) -> Vec<String> {
    triggers
        .all()
        .map(|descriptor| descriptor.id())
        .filter(|kind| triggers.owning_integration(kind) == Some(owner))
        .map(ToOwned::to_owned)
        .collect()
}

fn register_platform_triggers(triggers: &mut TriggerRegistry) {
    if let Err(e) = forge_platform_twitch::register_twitch_triggers(triggers) {
        eprintln!("forge-desktop: twitch trigger registration failed: {e}");
    }
    if let Err(e) = forge_obs::register_obs_triggers(triggers) {
        eprintln!("forge-desktop: obs trigger registration failed: {e}");
    }
    if let Err(e) = forge_vtube::register_vtube_triggers(triggers) {
        eprintln!("forge-desktop: vtube trigger registration failed: {e}");
    }
    if let Err(e) = forge_midi::register_midi_triggers(triggers) {
        eprintln!("forge-desktop: midi trigger registration failed: {e}");
    }
    if let Err(e) = forge_hotkey::register_hotkey_triggers(triggers) {
        eprintln!("forge-desktop: hotkey trigger registration failed: {e}");
    }
    if let Err(e) = forge_platform_youtube::register_youtube_triggers(triggers) {
        eprintln!("forge-desktop: youtube trigger registration failed: {e}");
    }
    if let Err(e) = forge_platform_kick::register_kick_triggers(triggers) {
        eprintln!("forge-desktop: kick trigger registration failed: {e}");
    }
}

pub(crate) fn publisher(bus: &Arc<EventBus>) -> Arc<dyn EventPublisher> {
    Arc::clone(bus) as Arc<dyn EventPublisher>
}

pub(crate) fn creds_of(backend: &Arc<dyn DataProvider>) -> Arc<dyn CredentialsRepo> {
    Arc::clone(backend) as Arc<dyn CredentialsRepo>
}

pub(crate) fn spawn_event_bridge(
    bus: Arc<dyn EventPublisher>,
    mut events: EventStream,
    label: &'static str,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => bus.publish(event),
                Err(EventsError::BusClosed) => break,
                Err(EventsError::LaggingReceiver) => {
                    eprintln!("forge-desktop: {label} event bridge lagging");
                    continue;
                }
                Err(_) => continue,
            }
        }
    })
}

enum ChatSendIntent {
    Plain,
    Reply { parent_message_id: String },
    Whisper { recipient_login: String },
}

impl ChatSendIntent {
    fn of(payload: &serde_json::Value) -> Self {
        if let Some(recipient) = payload.get(WHISPER_RECIPIENT_FIELD) {
            return Self::Whisper {
                recipient_login: recipient.as_str().unwrap_or_default().trim().to_owned(),
            };
        }
        match payload
            .get(REPLY_PARENT_FIELD)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|parent| !parent.is_empty())
        {
            Some(parent) => Self::Reply {
                parent_message_id: parent.to_owned(),
            },
            None => Self::Plain,
        }
    }
}

struct ChatSendRequest {
    message: String,
    intent: ChatSendIntent,
    caused_by: EventId,
}

fn chat_send_failed(source: EventSource, target: &str, error: String, caused_by: EventId) -> Event {
    Event::caused_by(
        source,
        "chat.send.failed",
        serde_json::json!({ "channel": target, "error": error }),
        caused_by,
    )
}

fn spawn_chat_send_worker(
    bus: Arc<EventBus>,
    platform: Arc<dyn ChatPlatform>,
    target: &'static str,
    source: EventSource,
    mut requests: mpsc::Receiver<ChatSendRequest>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let ChatSendRequest {
                message,
                intent,
                caused_by,
            } = request;
            let (outcome, delivered) = match intent {
                ChatSendIntent::Plain => (
                    platform.send_message(target, &message).await,
                    Event::caused_by(
                        source,
                        "chat.send",
                        serde_json::json!({ "channel": target, "message": message }),
                        caused_by,
                    ),
                ),
                ChatSendIntent::Reply { parent_message_id } => (
                    platform
                        .send_reply(target, &parent_message_id, &message)
                        .await,
                    Event::caused_by(
                        source,
                        "chat.send",
                        serde_json::json!({
                            "channel": target,
                            "message": message,
                            (REPLY_PARENT_FIELD): parent_message_id,
                        }),
                        caused_by,
                    ),
                ),
                ChatSendIntent::Whisper { recipient_login } => (
                    platform.send_whisper(&recipient_login, &message).await,
                    Event::caused_by(
                        source,
                        "chat.whisper.sent",
                        serde_json::json!({
                            "channel": target,
                            (WHISPER_RECIPIENT_FIELD): recipient_login,
                        }),
                        caused_by,
                    ),
                ),
            };
            match outcome {
                Ok(()) => bus.publish(delivered),
                Err(e) => bus.publish(chat_send_failed(source, target, e.to_string(), caused_by)),
            }
        }
    })
}

pub(crate) fn spawn_chat_send_bridge(
    bus: Arc<EventBus>,
    platform: Arc<dyn ChatPlatform>,
    target: &'static str,
    source: EventSource,
) -> TaskGroup {
    let (tx, rx) = mpsc::channel(CHAT_SEND_QUEUE_CAPACITY);
    let mut tasks = TaskGroup::default();
    tasks.track(spawn_chat_send_worker(
        Arc::clone(&bus),
        platform,
        target,
        source,
        rx,
    ));

    let whispers_supported =
        PlatformId::from_wire(target).is_some_and(PlatformId::supports_whispers);
    tasks.track(tokio::spawn(async move {
        let mut sub = bus.subscribe();
        let mut lag_count: u64 = 0;
        let mut lagging = false;
        loop {
            let event = match sub.recv().await {
                Ok(e) => {
                    lagging = false;
                    e
                }
                Err(EventsError::BusClosed) => break,
                Err(EventsError::LaggingReceiver) if lagging => continue,
                Err(EventsError::LaggingReceiver) => {
                    lagging = true;
                    lag_count += 1;
                    eprintln!(
                        "forge-desktop: WARN chat send bridge for {target} lagging; {lag_count} lag event(s) observed since bridge start"
                    );
                    bus.publish(Event::new(
                        source,
                        "chat.send.failed",
                        serde_json::json!({
                            "channel": target,
                            "error": format!(
                                "event bus receiver lagging ({lag_count} lag event(s) observed); chat.send.request events may have been dropped"
                            ),
                        }),
                    ));
                    continue;
                }
                Err(_) => continue,
            };
            if event.kind != "chat.send.request" {
                continue;
            }
            if !matches!(event.source, EventSource::Core | EventSource::Rhai) {
                continue;
            }
            let requested = event
                .payload
                .get("target")
                .and_then(|v| v.as_str())
                .and_then(requested_chat_target);
            if requested.is_some_and(|requested| requested != target) {
                continue;
            }
            let Some(message) = event
                .payload
                .get("message")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned)
            else {
                continue;
            };
            let caused_by = event.id;
            let intent = ChatSendIntent::of(&event.payload);
            if matches!(intent, ChatSendIntent::Whisper { .. }) && !whispers_supported {
                if requested.is_some() {
                    bus.publish(chat_send_failed(
                        source,
                        target,
                        whispers_unsupported_reason(target),
                        caused_by,
                    ));
                }
                continue;
            }
            let request = ChatSendRequest {
                message,
                intent,
                caused_by,
            };
            if tx.try_send(request).is_err() {
                bus.publish(chat_send_failed(
                    source,
                    target,
                    "chat send queue full or worker unavailable; request dropped".to_owned(),
                    caused_by,
                ));
            }
        }
    }));
    tasks
}

pub(crate) fn spawn_connect(
    platform: Arc<dyn ChatPlatform>,
    label: &'static str,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(e) = platform.connect().await {
            eprintln!("forge-desktop: {label} connect failed: {e}");
        }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use forge_events::EventStream;
    use forge_platform_core::{AuthFlow, ConnectionState, PlatformCapabilities};
    use forge_platform_core::{PlatformError, RateLimitOutcome, RateLimiter};
    use forge_runtime::NullEventLogRepo;
    use std::time::Duration;
    use tokio::sync::{Semaphore, broadcast, mpsc};

    struct NoopRateLimiter;

    #[async_trait::async_trait]
    impl RateLimiter for NoopRateLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
            Ok(RateLimitOutcome::Granted)
        }

        fn remaining(&self) -> u32 {
            u32::MAX
        }

        async fn observe_remote_throttle(&self, _retry_after: Duration) {}
    }

    #[derive(Debug, PartialEq)]
    enum PlatformCall {
        Message(String, String),
        Reply {
            channel: String,
            parent: String,
            text: String,
        },
        Whisper {
            recipient: String,
            text: String,
        },
    }

    struct RecordingPlatform {
        sends: mpsc::UnboundedSender<PlatformCall>,
        gate: Option<Arc<Semaphore>>,
        failure: Option<String>,
        auth: AuthFlow,
        caps: PlatformCapabilities,
    }

    impl RecordingPlatform {
        fn spawn() -> (Arc<Self>, mpsc::UnboundedReceiver<PlatformCall>) {
            Self::build(None, None)
        }

        fn gated() -> (
            Arc<Self>,
            mpsc::UnboundedReceiver<PlatformCall>,
            Arc<Semaphore>,
        ) {
            let gate = Arc::new(Semaphore::new(0));
            let (platform, rx) = Self::build(Some(Arc::clone(&gate)), None);
            (platform, rx, gate)
        }

        fn failing(reason: &str) -> (Arc<Self>, mpsc::UnboundedReceiver<PlatformCall>) {
            Self::build(None, Some(reason.to_owned()))
        }

        fn build(
            gate: Option<Arc<Semaphore>>,
            failure: Option<String>,
        ) -> (Arc<Self>, mpsc::UnboundedReceiver<PlatformCall>) {
            let (tx, rx) = mpsc::unbounded_channel();
            let platform = Arc::new(Self {
                sends: tx,
                gate,
                failure,
                auth: AuthFlow::None {
                    reason: String::new(),
                },
                caps: PlatformCapabilities {
                    can_send_chat: true,
                    can_moderate: false,
                    can_subscribe_events: false,
                    can_polls: false,
                    can_predictions: false,
                    can_channel_points: false,
                    limited: false,
                    limited_reason: None,
                },
            });
            (platform, rx)
        }
    }

    impl RecordingPlatform {
        fn outcome(&self) -> Result<(), PlatformError> {
            match &self.failure {
                Some(reason) => Err(PlatformError::Network {
                    reason: reason.clone(),
                }),
                None => Ok(()),
            }
        }
    }

    #[async_trait::async_trait]
    impl ChatPlatform for RecordingPlatform {
        fn platform_id(&self) -> &'static str {
            "mock"
        }
        fn auth_flow(&self) -> &AuthFlow {
            &self.auth
        }
        fn capabilities(&self) -> &PlatformCapabilities {
            &self.caps
        }
        fn connection_state(&self) -> ConnectionState {
            ConnectionState::Connected
        }
        async fn connect(&self) -> Result<(), PlatformError> {
            Ok(())
        }
        async fn disconnect(&self) -> Result<(), PlatformError> {
            Ok(())
        }
        async fn send_message(&self, channel: &str, text: &str) -> Result<(), PlatformError> {
            let _ = self
                .sends
                .send(PlatformCall::Message(channel.to_string(), text.to_string()));
            if let Some(gate) = &self.gate {
                gate.acquire().await.unwrap().forget();
            }
            self.outcome()
        }
        async fn send_reply(
            &self,
            channel: &str,
            reply_parent_message_id: &str,
            text: &str,
        ) -> Result<(), PlatformError> {
            let _ = self.sends.send(PlatformCall::Reply {
                channel: channel.to_string(),
                parent: reply_parent_message_id.to_string(),
                text: text.to_string(),
            });
            self.outcome()
        }
        async fn send_whisper(
            &self,
            recipient_login: &str,
            text: &str,
        ) -> Result<(), PlatformError> {
            let _ = self.sends.send(PlatformCall::Whisper {
                recipient: recipient_login.to_string(),
                text: text.to_string(),
            });
            self.outcome()
        }
        fn events(&self) -> EventStream {
            EventStream::new(broadcast::channel(1).1)
        }
    }

    fn test_bus() -> Arc<EventBus> {
        EventBus::new(Arc::new(NullEventLogRepo))
    }

    fn request(source: EventSource, payload: serde_json::Value) -> Event {
        Event::new(source, "chat.send.request", payload)
    }

    async fn expect_call(rx: &mut mpsc::UnboundedReceiver<PlatformCall>) -> PlatformCall {
        tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("a platform send call was expected but never arrived")
            .expect("mock send channel closed")
    }

    async fn expect_send(rx: &mut mpsc::UnboundedReceiver<PlatformCall>) -> (String, String) {
        match expect_call(rx).await {
            PlatformCall::Message(channel, text) => (channel, text),
            other => panic!("expected a plain send_message call, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rhai_broadcast_request_reaches_every_platform_bridge() {
        let bus = test_bus();
        let (twitch, mut twitch_rx) = RecordingPlatform::spawn();
        let (kick, mut kick_rx) = RecordingPlatform::spawn();
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        spawn_chat_send_bridge(Arc::clone(&bus), kick, "kick", EventSource::Kick);
        tokio::task::yield_now().await;

        bus.publish(request(
            EventSource::Rhai,
            serde_json::json!({ "message": "hello" }),
        ));

        assert_eq!(
            expect_send(&mut twitch_rx).await,
            ("twitch".to_string(), "hello".to_string())
        );
        assert_eq!(
            expect_send(&mut kick_rx).await,
            ("kick".to_string(), "hello".to_string())
        );
    }

    #[tokio::test]
    async fn rhai_targeted_request_routes_only_to_named_platform() {
        let bus = test_bus();
        let (twitch, mut twitch_rx) = RecordingPlatform::spawn();
        let (kick, mut kick_rx) = RecordingPlatform::spawn();
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        spawn_chat_send_bridge(Arc::clone(&bus), kick, "kick", EventSource::Kick);
        tokio::task::yield_now().await;

        bus.publish(request(
            EventSource::Rhai,
            serde_json::json!({ "target": "twitch", "message": "for-twitch" }),
        ));
        bus.publish(request(
            EventSource::Rhai,
            serde_json::json!({ "message": "sentinel" }),
        ));

        assert_eq!(
            expect_send(&mut twitch_rx).await,
            ("twitch".to_string(), "for-twitch".to_string())
        );
        assert_eq!(
            expect_send(&mut kick_rx).await,
            ("kick".to_string(), "sentinel".to_string()),
            "kick must skip the twitch-targeted request and only deliver the broadcast sentinel"
        );
    }

    #[tokio::test]
    async fn padded_target_routes_only_to_the_named_platform() {
        let bus = test_bus();
        let (twitch, mut twitch_rx) = RecordingPlatform::spawn();
        let (kick, mut kick_rx) = RecordingPlatform::spawn();
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        spawn_chat_send_bridge(Arc::clone(&bus), kick, "kick", EventSource::Kick);
        tokio::task::yield_now().await;

        bus.publish(request(
            EventSource::Rhai,
            serde_json::json!({ "target": " twitch ", "message": "padded" }),
        ));
        bus.publish(request(
            EventSource::Rhai,
            serde_json::json!({ "target": "kick", "message": "sentinel" }),
        ));

        assert_eq!(
            expect_send(&mut twitch_rx).await,
            ("twitch".to_string(), "padded".to_string())
        );
        assert_eq!(
            expect_send(&mut kick_rx).await,
            ("kick".to_string(), "sentinel".to_string()),
            "kick must skip the request padded-targeted at twitch"
        );
    }

    #[tokio::test]
    async fn blank_target_request_reaches_every_platform_bridge() {
        for blank in ["", "   "] {
            let bus = test_bus();
            let (twitch, mut twitch_rx) = RecordingPlatform::spawn();
            let (kick, mut kick_rx) = RecordingPlatform::spawn();
            spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
            spawn_chat_send_bridge(Arc::clone(&bus), kick, "kick", EventSource::Kick);
            tokio::task::yield_now().await;

            bus.publish(request(
                EventSource::Core,
                serde_json::json!({ "target": blank, "message": "everyone" }),
            ));

            assert_eq!(
                expect_send(&mut twitch_rx).await,
                ("twitch".to_string(), "everyone".to_string()),
                "target {blank:?}"
            );
            assert_eq!(
                expect_send(&mut kick_rx).await,
                ("kick".to_string(), "everyone".to_string()),
                "target {blank:?}"
            );
        }
    }

    #[tokio::test]
    async fn core_sourced_targeted_request_is_still_delivered() {
        let bus = test_bus();
        let (twitch, mut twitch_rx) = RecordingPlatform::spawn();
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        tokio::task::yield_now().await;

        bus.publish(request(
            EventSource::Core,
            serde_json::json!({ "target": "twitch", "message": "from-core" }),
        ));

        assert_eq!(
            expect_send(&mut twitch_rx).await,
            ("twitch".to_string(), "from-core".to_string())
        );
    }

    #[tokio::test]
    async fn non_core_or_rhai_source_request_is_ignored() {
        let bus = test_bus();
        let (twitch, mut twitch_rx) = RecordingPlatform::spawn();
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        tokio::task::yield_now().await;

        bus.publish(request(
            EventSource::Twitch,
            serde_json::json!({ "message": "loop-risk" }),
        ));
        bus.publish(request(
            EventSource::Rhai,
            serde_json::json!({ "message": "sentinel" }),
        ));

        assert_eq!(
            expect_send(&mut twitch_rx).await,
            ("twitch".to_string(), "sentinel".to_string()),
            "a platform-sourced request must be ignored so bridges cannot re-enter"
        );
    }

    async fn next_of_kind(sub: &mut forge_runtime::EventSubscription, kind: &str) -> Event {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let event = sub.recv().await.expect("observer subscription failed");
                if event.kind == kind {
                    return event;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("no {kind} event was published"))
    }

    fn drain_of_kind(sub: &mut forge_runtime::EventSubscription, kind: &str) -> Vec<Event> {
        let mut seen = Vec::new();
        while let Some(event) = sub.try_recv().expect("observer subscription failed") {
            if event.kind == kind {
                seen.push(event);
            }
        }
        seen
    }

    #[tokio::test]
    async fn successful_send_publishes_chat_send_caused_by_the_request() {
        let bus = test_bus();
        let mut observer = bus.subscribe();
        let (twitch, mut twitch_rx) = RecordingPlatform::spawn();
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        tokio::task::yield_now().await;
        let req = request(EventSource::Rhai, serde_json::json!({ "message": "hi" }));
        let req_id = req.id;

        bus.publish(req);
        expect_send(&mut twitch_rx).await;

        let sent = next_of_kind(&mut observer, "chat.send").await;
        assert_eq!(sent.caused_by, Some(req_id));
        assert_eq!(sent.payload["message"], "hi");
    }

    #[tokio::test]
    async fn platform_send_error_publishes_failure_with_its_text_caused_by_the_request() {
        let bus = test_bus();
        let mut observer = bus.subscribe();
        let (twitch, _twitch_rx) = RecordingPlatform::failing("msg_duplicate: identical message");
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        tokio::task::yield_now().await;
        let req = request(EventSource::Rhai, serde_json::json!({ "message": "hi" }));
        let req_id = req.id;

        bus.publish(req);

        let failed = next_of_kind(&mut observer, "chat.send.failed").await;
        assert_eq!(failed.caused_by, Some(req_id));
        assert!(
            failed.payload["error"]
                .as_str()
                .unwrap()
                .contains("msg_duplicate: identical message"),
            "got {}",
            failed.payload
        );
    }

    #[tokio::test]
    async fn requests_are_sent_one_at_a_time_in_publish_order() {
        let bus = test_bus();
        let (twitch, mut twitch_rx, gate) = RecordingPlatform::gated();
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        tokio::task::yield_now().await;

        for message in ["first", "second", "third"] {
            bus.publish(request(
                EventSource::Rhai,
                serde_json::json!({ "message": message }),
            ));
        }
        assert_eq!(expect_send(&mut twitch_rx).await.1, "first");
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        assert!(
            twitch_rx.try_recv().is_err(),
            "a second send started while the first was still in flight"
        );
        gate.add_permits(3);

        assert_eq!(expect_send(&mut twitch_rx).await.1, "second");
        assert_eq!(expect_send(&mut twitch_rx).await.1, "third");
    }

    #[tokio::test]
    async fn blocked_send_queues_up_to_capacity_and_reports_the_first_overflow_request() {
        let bus = test_bus();
        let mut observer = bus.subscribe();
        let (twitch, mut twitch_rx, _gate) = RecordingPlatform::gated();
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        tokio::task::yield_now().await;
        bus.publish(request(
            EventSource::Rhai,
            serde_json::json!({ "message": "in-flight" }),
        ));
        expect_send(&mut twitch_rx).await;

        for n in 0..CHAT_SEND_QUEUE_CAPACITY {
            bus.publish(request(
                EventSource::Rhai,
                serde_json::json!({ "message": format!("queued-{n}") }),
            ));
        }
        let overflow = request(
            EventSource::Rhai,
            serde_json::json!({ "message": "overflow" }),
        );
        let overflow_id = overflow.id;
        bus.publish(overflow);

        let failed = next_of_kind(&mut observer, "chat.send.failed").await;
        assert_eq!(
            failed.caused_by,
            Some(overflow_id),
            "the reader must keep accepting while a send blocks, rejecting only past capacity"
        );
    }

    #[tokio::test]
    async fn lagged_bus_reader_publishes_one_failure_naming_the_lag_and_keeps_serving() {
        const OBSERVER_CAPACITY: usize = 64;
        const FLOOD: usize = OBSERVER_CAPACITY * 2;
        let config = forge_runtime::Config {
            bus_observer_capacity: OBSERVER_CAPACITY,
            ..forge_runtime::Config::default()
        };
        let bus = EventBus::with_config(Arc::new(NullEventLogRepo), &config);
        let (twitch, mut twitch_rx) = RecordingPlatform::spawn();
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        tokio::task::yield_now().await;
        for _ in 0..FLOOD {
            bus.publish(Event::new(
                EventSource::Core,
                "noise",
                serde_json::Value::Null,
            ));
        }
        let mut observer = bus.subscribe();

        bus.publish(request(
            EventSource::Rhai,
            serde_json::json!({ "message": "after-lag" }),
        ));
        assert_eq!(expect_send(&mut twitch_rx).await.1, "after-lag");

        let failures = drain_of_kind(&mut observer, "chat.send.failed");
        assert_eq!(failures.len(), 1, "got {failures:?}");
        assert!(
            failures[0].payload["error"]
                .as_str()
                .unwrap()
                .contains("lagging"),
            "got {}",
            failures[0].payload
        );
    }

    fn whisper_request(target: Option<&str>, recipient: &str, message: &str) -> Event {
        let mut payload = serde_json::json!({
            "message": message,
            WHISPER_RECIPIENT_FIELD: recipient,
        });
        if let Some(target) = target {
            payload["target"] = target.into();
        }
        request(EventSource::Rhai, payload)
    }

    #[tokio::test]
    async fn whisper_request_calls_send_whisper_with_the_trimmed_recipient() {
        let bus = test_bus();
        let (twitch, mut twitch_rx) = RecordingPlatform::spawn();
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        tokio::task::yield_now().await;

        bus.publish(whisper_request(None, " viewer ", "psst"));

        assert_eq!(
            expect_call(&mut twitch_rx).await,
            PlatformCall::Whisper {
                recipient: "viewer".to_owned(),
                text: "psst".to_owned(),
            }
        );
    }

    #[tokio::test]
    async fn whisper_request_never_reaches_public_chat() {
        let bus = test_bus();
        let (twitch, mut twitch_rx) = RecordingPlatform::failing("missing whisper scope");
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        tokio::task::yield_now().await;

        bus.publish(whisper_request(Some("twitch"), "viewer", "psst"));
        bus.publish(request(
            EventSource::Rhai,
            serde_json::json!({ "message": "sentinel" }),
        ));

        assert!(matches!(
            expect_call(&mut twitch_rx).await,
            PlatformCall::Whisper { .. }
        ));
        assert_eq!(
            expect_send(&mut twitch_rx).await.1,
            "sentinel",
            "a failed whisper must not be retried as a public message"
        );
    }

    #[tokio::test]
    async fn targeted_whisper_on_a_platform_without_whispers_fails_unsent() {
        let bus = test_bus();
        let mut observer = bus.subscribe();
        let (kick, mut kick_rx) = RecordingPlatform::spawn();
        spawn_chat_send_bridge(Arc::clone(&bus), kick, "kick", EventSource::Kick);
        tokio::task::yield_now().await;
        let req = whisper_request(Some("kick"), "viewer", "psst");
        let req_id = req.id;

        bus.publish(req);
        bus.publish(request(
            EventSource::Rhai,
            serde_json::json!({ "message": "sentinel" }),
        ));

        let failed = next_of_kind(&mut observer, "chat.send.failed").await;
        assert_eq!(failed.caused_by, Some(req_id));
        assert_eq!(failed.payload["error"], whispers_unsupported_reason("kick"));
        assert_eq!(
            expect_send(&mut kick_rx).await.1,
            "sentinel",
            "kick must not deliver the whisper in any form"
        );
    }

    #[tokio::test]
    async fn broadcast_whisper_goes_to_twitch_while_kick_stays_silent() {
        let bus = test_bus();
        let mut observer = bus.subscribe();
        let (twitch, mut twitch_rx) = RecordingPlatform::spawn();
        let (kick, mut kick_rx) = RecordingPlatform::spawn();
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        spawn_chat_send_bridge(Arc::clone(&bus), kick, "kick", EventSource::Kick);
        tokio::task::yield_now().await;

        bus.publish(whisper_request(None, "viewer", "psst"));
        bus.publish(request(
            EventSource::Rhai,
            serde_json::json!({ "target": "kick", "message": "sentinel" }),
        ));

        assert!(matches!(
            expect_call(&mut twitch_rx).await,
            PlatformCall::Whisper { .. }
        ));
        assert_eq!(expect_send(&mut kick_rx).await.1, "sentinel");
        let failures = drain_of_kind(&mut observer, "chat.send.failed");
        assert!(
            failures.is_empty(),
            "a broadcast whisper must not fail on kick: {failures:?}"
        );
    }

    #[tokio::test]
    async fn delivered_whisper_publishes_whisper_sent_without_the_message_text() {
        let bus = test_bus();
        let mut observer = bus.subscribe();
        let (twitch, mut twitch_rx) = RecordingPlatform::spawn();
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        tokio::task::yield_now().await;
        let req = whisper_request(None, "viewer", "WHISPER_TEXT_SENTINEL");
        let req_id = req.id;

        bus.publish(req);
        expect_call(&mut twitch_rx).await;

        let sent = next_of_kind(&mut observer, "chat.whisper.sent").await;
        assert_eq!(sent.caused_by, Some(req_id));
        assert_eq!(sent.payload[WHISPER_RECIPIENT_FIELD], "viewer");
        assert!(
            !sent.payload.to_string().contains("WHISPER_TEXT_SENTINEL"),
            "whisper text must stay private: {}",
            sent.payload
        );
    }

    #[tokio::test]
    async fn reply_request_calls_send_reply_with_the_parent_id() {
        let bus = test_bus();
        let (twitch, mut twitch_rx) = RecordingPlatform::spawn();
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        tokio::task::yield_now().await;

        bus.publish(request(
            EventSource::Rhai,
            serde_json::json!({ "message": "hello", REPLY_PARENT_FIELD: " msg-1 " }),
        ));

        assert_eq!(
            expect_call(&mut twitch_rx).await,
            PlatformCall::Reply {
                channel: "twitch".to_owned(),
                parent: "msg-1".to_owned(),
                text: "hello".to_owned(),
            }
        );
    }

    #[tokio::test]
    async fn reply_request_with_a_blank_parent_is_sent_as_a_plain_message() {
        let bus = test_bus();
        let (twitch, mut twitch_rx) = RecordingPlatform::spawn();
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        tokio::task::yield_now().await;

        for parent in ["", "  "] {
            bus.publish(request(
                EventSource::Rhai,
                serde_json::json!({ "message": "hello", REPLY_PARENT_FIELD: parent }),
            ));

            assert_eq!(
                expect_send(&mut twitch_rx).await,
                ("twitch".to_owned(), "hello".to_owned()),
                "parent {parent:?}"
            );
        }
    }

    #[tokio::test]
    async fn request_carrying_both_keys_is_sent_as_a_whisper() {
        let bus = test_bus();
        let (twitch, mut twitch_rx) = RecordingPlatform::spawn();
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        tokio::task::yield_now().await;

        bus.publish(request(
            EventSource::Rhai,
            serde_json::json!({
                "message": "psst",
                REPLY_PARENT_FIELD: "msg-1",
                WHISPER_RECIPIENT_FIELD: "viewer",
            }),
        ));

        assert!(matches!(
            expect_call(&mut twitch_rx).await,
            PlatformCall::Whisper { .. }
        ));
    }

    struct SilentPublisher;

    impl EventPublisher for SilentPublisher {
        fn publish(&self, _event: Event) {}
    }

    async fn listening_but_silent_port() -> (tokio::net::TcpListener, u16) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    #[tokio::test]
    async fn retiring_the_live_client_empties_the_slot_and_leaves_it_disconnected() {
        let (_listener, port) = listening_but_silent_port().await;
        let seed = ObsInstallSeed::new(forge_obs::SwitchableObsSink::new());
        let client = Arc::new(
            forge_obs::ObsClient::connect(
                &format!("127.0.0.1:{port}"),
                None,
                Arc::new(SilentPublisher),
            )
            .await
            .unwrap(),
        );
        seed.install(Arc::clone(&client));

        seed.disconnect_live().await;

        assert!(
            seed.live().is_none(),
            "a reconnect whose new connect fails must leave no live client behind"
        );
        assert_eq!(
            client.connection_state(),
            ConnectionState::Disconnected,
            "the retired client was released before its supervisor joined"
        );
    }

    mod detail_lifetime {
        use std::sync::Mutex;

        use forge_components::{Density, ThemeId};
        use forge_platform_core::{
            BuiltinContent, BuiltinHealth, BuiltinStatus, CapabilityFlags, ConnectionState,
            DetailSection, HeaderAction, HealthDelta, HealthMetric, HealthStream, HealthValue,
            QuickAction, QuickActions, SectionIcon,
        };
        use forge_runtime::{
            ActionCancelRegistry, spawn_action_engine, spawn_live_viewer_aggregator,
        };
        use forge_types::IntegrationId;
        use gpui::{AppContext as _, TestAppContext};

        use super::super::{BuiltinObject, ObsInstallSeed, VTubeInstallSeed};
        use crate::integration_detail::IntegrationDetail;
        use crate::platforms::PlatformConnectivity;
        use crate::presentation::Presentation;
        use crate::test_support::{StubActions, StubEventLog, StubHistory, runtime, test_backend};
        use forge_registry::{SubActionRegistry, TriggerRegistry};
        use forge_runtime::EventBus;
        use forge_storage::{CredentialsRepo, SettingsRepo};
        use std::sync::Arc;
        use std::time::Duration;
        use tokio::sync::mpsc;

        struct HeldStreamBuiltin {
            id: IntegrationId,
            deltas: Mutex<Option<mpsc::UnboundedReceiver<HealthDelta>>>,
        }

        impl BuiltinStatus for HeldStreamBuiltin {
            fn id(&self) -> &IntegrationId {
                &self.id
            }
            fn display_name(&self) -> &str {
                "OBS"
            }
            fn version(&self) -> Option<&str> {
                None
            }
            fn connection(&self) -> ConnectionState {
                ConnectionState::Connected
            }
            fn uptime(&self) -> Option<Duration> {
                None
            }
            fn endpoint(&self) -> Option<&str> {
                None
            }
            fn capability_flags(&self) -> CapabilityFlags {
                CapabilityFlags {
                    limited: false,
                    label: None,
                }
            }
            fn header_actions(&self) -> Vec<HeaderAction> {
                Vec::new()
            }
        }

        impl BuiltinHealth for HeldStreamBuiltin {
            fn metrics(&self) -> [HealthMetric; 4] {
                std::array::from_fn(|i| HealthMetric {
                    label: format!("metric{i}"),
                    value: HealthValue::Text {
                        primary: String::new(),
                        secondary: None,
                    },
                })
            }
            fn stream(&self) -> HealthStream {
                let rx = self.deltas.lock().unwrap().take().unwrap();
                Box::pin(futures_util::stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|delta| (delta, rx))
                }))
            }
        }

        impl BuiltinContent for HeldStreamBuiltin {
            fn sections(&self) -> Vec<DetailSection> {
                Vec::new()
            }
        }

        impl QuickActions for HeldStreamBuiltin {
            fn actions(&self) -> Vec<QuickAction> {
                Vec::new()
            }
        }

        #[gpui::test]
        fn closing_an_integration_screen_ends_the_task_holding_its_health_stream(
            cx: &mut TestAppContext,
        ) {
            cx.update(|cx| {
                cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            });
            let rt = runtime();
            let _enter = rt.enter();
            let (deltas_tx, deltas_rx) = mpsc::unbounded_channel();
            let probe = Arc::new(HeldStreamBuiltin {
                id: IntegrationId::new("obs"),
                deltas: Mutex::new(Some(deltas_rx)),
            });
            let object = BuiltinObject {
                icon: SectionIcon::new("bug"),
                status: probe.clone(),
                health: probe.clone(),
                content: probe.clone(),
                quick: probe,
                control: None,
                collections: None,
                obs_client: None,
                vtube_client: None,
            };
            let bus = EventBus::new(Arc::new(StubEventLog));
            let engine = spawn_action_engine(
                Arc::clone(&bus),
                crate::test_support::stub_catalog(),
                Arc::new(StubActions),
                Arc::new(StubHistory),
                Arc::new(SubActionRegistry::new()),
                Arc::new(ActionCancelRegistry::new()),
            );
            let (backend, _writes) = test_backend();
            let connectivity = cx.new(|_| PlatformConnectivity::new());
            let view = cx.new(|cx| {
                IntegrationDetail::new(
                    object,
                    rt.handle().clone(),
                    engine,
                    Arc::clone(&backend) as Arc<dyn CredentialsRepo>,
                    backend as Arc<dyn SettingsRepo>,
                    Arc::new(StubHistory),
                    Arc::new(TriggerRegistry::new()),
                    Arc::clone(&bus) as Arc<dyn forge_events::EventPublisher>,
                    Arc::clone(&bus),
                    spawn_live_viewer_aggregator(),
                    None,
                    None,
                    None,
                    ObsInstallSeed::new(forge_obs::SwitchableObsSink::new()),
                    VTubeInstallSeed::new(forge_vtube::SwitchableVTubeSink::new()),
                    connectivity,
                    cx,
                )
            });
            cx.run_until_parked();
            assert!(
                !deltas_tx.is_closed(),
                "the open screen must hold its health stream"
            );

            drop(view);
            cx.update(|_| {});
            cx.run_until_parked();

            assert!(
                deltas_tx.is_closed(),
                "the closed screen's health task must end without waiting for a delta"
            );
        }
    }

    async fn every_registry_entry() -> (TriggerRegistry, SubActionRegistry) {
        let media_root = tempfile::tempdir().unwrap();
        let backend: Arc<dyn DataProvider> = Arc::new(
            forge_storage_sqlite::SqliteBackend::open_for_test(
                "sqlite::memory:",
                [0x11; 32],
                media_root.path().to_owned(),
                None,
            )
            .await
            .unwrap(),
        );
        let bus = test_bus();
        let mut triggers = TriggerRegistry::new();
        forge_runtime::register_core_triggers(&mut triggers).unwrap();
        register_platform_triggers(&mut triggers);

        let mut sub_actions = SubActionRegistry::new();
        forge_runtime::register_core_sub_actions(
            &mut sub_actions,
            Arc::clone(&backend) as Arc<dyn forge_storage::GlobalsRepo>,
            Arc::clone(&backend) as Arc<dyn forge_storage::UserGlobalsRepo>,
            Arc::new(forge_runtime::ScriptRegistry::new()),
            publisher(&bus),
            Arc::clone(&backend) as Arc<dyn forge_storage::SettingsRepo>,
            forge_runtime::SchedulerCell::new(),
            backend.trigger_instance_repo(),
            backend.action_repo(),
            Arc::clone(&backend) as Arc<dyn forge_storage::ScriptRepo>,
            Arc::new(forge_runtime::ActionCancelRegistry::new()),
            forge_runtime::OverlayServiceCell::new(),
            forge_runtime::Config::default(),
        )
        .unwrap();
        wire_obs(&mut sub_actions, &backend, &bus);
        wire_vtube(&mut sub_actions, &backend, &bus);
        wire_discord(&mut sub_actions, &backend, &bus);
        wire_midi(&mut sub_actions, &backend, &bus, Vec::new());
        register_twitch_runners(&mut sub_actions, &backend, &bus);
        register_youtube_runners(&mut sub_actions, &backend);
        register_kick_runners(&mut sub_actions, &backend);
        (triggers, sub_actions)
    }

    fn register_twitch_runners(
        sub_actions: &mut SubActionRegistry,
        backend: &Arc<dyn DataProvider>,
        bus: &Arc<EventBus>,
    ) {
        let creds = creds_of(backend);
        let manager = Arc::new(forge_platform_twitch::TwitchCredentialsManager::new(
            Arc::clone(&creds),
            "test-client".to_owned(),
        ));
        let transport: Arc<dyn forge_platform_twitch::HelixTransport> =
            Arc::new(forge_platform_twitch::HelixHttpTransport::new(
                &PlatformEndpoints::default(),
                Arc::new(NoopRateLimiter),
                publisher(bus),
                "test-client".to_owned(),
                manager as Arc<dyn forge_platform_twitch::HelixTokenSource>,
            ));
        forge_platform_twitch::register_twitch_sub_actions(
            sub_actions,
            transport,
            creds,
            forge_platform_twitch::TwitchLifecycle::new(),
        )
        .unwrap();
    }

    fn youtube_token_source() -> Arc<
        dyn Fn() -> futures_util::future::BoxFuture<'static, Result<String, PlatformError>>
            + Send
            + Sync,
    > {
        Arc::new(|| Box::pin(async { Ok("token".to_owned()) }))
    }

    fn register_youtube_runners(sub_actions: &mut SubActionRegistry, _: &Arc<dyn DataProvider>) {
        use forge_platform_youtube as yt;
        let quota = Arc::new(tokio::sync::Mutex::new(yt::QuotaState::default()));
        let chat = yt::LiveChatIdHandle::new();
        let broadcast = yt::ActiveBroadcastIdHandle::new();
        yt::register_youtube_sub_actions(
            sub_actions,
            Arc::new(yt::YoutubeSendChat::new(
                &forge_platform_core::PlatformEndpoints::default(),
                youtube_token_source(),
                chat.clone(),
                Arc::clone(&quota),
            )),
            Arc::new(yt::YoutubeModeration::new(
                &forge_platform_core::PlatformEndpoints::default(),
                youtube_token_source(),
                chat,
                Arc::clone(&quota),
            )),
            Arc::new(yt::YoutubeStreamMetadata::new(
                &forge_platform_core::PlatformEndpoints::default(),
                youtube_token_source(),
                broadcast.clone(),
                Arc::clone(&quota),
            )),
            Arc::new(yt::YoutubeStreamStats::new(
                &forge_platform_core::PlatformEndpoints::default(),
                youtube_token_source(),
                broadcast.clone(),
                Arc::clone(&quota),
            )),
            Arc::new(yt::YoutubeAdBreak::new(
                &forge_platform_core::PlatformEndpoints::default(),
                youtube_token_source(),
                broadcast.clone(),
                Arc::clone(&quota),
            )),
            Arc::new(yt::YoutubeThumbnail::new(
                &forge_platform_core::PlatformEndpoints::default(),
                youtube_token_source(),
                broadcast,
                Arc::clone(&quota),
            )),
            Arc::new(yt::YoutubeChannelLookup::new(
                &forge_platform_core::PlatformEndpoints::default(),
                youtube_token_source(),
                quota,
            )),
        )
        .unwrap();
    }

    fn register_kick_runners(sub_actions: &mut SubActionRegistry, _: &Arc<dyn DataProvider>) {
        use forge_platform_kick as kick;
        let limiter: Arc<dyn RateLimiter> = Arc::new(NoopRateLimiter);
        kick::register_kick_sub_actions(
            sub_actions,
            kick::KickSubActionDeps {
                client: Arc::new(kick::KickSendChat::new(
                    &forge_platform_core::PlatformEndpoints::default(),
                    Arc::clone(&limiter),
                )),
                token_source: Arc::new(|| Box::pin(async { Ok("token".to_owned()) })),
                broadcaster_id_source: Arc::new(|| Box::pin(async { Ok(1) })),
                moderation: Arc::new(kick::KickModeration::new(
                    &forge_platform_core::PlatformEndpoints::default(),
                    Arc::clone(&limiter),
                )),
                channel: Arc::new(kick::KickChannel::new(
                    &forge_platform_core::PlatformEndpoints::default(),
                    Arc::clone(&limiter),
                )),
                rewards: Arc::new(kick::KickRewards::new(
                    &forge_platform_core::PlatformEndpoints::default(),
                    Arc::clone(&limiter),
                )),
                categories: Arc::new(kick::KickCategories::new(
                    &forge_platform_core::PlatformEndpoints::default(),
                    limiter,
                )),
            },
        )
        .unwrap();
    }

    fn declared_integrations() -> Vec<IntegrationId> {
        vec![
            forge_platform_twitch::TWITCH_INTEGRATION.id,
            forge_platform_youtube::YOUTUBE_INTEGRATION.id,
            forge_platform_kick::KICK_INTEGRATION.id,
            forge_obs::OBS_INTEGRATION.id,
            forge_vtube::VTUBE_INTEGRATION.id,
            forge_discord::DISCORD_INTEGRATION.id,
            forge_midi::MIDI_INTEGRATION.id,
            forge_hotkey::HOTKEY_INTEGRATION.id,
        ]
    }

    const TARGET_ROUTED_CORE_RUNNERS: &[&str] = &["twitch.chat.send_message"];

    fn ownership_violations<'a>(
        kinds: impl Iterator<Item = &'a str>,
        owner_of: impl Fn(&str) -> Option<IntegrationId>,
    ) -> Vec<String> {
        let declared = declared_integrations();
        kinds
            .filter(|kind| !TARGET_ROUTED_CORE_RUNNERS.contains(kind))
            .filter_map(|kind| {
                let namespace = kind.split('.').next().unwrap_or(kind);
                let expected = declared
                    .iter()
                    .find(|integration| integration.as_str() == namespace)
                    .cloned();
                let actual = owner_of(kind);
                (actual != expected)
                    .then(|| format!("{kind}: owned by {actual:?}, expected {expected:?}"))
            })
            .collect()
    }

    #[tokio::test]
    async fn every_trigger_belongs_to_the_integration_named_by_its_namespace_or_to_core() {
        let (triggers, _) = every_registry_entry().await;

        let violations = ownership_violations(triggers.all().map(|d| d.id()), |kind| {
            triggers.owning_integration(kind).cloned()
        });

        assert!(violations.is_empty(), "{violations:#?}");
        for integration in [
            forge_platform_twitch::TWITCH_INTEGRATION.id,
            forge_midi::MIDI_INTEGRATION.id,
            forge_hotkey::HOTKEY_INTEGRATION.id,
        ] {
            assert!(
                triggers
                    .all()
                    .any(|d| triggers.owning_integration(d.id()) == Some(&integration)),
                "{integration} owns no trigger"
            );
        }
    }

    #[tokio::test]
    async fn every_sub_action_belongs_to_the_integration_named_by_its_namespace_or_to_core() {
        let (_, sub_actions) = every_registry_entry().await;

        let violations = ownership_violations(sub_actions.all().map(|r| r.id()), |kind| {
            sub_actions.owning_integration(kind).cloned()
        });

        assert!(violations.is_empty(), "{violations:#?}");
        for integration in declared_integrations()
            .into_iter()
            .filter(|id| id.as_str() != "hotkey")
        {
            assert!(
                sub_actions
                    .all()
                    .any(|r| sub_actions.owning_integration(r.id()) == Some(&integration)),
                "{integration} owns no sub-action"
            );
        }
    }
}
