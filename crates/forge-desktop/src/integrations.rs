use std::collections::HashMap;
use std::sync::Arc;

use forge_events::{
    ActingSender, ChatSendFailed, ChatSendRequest, ChatSent, Event, EventPublisher, EventSource,
    EventSubscription, EventsError, TypedEvent,
};
use forge_platform_core::{
    BanListSource, BuiltinCollections, BuiltinContent, BuiltinControl, BuiltinHealth,
    BuiltinOptionSource, BuiltinStatus, ChatPlatform, FollowLookup, PlatformEndpoints,
    QuickActions, SectionIcon, SenderPreference,
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
use crate::integration_factories::{
    wire_discord, wire_hotkey, wire_kick, wire_midi, wire_obs, wire_twitch, wire_vtube,
    wire_youtube,
};
use crate::platform_credentials::PlatformCredentialManagers;
use forge_hotkey::HotkeyReconciler;
use forge_runtime::{IntegrationFactory, IntegrationObjects, TaskGroup};

const CHAT_SEND_QUEUE_CAPACITY: usize = 64;
const WHISPER_SENT_KIND: &str = "chat.whisper.sent";

#[derive(Clone)]
pub struct BuiltinObject {
    pub icon: SectionIcon,
    pub status: Arc<dyn BuiltinStatus>,
    pub health: Arc<dyn BuiltinHealth>,
    pub content: Arc<dyn BuiltinContent>,
    pub quick: Arc<dyn QuickActions>,
    pub control: Option<Arc<dyn BuiltinControl>>,
    pub collections: Option<Arc<dyn BuiltinCollections>>,
    pub option_source: Option<Arc<dyn BuiltinOptionSource>>,
    pub obs_client: Option<Arc<forge_obs::ObsClient>>,
    pub vtube_client: Option<Arc<forge_vtube::VTubeClient>>,
    pub follow: Option<Arc<dyn FollowLookup>>,
    pub ban_list: Option<Arc<dyn BanListSource>>,
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

impl IntegrationObjects<BuiltinObject> for BuiltinRegistry {
    fn install(&self, object: BuiltinObject) {
        BuiltinRegistry::install(self, object);
    }

    fn remove(&self, id: &IntegrationId) {
        BuiltinRegistry::remove(self, id);
    }

    fn get(&self, id: &IntegrationId) -> Option<BuiltinObject> {
        BuiltinRegistry::get(self, id)
    }
}

pub struct Integrations {
    pub builtins: BuiltinRegistry,
    pub factories: Vec<Arc<dyn IntegrationFactory<BuiltinObject>>>,
    pub obs_install_seed: ObsInstallSeed,
    pub vtube_install_seed: VTubeInstallSeed,
    pub discord_client: Option<Arc<forge_discord::DiscordClient>>,
    pub midi_sink: Arc<forge_midi::SwitchableMidiSink>,
    pub hotkey_client: Option<Arc<forge_hotkey::HotkeyClient>>,
    pub hotkey_reconciler: Option<Arc<HotkeyReconciler>>,
    pub donation_services: DonationServices,
    pub twitch_bot: Option<Arc<forge_platform_twitch::TwitchBotAccount>>,
    pub twitch_sign_in: Option<forge_platform_twitch::TwitchSignIn>,
    pub youtube_sign_in: Option<forge_platform_youtube::YoutubeSignIn>,
    pub kick_sign_in: Option<forge_platform_kick::KickSignIn>,
    pub platform_credentials: PlatformCredentialManagers,
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
        if let Some(client) = previous
            && let Err(err) = client.disconnect().await
        {
            tracing::warn!(error = %err, "disconnecting the live client failed");
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
        if let Some(client) = previous
            && let Err(err) = client.disconnect().await
        {
            tracing::warn!(error = %err, "disconnecting the live client failed");
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
        option_source: Some(Arc::clone(&client) as Arc<dyn BuiltinOptionSource>),
        obs_client: None,
        vtube_client: Some(client),
        follow: None,
        ban_list: None,
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
        option_source: Some(Arc::clone(&client) as Arc<dyn BuiltinOptionSource>),
        obs_client: Some(client),
        vtube_client: None,
        follow: None,
        ban_list: None,
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
        collections: Some(Arc::clone(&bundle) as Arc<dyn BuiltinCollections>),
        option_source: None,
        obs_client: None,
        vtube_client: None,
        follow: Some(Arc::clone(&bundle) as Arc<dyn FollowLookup>),
        ban_list: Some(bundle as Arc<dyn BanListSource>),
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
        control: Some(Arc::clone(&bundle) as Arc<dyn BuiltinControl>),
        collections: None,
        option_source: None,
        obs_client: None,
        vtube_client: None,
        follow: Some(Arc::clone(&bundle) as Arc<dyn FollowLookup>),
        ban_list: Some(bundle as Arc<dyn BanListSource>),
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
        control: Some(Arc::clone(&bundle) as Arc<dyn BuiltinControl>),
        collections: None,
        option_source: None,
        obs_client: None,
        vtube_client: None,
        follow: None,
        ban_list: Some(bundle as Arc<dyn BanListSource>),
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

    let mut factories: Vec<Arc<dyn IntegrationFactory<BuiltinObject>>> = Vec::new();
    let mut twitch_bot = None;
    let mut twitch_sign_in = None;
    let mut platform_credentials = PlatformCredentialManagers::default();
    if let Some(twitch) = wire_twitch(sub_actions, backend, bus, endpoints) {
        twitch_bot = Some(twitch.bot());
        twitch_sign_in = Some(twitch.sign_in());
        platform_credentials.twitch = Some(twitch.credentials_manager());
        factories.push(Arc::new(twitch));
    }
    let obs = wire_obs(sub_actions, backend, bus);
    let obs_install_seed = obs.seed();
    factories.push(Arc::new(obs));
    let vtube = wire_vtube(sub_actions, backend, bus);
    let vtube_install_seed = vtube.seed();
    factories.push(Arc::new(vtube));
    let mut discord_client = None;
    if let Some(discord) = wire_discord(sub_actions, backend, bus, endpoints) {
        discord_client = Some(discord.client());
        factories.push(Arc::new(discord));
    }
    let midi = wire_midi(sub_actions, backend, bus, midi_kinds);
    let midi_sink = midi.sink();
    factories.push(Arc::new(midi));
    let (hotkey, hotkey_reconciler) = wire_hotkey(backend, bus, hotkey_main_thread).await;
    let hotkey_client = hotkey.client();
    factories.push(Arc::new(hotkey));
    let mut youtube_sign_in = None;
    if let Some(youtube) = wire_youtube(sub_actions, backend, bus, endpoints) {
        youtube_sign_in = Some(youtube.sign_in());
        platform_credentials.youtube = Some(youtube.credentials_manager());
        factories.push(Arc::new(youtube));
    }
    let mut kick_sign_in = None;
    if let Some(kick) = wire_kick(sub_actions, backend, bus, endpoints) {
        kick_sign_in = Some(kick.sign_in());
        platform_credentials.kick = Some(kick.credentials_manager());
        factories.push(Arc::new(kick));
    }
    let mut donation_services = DonationServices::default();
    if let Some(donatello) = wire_donatello(backend, endpoints, donations, bus) {
        let provider = donatello.provider();
        restore_donatello_poll_interval(&provider, backend.as_ref()).await;
        donation_services.donatello = Some(provider);
        factories.push(Arc::new(donatello));
    }
    if let Some(monobank) = wire_monobank(backend, endpoints, donations, bus) {
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
        twitch_bot,
        twitch_sign_in,
        youtube_sign_in,
        kick_sign_in,
        platform_credentials,
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
        tracing::error!(error = %e, "twitch trigger registration failed");
    }
    if let Err(e) = forge_obs::register_obs_triggers(triggers) {
        tracing::error!(error = %e, "obs trigger registration failed");
    }
    if let Err(e) = forge_vtube::register_vtube_triggers(triggers) {
        tracing::error!(error = %e, "vtube trigger registration failed");
    }
    if let Err(e) = forge_midi::register_midi_triggers(triggers) {
        tracing::error!(error = %e, "midi trigger registration failed");
    }
    if let Err(e) = forge_hotkey::register_hotkey_triggers(triggers) {
        tracing::error!(error = %e, "hotkey trigger registration failed");
    }
    if let Err(e) = forge_platform_youtube::register_youtube_triggers(triggers) {
        tracing::error!(error = %e, "youtube trigger registration failed");
    }
    if let Err(e) = forge_platform_kick::register_kick_triggers(triggers) {
        tracing::error!(error = %e, "kick trigger registration failed");
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
    mut events: EventSubscription,
    label: &'static str,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => bus.publish(event),
                Err(EventsError::BusClosed) => break,
                Err(EventsError::LaggingReceiver) => {
                    tracing::warn!(bridge = label, "event bridge lagging");
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

struct QueuedChatSend {
    message: String,
    intent: ChatSendIntent,
    sender: SenderPreference,
    caused_by: EventId,
}

fn whisper_sent(
    source: EventSource,
    target: &str,
    recipient_login: String,
    acting: Option<ActingSender>,
    caused_by: EventId,
) -> Event {
    let mut payload = serde_json::json!({
        "channel": target,
        (WHISPER_RECIPIENT_FIELD): recipient_login,
    });
    if let Some(acting) = acting {
        acting.insert_into(&mut payload);
    }
    Event::caused_by(source, WHISPER_SENT_KIND, payload, caused_by)
}

fn chat_send_failed(source: EventSource, target: &str, error: String, caused_by: EventId) -> Event {
    ChatSendFailed {
        channel: target.to_owned(),
        error,
    }
    .into_event(source, Some(caused_by))
}

fn spawn_chat_send_worker(
    bus: Arc<EventBus>,
    platform: Arc<dyn ChatPlatform>,
    target: &'static str,
    source: EventSource,
    mut requests: mpsc::Receiver<QueuedChatSend>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let QueuedChatSend {
                message,
                intent,
                sender,
                caused_by,
            } = request;
            let delivered = match intent {
                ChatSendIntent::Plain => platform
                    .send_message_as(target, &message, sender)
                    .await
                    .map(|sent_as| {
                        ChatSent {
                            channel: target.to_owned(),
                            message,
                            reply_to_message_id: None,
                            sent_as,
                        }
                        .into_event(source, Some(caused_by))
                    }),
                ChatSendIntent::Reply { parent_message_id } => platform
                    .send_reply_as(target, &parent_message_id, &message, sender)
                    .await
                    .map(|sent_as| {
                        ChatSent {
                            channel: target.to_owned(),
                            message,
                            reply_to_message_id: Some(parent_message_id),
                            sent_as,
                        }
                        .into_event(source, Some(caused_by))
                    }),
                ChatSendIntent::Whisper { recipient_login } => platform
                    .send_whisper_as(&recipient_login, &message, sender)
                    .await
                    .map(|acting| whisper_sent(source, target, recipient_login, acting, caused_by)),
            };
            match delivered {
                Ok(event) => bus.publish(event),
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
                    tracing::warn!(
                        channel = target,
                        lag_count,
                        "chat send bridge lagging"
                    );
                    bus.publish(
                        ChatSendFailed {
                            channel: target.to_owned(),
                            error: format!(
                                "event bus receiver lagging ({lag_count} lag event(s) observed); {} events may have been dropped",
                                ChatSendRequest::KIND
                            ),
                        }
                        .into_event(source, None),
                    );
                    continue;
                }
                Err(_) => continue,
            };
            if event.kind != ChatSendRequest::KIND {
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
            if requested.is_some_and(|requested| {
                PlatformId::from_wire(requested).is_none_or(|platform| platform.as_str() != target)
            }) {
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
            let sender = match SenderPreference::from_payload(&event.payload) {
                Ok(sender) => sender,
                Err(reason) => {
                    bus.publish(chat_send_failed(source, target, reason, caused_by));
                    continue;
                }
            };
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
            let request = QueuedChatSend {
                message,
                intent,
                sender,
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
            tracing::error!(platform = label, error = %e, "chat platform connect failed");
        }
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use forge_events::EventChannel;
    use forge_net::{RateLimitError, RateLimitOutcome, RateLimiter};
    use forge_platform_core::{ConnectionState, PlatformError};
    use forge_platform_core::{SENDER_FIELD, SENT_AS_FIELD, SenderRole, unknown_sender_reason};
    use forge_runtime::NullEventLogRepo;
    use std::time::Duration;
    use tokio::sync::{Semaphore, mpsc};

    struct NoopRateLimiter;

    #[async_trait::async_trait]
    impl RateLimiter for NoopRateLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, RateLimitError> {
            Ok(RateLimitOutcome::Granted)
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
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the receiver may already be gone"
            )]
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
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the receiver may already be gone"
            )]
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
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the receiver may already be gone"
            )]
            let _ = self.sends.send(PlatformCall::Whisper {
                recipient: recipient_login.to_string(),
                text: text.to_string(),
            });
            self.outcome()
        }
        fn events(&self) -> EventSubscription {
            EventChannel::new(1).subscribe()
        }
    }

    fn test_bus() -> Arc<EventBus> {
        EventBus::new(Arc::new(NullEventLogRepo))
    }

    fn request(source: EventSource, payload: serde_json::Value) -> Event {
        Event::new(source, "chat.send.request", payload)
    }

    async fn expect_call(rx: &mut mpsc::UnboundedReceiver<PlatformCall>) -> PlatformCall {
        tokio::time::timeout(Duration::from_secs(30), rx.recv())
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
    async fn mixed_case_target_routes_only_to_the_named_platform() {
        let bus = test_bus();
        let (twitch, mut twitch_rx) = RecordingPlatform::spawn();
        let (kick, mut kick_rx) = RecordingPlatform::spawn();
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        spawn_chat_send_bridge(Arc::clone(&bus), kick, "kick", EventSource::Kick);
        tokio::task::yield_now().await;

        bus.publish(request(
            EventSource::Rhai,
            serde_json::json!({ "target": "Twitch", "message": "cased" }),
        ));
        bus.publish(request(
            EventSource::Rhai,
            serde_json::json!({ "target": "kick", "message": "sentinel" }),
        ));

        assert_eq!(
            expect_send(&mut twitch_rx).await,
            ("twitch".to_string(), "cased".to_string())
        );
        assert_eq!(
            expect_send(&mut kick_rx).await,
            ("kick".to_string(), "sentinel".to_string()),
            "kick must skip the request targeted at Twitch"
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

    async fn next_of_kind(sub: &mut EventSubscription, kind: &str) -> Event {
        tokio::time::timeout(Duration::from_secs(30), async {
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

    fn drain_of_kind(sub: &mut EventSubscription, kind: &str) -> Vec<Event> {
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

    #[derive(Debug, PartialEq)]
    enum SenderCall {
        Message(SenderPreference),
        Reply(SenderPreference),
        Whisper(SenderPreference),
        Unrouted,
    }

    struct SenderAwarePlatform {
        calls: mpsc::UnboundedSender<SenderCall>,
        acting: Option<ActingSender>,
    }

    impl SenderAwarePlatform {
        fn spawn(acting: Option<ActingSender>) -> (Arc<Self>, mpsc::UnboundedReceiver<SenderCall>) {
            let (calls, rx) = mpsc::unbounded_channel();
            (Arc::new(Self { calls, acting }), rx)
        }

        fn record(&self, call: SenderCall) -> Result<Option<ActingSender>, PlatformError> {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the receiver may already be gone"
            )]
            let _ = self.calls.send(call);
            Ok(self.acting.clone())
        }
    }

    #[async_trait::async_trait]
    impl ChatPlatform for SenderAwarePlatform {
        fn connection_state(&self) -> ConnectionState {
            ConnectionState::Connected
        }
        async fn connect(&self) -> Result<(), PlatformError> {
            Ok(())
        }
        async fn disconnect(&self) -> Result<(), PlatformError> {
            Ok(())
        }
        async fn send_message(&self, _channel: &str, _text: &str) -> Result<(), PlatformError> {
            self.record(SenderCall::Unrouted).map(|_| ())
        }
        async fn send_message_as(
            &self,
            _channel: &str,
            _text: &str,
            sender: SenderPreference,
        ) -> Result<Option<ActingSender>, PlatformError> {
            self.record(SenderCall::Message(sender))
        }
        async fn send_reply_as(
            &self,
            _channel: &str,
            _reply_parent_message_id: &str,
            _text: &str,
            sender: SenderPreference,
        ) -> Result<Option<ActingSender>, PlatformError> {
            self.record(SenderCall::Reply(sender))
        }
        async fn send_whisper_as(
            &self,
            _recipient_login: &str,
            _text: &str,
            sender: SenderPreference,
        ) -> Result<Option<ActingSender>, PlatformError> {
            self.record(SenderCall::Whisper(sender))
        }
        fn events(&self) -> EventSubscription {
            EventChannel::new(1).subscribe()
        }
    }

    async fn expect_sender_call(rx: &mut mpsc::UnboundedReceiver<SenderCall>) -> SenderCall {
        tokio::time::timeout(Duration::from_secs(30), rx.recv())
            .await
            .expect("a platform send call was expected but never arrived")
            .expect("mock send channel closed")
    }

    fn bot_acting() -> ActingSender {
        ActingSender {
            role: SenderRole::Bot,
            login: "forgebot".to_owned(),
        }
    }

    fn one_of_each_send_kind() -> [(serde_json::Value, &'static str); 3] {
        [
            (serde_json::json!({ "message": "plain" }), "chat.send"),
            (
                serde_json::json!({ "message": "reply", REPLY_PARENT_FIELD: "msg-1" }),
                "chat.send",
            ),
            (
                serde_json::json!({ "message": "psst", WHISPER_RECIPIENT_FIELD: "viewer" }),
                "chat.whisper.sent",
            ),
        ]
    }

    #[tokio::test]
    async fn each_send_kind_hands_the_requested_sender_to_the_platform() {
        let bus = test_bus();
        let (twitch, mut twitch_rx) = SenderAwarePlatform::spawn(None);
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        tokio::task::yield_now().await;

        for (payload, expected) in [
            (
                serde_json::json!({ "message": "m" }),
                SenderCall::Message(SenderPreference::Default),
            ),
            (
                serde_json::json!({ "message": "m", SENDER_FIELD: null }),
                SenderCall::Message(SenderPreference::Default),
            ),
            (
                serde_json::json!({ "message": "m", SENDER_FIELD: "streamer" }),
                SenderCall::Message(SenderPreference::Streamer),
            ),
            (
                serde_json::json!({ "message": "m", SENDER_FIELD: "bot" }),
                SenderCall::Message(SenderPreference::BotOnly),
            ),
            (
                serde_json::json!({ "message": "m", REPLY_PARENT_FIELD: "msg-1", SENDER_FIELD: "bot" }),
                SenderCall::Reply(SenderPreference::BotOnly),
            ),
            (
                serde_json::json!({ "message": "m", WHISPER_RECIPIENT_FIELD: "viewer", SENDER_FIELD: "streamer" }),
                SenderCall::Whisper(SenderPreference::Streamer),
            ),
        ] {
            bus.publish(request(EventSource::Rhai, payload.clone()));

            assert_eq!(
                expect_sender_call(&mut twitch_rx).await,
                expected,
                "{payload}"
            );
        }
    }

    #[tokio::test]
    async fn an_unknown_sender_fails_the_request_unsent_naming_the_value() {
        for (sender, named) in [
            (serde_json::json!("Bot"), "Bot"),
            (serde_json::json!(7), "7"),
        ] {
            let bus = test_bus();
            let mut observer = bus.subscribe();
            let (twitch, mut twitch_rx) = SenderAwarePlatform::spawn(None);
            spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
            tokio::task::yield_now().await;
            let req = request(
                EventSource::Rhai,
                serde_json::json!({ "message": "hi", SENDER_FIELD: sender }),
            );
            let req_id = req.id;

            bus.publish(req);
            bus.publish(request(
                EventSource::Rhai,
                serde_json::json!({ "message": "sentinel" }),
            ));

            let failed = next_of_kind(&mut observer, "chat.send.failed").await;
            assert_eq!(failed.caused_by, Some(req_id), "{named}");
            assert_eq!(failed.payload["error"], unknown_sender_reason(named));
            assert_eq!(
                expect_sender_call(&mut twitch_rx).await,
                SenderCall::Message(SenderPreference::Default),
                "the request with sender {named} must not reach the platform"
            );
        }
    }

    #[tokio::test]
    async fn every_outcome_event_records_the_identity_the_platform_says_acted() {
        let bus = test_bus();
        let mut observer = bus.subscribe();
        let (twitch, _twitch_rx) = SenderAwarePlatform::spawn(Some(bot_acting()));
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        tokio::task::yield_now().await;

        for (payload, outcome_kind) in one_of_each_send_kind() {
            bus.publish(request(EventSource::Rhai, payload.clone()));

            let outcome = next_of_kind(&mut observer, outcome_kind).await;
            assert_eq!(
                outcome.payload[SENT_AS_FIELD],
                serde_json::json!({ "role": "bot", "login": "forgebot" }),
                "{payload}"
            );
        }
    }

    #[tokio::test]
    async fn outcome_events_carry_no_sent_as_when_the_platform_reports_no_identity() {
        let bus = test_bus();
        let mut observer = bus.subscribe();
        let (twitch, _twitch_rx) = RecordingPlatform::spawn();
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        tokio::task::yield_now().await;

        for (payload, outcome_kind) in one_of_each_send_kind() {
            bus.publish(request(EventSource::Rhai, payload.clone()));

            let outcome = next_of_kind(&mut observer, outcome_kind).await;
            assert!(
                outcome.payload.get(SENT_AS_FIELD).is_none(),
                "{payload} published {}",
                outcome.payload
            );
        }
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
        use crate::test_support::{StubActions, StubEventLog, StubHistory, runtime, test_backend};
        use forge_components::Presentation;
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
                option_source: None,
                obs_client: None,
                vtube_client: None,
                follow: None,
                ban_list: None,
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

    pub(crate) struct SilentSoundboard;

    #[async_trait::async_trait]
    impl forge_soundboard::SoundboardControl for SilentSoundboard {
        async fn play(
            &self,
            _clip_id: forge_types::ClipId,
            _output_device_override: Option<forge_types::OutputDevice>,
        ) -> Result<(), forge_soundboard::SoundboardError> {
            Ok(())
        }

        async fn stop(
            &self,
            _clip_id: forge_types::ClipId,
        ) -> Result<(), forge_soundboard::SoundboardError> {
            Ok(())
        }

        async fn stop_all(&self) -> Result<(), forge_soundboard::SoundboardError> {
            Ok(())
        }

        async fn set_master_volume(
            &self,
            _gain: f32,
        ) -> Result<(), forge_soundboard::SoundboardError> {
            Ok(())
        }
    }

    struct SilentSpeaker;

    #[async_trait::async_trait]
    impl forge_runtime::SpeakDispatcher for SilentSpeaker {
        async fn speak(
            &self,
            _text: String,
            _voice: Option<String>,
            _origin: forge_runtime::SpeechOrigin,
        ) -> Result<(), forge_runtime::SpeakDispatchError> {
            Ok(())
        }
    }

    struct NoLatestValues;

    impl forge_types::LatestValueReader for NoLatestValues {
        fn latest(
            &self,
            _slot: &str,
            _scope: forge_types::LatestScope<'_>,
        ) -> Option<forge_types::LatestValue> {
            None
        }
    }

    pub(crate) async fn every_registry_entry() -> (TriggerRegistry, SubActionRegistry) {
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
            forge_runtime::CoreSubActionDeps {
                globals: Arc::clone(&backend) as Arc<dyn forge_storage::GlobalsRepo>,
                user_globals: Arc::clone(&backend) as Arc<dyn forge_storage::UserGlobalsRepo>,
                scripts: Arc::new(forge_runtime::ScriptRegistry::new()),
                publisher: publisher(&bus),
                settings: Arc::clone(&backend) as Arc<dyn forge_storage::SettingsRepo>,
                scheduler: forge_runtime::SchedulerCell::new(),
                trigger_instances: backend.trigger_instance_repo(),
                actions: backend.action_repo(),
                script_repo: Arc::clone(&backend) as Arc<dyn forge_storage::ScriptRepo>,
                cancel_registry: Arc::new(forge_runtime::ActionCancelRegistry::new()),
                overlays: forge_runtime::OverlayServiceCell::new(),
                config: forge_runtime::Config::default(),
            },
        )
        .unwrap();
        wire_obs(&mut sub_actions, &backend, &bus);
        wire_vtube(&mut sub_actions, &backend, &bus);
        wire_discord(
            &mut sub_actions,
            &backend,
            &bus,
            &PlatformEndpoints::default(),
        );
        wire_midi(&mut sub_actions, &backend, &bus, Vec::new());
        register_twitch_runners(&mut sub_actions, &backend, &bus);
        register_youtube_runners(&mut sub_actions, &bus);
        register_kick_runners(&mut sub_actions, &bus);
        forge_runtime::register_scheduled_run_sub_actions(
            &mut sub_actions,
            forge_runtime::ScheduledRunsCell::new(),
        )
        .unwrap();
        forge_soundboard::register_soundboard_sub_actions(
            &mut sub_actions,
            Arc::new(SilentSoundboard),
        )
        .unwrap();
        forge_runtime::register_speech_sub_actions(
            &mut sub_actions,
            Arc::new(SilentSpeaker),
            forge_runtime::TwitchEmoteLexicon::default(),
        )
        .unwrap();
        forge_runtime::register_donation_sub_actions(
            &mut sub_actions,
            Arc::new(DonationIngest::new(
                backend.donation_repo(),
                publisher(&bus),
                forge_types::Shared::new(String::new()),
            )),
        )
        .unwrap();
        forge_runtime::register_latest_sub_actions(&mut sub_actions, Arc::new(NoLatestValues))
            .unwrap();
        (triggers, sub_actions)
    }

    fn register_twitch_runners(
        sub_actions: &mut SubActionRegistry,
        backend: &Arc<dyn DataProvider>,
        bus: &Arc<EventBus>,
    ) {
        let creds = creds_of(backend);
        let manager = Arc::new(forge_platform_twitch::TwitchCredentialsManager::new(
            crate::test_support::default_twitch_oauth_client(),
            Arc::clone(&creds),
        ));
        let transport: Arc<dyn forge_platform_twitch::HelixTransport> =
            Arc::new(forge_platform_twitch::HelixHttpTransport::new(
                crate::test_support::default_twitch_helix_gateway(publisher(bus)),
                "test-client".to_owned(),
                manager as Arc<dyn forge_platform_twitch::HelixTokenSource>,
            ));
        forge_platform_twitch::register_twitch_sub_actions(
            sub_actions,
            transport,
            creds,
            forge_platform_twitch::TwitchLifecycle::new(),
            None,
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

    fn register_youtube_runners(sub_actions: &mut SubActionRegistry, bus: &Arc<EventBus>) {
        use forge_platform_youtube as yt;
        let api = yt::YoutubeDataApi::new(
            &forge_platform_core::PlatformEndpoints::default(),
            yt::SharedQuota::default(),
            publisher(bus),
        )
        .unwrap();
        let chat = yt::LiveChatIdHandle::new();
        let broadcast = yt::ActiveBroadcastIdHandle::new();
        yt::register_youtube_sub_actions(
            sub_actions,
            Arc::new(yt::YoutubeSendChat::new(
                api.clone(),
                youtube_token_source(),
                chat.clone(),
            )),
            Arc::new(yt::YoutubeModeration::new(
                api.clone(),
                youtube_token_source(),
                Arc::new(|| {
                    Box::pin(async {
                        Ok(yt::YoutubeBroadcaster {
                            channel_id: "UCtest".to_owned(),
                            channel_title: "Test".to_owned(),
                        })
                    })
                }),
                chat,
                Arc::new(forge_storage::ban_ledger::MockBanLedgerRepo::new()),
            )),
            Arc::new(yt::YoutubeStreamMetadata::new(
                api.clone(),
                youtube_token_source(),
                broadcast.clone(),
            )),
            Arc::new(yt::YoutubeStreamStats::new(
                api.clone(),
                youtube_token_source(),
                broadcast.clone(),
            )),
            Arc::new(yt::YoutubeAdBreak::new(
                api.clone(),
                youtube_token_source(),
                broadcast.clone(),
            )),
            Arc::new(yt::YoutubeThumbnail::new(
                api.clone(),
                youtube_token_source(),
                broadcast,
            )),
            Arc::new(yt::YoutubeChannelLookup::new(api, youtube_token_source())),
        )
        .unwrap();
    }

    fn register_kick_runners(sub_actions: &mut SubActionRegistry, bus: &Arc<EventBus>) {
        use forge_platform_kick as kick;
        let api = kick::kick_api_gateway(
            &forge_platform_core::PlatformEndpoints::default(),
            Arc::new(NoopRateLimiter),
            publisher(bus),
        )
        .unwrap();
        kick::register_kick_sub_actions(
            sub_actions,
            kick::KickSubActionDeps {
                client: Arc::new(kick::KickSendChat::new(api.clone())),
                token_source: Arc::new(|| Box::pin(async { Ok("token".to_owned()) })),
                broadcaster_id_source: Arc::new(|| Box::pin(async { Ok(1) })),
                moderation: Arc::new(kick::KickModeration::new(
                    api.clone(),
                    Arc::new(forge_storage::ban_ledger::MockBanLedgerRepo::new()),
                )),
                channel: Arc::new(kick::KickChannel::new(api.clone())),
                rewards: Arc::new(kick::KickRewards::new(api.clone())),
                categories: Arc::new(kick::KickCategories::new(api)),
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

    #[tokio::test]
    async fn event_bridge_forwards_a_platform_s_final_events_then_exits_once_its_channel_closes() {
        let bus = test_bus();
        let mut observer = bus.subscribe();
        let platform = EventChannel::new(16);
        let bridge = spawn_event_bridge(publisher(&bus), platform.subscribe(), "twitch");

        for state in ["connected", "disconnected"] {
            platform.publish(Event::new(
                EventSource::Core,
                "platform.connection.changed",
                serde_json::json!({ "platform_id": "twitch", "state": state }),
            ));
        }
        drop(platform);

        tokio::time::timeout(Duration::from_secs(30), bridge)
            .await
            .expect("the bridge must exit once every platform sender is gone")
            .unwrap();
        for state in ["connected", "disconnected"] {
            let event = observer.recv().await.unwrap();
            assert_eq!(event.payload["state"], state);
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
    async fn chat_sent_payloads_match_the_golden() {
        let bus = test_bus();
        let mut observer = bus.subscribe();
        let (twitch, _twitch_rx) = SenderAwarePlatform::spawn(Some(bot_acting()));
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        tokio::task::yield_now().await;
        let (kick, _kick_rx) = RecordingPlatform::spawn();
        spawn_chat_send_bridge(Arc::clone(&bus), kick, "kick", EventSource::Kick);
        tokio::task::yield_now().await;

        let mut sent = Vec::new();
        for payload in [
            serde_json::json!({ "message": "golden plain", "target": "twitch" }),
            serde_json::json!({ "message": "golden reply", "target": "twitch", REPLY_PARENT_FIELD: "msg-1" }),
            serde_json::json!({ "message": "golden kick", "target": "kick" }),
        ] {
            bus.publish(request(EventSource::Rhai, payload));
            sent.push(next_of_kind(&mut observer, "chat.send").await);
        }

        assert_matches_golden(&sent, "chat.send.json");
    }

    #[tokio::test]
    async fn chat_send_failed_payloads_match_the_golden() {
        let bus = test_bus();
        let mut observer = bus.subscribe();
        let (twitch, _twitch_rx) = RecordingPlatform::failing("msg_duplicate: identical message");
        spawn_chat_send_bridge(Arc::clone(&bus), twitch, "twitch", EventSource::Twitch);
        tokio::task::yield_now().await;

        let mut failed = Vec::new();
        for payload in [
            serde_json::json!({ "message": "golden", SENDER_FIELD: "Bot" }),
            serde_json::json!({ "message": "golden" }),
        ] {
            bus.publish(request(EventSource::Rhai, payload));
            failed.push(next_of_kind(&mut observer, "chat.send.failed").await);
        }

        assert_matches_golden(&failed, "chat.send.failed.json");
    }
}
