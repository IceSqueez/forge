use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventPublisher, EventSource, EventStream, EventsError};
use forge_platform_core::{
    BuiltinContent, BuiltinControl, BuiltinHealth, BuiltinId, BuiltinStatus, ChatPlatform,
    LiveViewerSource, PlatformEndpoints, PlatformError, QuickActions, RateLimitOutcome,
    RateLimiter, SectionIcon, TokenBucketRateLimiter,
};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::EventBus;
use forge_storage::{CredentialsRepo, DataProvider, SettingsRepo, get_bool_setting};
use forge_types::EventId;
use tokio::sync::mpsc;

use crate::hotkey_bindings::{HOTKEY_ENABLED_KEY, load_hold_ceiling};
use crate::hotkey_sync::HotkeyReconciler;
use crate::midi_screen::MIDI_ENABLED_KEY;
use crate::obs_credentials_form::{OBS_AUTO_RECONNECT_KEY, OBS_CONNECT_ON_LAUNCH_KEY};
use crate::vtube_connect_form::{VTUBE_AUTO_RECONNECT_KEY, VTUBE_CONNECT_ON_LAUNCH_KEY};

const CONNECT_GUARD: Duration = Duration::from_secs(5);
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
    pub obs_client: Option<Arc<forge_obs::ObsClient>>,
}

/// Every non-OBS screen mount resolves through this map, so it must stay current across sign-in and sign-out.
#[derive(Clone, Default)]
pub struct BuiltinRegistry {
    entries: Arc<std::sync::RwLock<HashMap<BuiltinId, BuiltinObject>>>,
}

impl BuiltinRegistry {
    fn seeded(entries: HashMap<BuiltinId, BuiltinObject>) -> Self {
        Self {
            entries: Arc::new(std::sync::RwLock::new(entries)),
        }
    }

    pub fn get(&self, id: &BuiltinId) -> Option<BuiltinObject> {
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

    pub fn remove(&self, id: &BuiltinId) {
        let mut guard = self.entries.write().unwrap_or_else(|e| e.into_inner());
        guard.remove(id);
    }
}

pub struct Integrations {
    pub builtins: BuiltinRegistry,
    pub viewer_sources: Vec<Box<dyn LiveViewerSource>>,
    pub twitch_install_seed: Option<TwitchInstallSeed>,
    pub kick_install_seed: Option<KickInstallSeed>,
    pub youtube_install_seed: Option<YoutubeInstallSeed>,
    pub obs_install_seed: ObsInstallSeed,
    pub vtube_install_seed: VTubeInstallSeed,
    pub discord_client: Arc<forge_discord::DiscordClient>,
    /// `None` when the platform MIDI backend failed to initialize.
    pub midi_client: Option<Arc<forge_midi::MidiClient>>,
    pub hotkey_client: Option<Arc<forge_hotkey::HotkeyClient>>,
    pub hotkey_reconciler: Option<Arc<HotkeyReconciler>>,
}

/// Holds the same `SwitchableObsSink` the registered OBS runners resolve through, so a post-boot
/// connect reaches them without a restart.
#[derive(Clone)]
pub struct ObsInstallSeed {
    sink: Arc<forge_obs::SwitchableObsSink>,
    live: Arc<std::sync::RwLock<Option<Arc<forge_obs::ObsClient>>>>,
}

impl ObsInstallSeed {
    fn new(sink: Arc<forge_obs::SwitchableObsSink>) -> Self {
        Self {
            sink,
            live: Arc::new(std::sync::RwLock::new(None)),
        }
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

    /// Returns once the retired client's supervisor has joined, so it can no longer publish.
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

/// Holds the same `SwitchableVTubeSink` the registered VTube runners resolve through, so a
/// post-boot connect reaches them without a restart.
#[derive(Clone)]
pub struct VTubeInstallSeed {
    sink: Arc<forge_vtube::SwitchableVTubeSink>,
    live: Arc<std::sync::RwLock<Option<Arc<forge_vtube::VTubeClient>>>>,
}

impl VTubeInstallSeed {
    fn new(sink: Arc<forge_vtube::SwitchableVTubeSink>) -> Self {
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
        obs_client: None,
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
        obs_client: Some(client),
    }
}

#[derive(Clone)]
pub struct KickInstallSeed {
    pub manager: Arc<forge_platform_kick::KickCredentialsManager>,
    pub rate_limiter: Arc<dyn RateLimiter>,
    pub platform: Arc<forge_platform_kick::KickPlatform>,
    pub channel: Arc<forge_platform_kick::KickChannel>,
    pub rewards: Arc<forge_platform_kick::KickRewards>,
}

/// Holds the same lifecycle cell and credentials manager the registered Twitch sub-actions use, so a
/// post-boot sign-in shares them without a restart.
#[derive(Clone)]
pub struct TwitchInstallSeed {
    pub lifecycle: forge_platform_twitch::TwitchLifecycle,
    pub endpoints: PlatformEndpoints,
    pub manager: Arc<forge_platform_twitch::TwitchCredentialsManager>,
}

/// Holds the same handles the registered YouTube sub-actions resolve through, so a post-boot sign-in reaches them without a restart.
#[derive(Clone)]
pub struct YoutubeInstallSeed {
    pub manager: Arc<forge_platform_youtube::YoutubeCredentialsManager>,
    pub live_chat_id: forge_platform_youtube::LiveChatIdHandle,
    pub active_broadcast: forge_platform_youtube::ActiveBroadcastIdHandle,
    pub quota: Arc<tokio::sync::Mutex<forge_platform_youtube::QuotaState>>,
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
        control: Some(bundle as Arc<dyn BuiltinControl>),
        obs_client: None,
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
        obs_client: None,
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
        obs_client: None,
    }
}

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

pub async fn build_integrations(
    sub_actions: &mut SubActionRegistry,
    triggers: &mut TriggerRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
    endpoints: &PlatformEndpoints,
    hotkey_main_thread: forge_hotkey::MainThreadLink,
) -> Integrations {
    register_platform_triggers(triggers);

    let mut builtins: HashMap<BuiltinId, BuiltinObject> = HashMap::new();
    let mut viewer_sources: Vec<Box<dyn LiveViewerSource>> = Vec::new();

    let mut insert = |id: &str, object: Option<BuiltinObject>| {
        if let Some(object) = object {
            builtins.insert(BuiltinId::new(id), object);
        }
    };

    let (twitch, twitch_install_seed) = build_twitch(sub_actions, backend, bus, endpoints).await;
    insert("twitch", twitch);
    let obs_install_seed = build_obs(sub_actions, backend, bus).await;
    let vtube_install_seed = build_vtube(sub_actions, backend, bus).await;
    let (discord, discord_client) = build_discord(sub_actions, backend, bus);
    insert("discord", discord);
    let (midi, midi_client) = build_midi(sub_actions, backend, bus).await;
    insert("midi", midi);
    let (hotkey, hotkey_client, hotkey_reconciler) =
        build_hotkey(backend, bus, hotkey_main_thread).await;
    insert("hotkey", hotkey);

    let (youtube, youtube_viewers, youtube_install_seed) =
        build_youtube(sub_actions, backend, bus).await;
    if let Some(source) = youtube_viewers {
        viewer_sources.push(source);
    }
    insert("youtube", youtube);

    let (kick, kick_viewers, kick_install_seed) = build_kick(sub_actions, backend, bus).await;
    if let Some(source) = kick_viewers {
        viewer_sources.push(source);
    }
    insert("kick", kick);

    Integrations {
        builtins: BuiltinRegistry::seeded(builtins),
        viewer_sources,
        twitch_install_seed,
        kick_install_seed,
        youtube_install_seed,
        obs_install_seed,
        vtube_install_seed,
        discord_client,
        midi_client,
        hotkey_client,
        hotkey_reconciler,
    }
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

fn publisher(bus: &Arc<EventBus>) -> Arc<dyn EventPublisher> {
    Arc::clone(bus) as Arc<dyn EventPublisher>
}

fn creds_of(backend: &Arc<dyn DataProvider>) -> Arc<dyn CredentialsRepo> {
    Arc::clone(backend) as Arc<dyn CredentialsRepo>
}

fn spawn_event_bridge(bus: Arc<dyn EventPublisher>, mut events: EventStream, label: &'static str) {
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
    });
}

struct ChatSendRequest {
    message: String,
    caused_by: EventId,
}

fn spawn_chat_send_worker(
    bus: Arc<EventBus>,
    platform: Arc<dyn ChatPlatform>,
    target: &'static str,
    source: EventSource,
    mut requests: mpsc::Receiver<ChatSendRequest>,
) {
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            match platform.send_message(target, &request.message).await {
                Ok(()) => bus.publish(Event::caused_by(
                    source,
                    "chat.send",
                    serde_json::json!({ "channel": target, "message": request.message }),
                    request.caused_by,
                )),
                Err(e) => bus.publish(Event::caused_by(
                    source,
                    "chat.send.failed",
                    serde_json::json!({ "channel": target, "error": e.to_string() }),
                    request.caused_by,
                )),
            }
        }
    });
}

fn spawn_chat_send_bridge(
    bus: Arc<EventBus>,
    platform: Arc<dyn ChatPlatform>,
    target: &'static str,
    source: EventSource,
) {
    let (tx, rx) = mpsc::channel(CHAT_SEND_QUEUE_CAPACITY);
    spawn_chat_send_worker(Arc::clone(&bus), platform, target, source, rx);

    tokio::spawn(async move {
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
            if let Some(requested) = event.payload.get("target").and_then(|v| v.as_str())
                && requested != target
            {
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
            if tx.try_send(ChatSendRequest { message, caused_by }).is_err() {
                bus.publish(Event::caused_by(
                    source,
                    "chat.send.failed",
                    serde_json::json!({
                        "channel": target,
                        "error": "chat send queue full or worker unavailable; request dropped",
                    }),
                    caused_by,
                ));
            }
        }
    });
}

fn spawn_connect(platform: Arc<dyn ChatPlatform>, label: &'static str) {
    tokio::spawn(async move {
        if let Err(e) = platform.connect().await {
            eprintln!("forge-desktop: {label} connect failed: {e}");
        }
    });
}

async fn build_twitch(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
    endpoints: &PlatformEndpoints,
) -> (Option<BuiltinObject>, Option<TwitchInstallSeed>) {
    let Some(client_id) = forge_platform_twitch::client_id() else {
        return (None, None);
    };
    let creds = creds_of(backend);
    let lifecycle = forge_platform_twitch::TwitchLifecycle::new();
    let manager = Arc::new(forge_platform_twitch::TwitchCredentialsManager::new(
        Arc::clone(&creds),
        client_id.clone(),
    ));
    let seed = TwitchInstallSeed {
        lifecycle: lifecycle.clone(),
        endpoints: endpoints.clone(),
        manager: Arc::clone(&manager),
    };

    let rate_limiter: Arc<dyn RateLimiter> = Arc::new(TokenBucketRateLimiter::new(
        forge_platform_twitch::HELIX_BUDGET_CAPACITY,
        forge_platform_twitch::HELIX_BUDGET_WINDOW,
    ));
    let transport: Arc<dyn forge_platform_twitch::HelixTransport> =
        Arc::new(
            forge_platform_twitch::HelixHttpTransport::new(
                endpoints,
                Arc::clone(&rate_limiter),
                publisher(bus),
                client_id.clone(),
                Arc::clone(&manager) as Arc<dyn forge_platform_twitch::HelixTokenSource>,
            )
            .with_refresher(
                Arc::clone(&manager) as Arc<dyn forge_platform_twitch::HelixTokenRefresher>
            ),
        );
    if let Err(e) = forge_platform_twitch::register_twitch_sub_actions(
        sub_actions,
        transport,
        Arc::clone(&creds),
        lifecycle.clone(),
    ) {
        eprintln!("forge-desktop: twitch sub-action registration failed: {e}");
    }

    let send_platform: Arc<dyn ChatPlatform> =
        Arc::new(forge_platform_twitch::TwitchPlatform::new(
            forge_platform_twitch::ChatSessionConfig {
                client_id: client_id.clone(),
                broadcaster_id: String::new(),
                user_id: String::new(),
                endpoints: endpoints.clone(),
            },
            Arc::clone(&creds),
            Arc::clone(&manager),
            forge_platform_twitch::SubscriptionTracker::default(),
            Arc::clone(&rate_limiter),
            lifecycle.clone(),
        ));
    spawn_chat_send_bridge(
        Arc::clone(bus),
        send_platform,
        "twitch",
        EventSource::Twitch,
    );

    let Some(stored) = forge_platform_twitch::credentials::load(&*creds)
        .await
        .ok()
        .flatten()
    else {
        return (None, Some(seed));
    };

    let login = (!stored.login.is_empty()).then(|| stored.login.clone());
    let tracker = forge_platform_twitch::SubscriptionTracker::default();
    let config = forge_platform_twitch::ChatSessionConfig {
        client_id,
        broadcaster_id: stored.user_id.clone(),
        user_id: stored.user_id,
        endpoints: endpoints.clone(),
    };
    let bundle = forge_platform_twitch::TwitchIntegrationBundle::new(
        login,
        config,
        publisher(bus),
        creds,
        manager,
        tracker,
        rate_limiter,
        lifecycle,
    );

    (Some(twitch_builtin_object(bundle)), Some(seed))
}

async fn build_obs(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
) -> ObsInstallSeed {
    let sink = forge_obs::SwitchableObsSink::new();
    if let Err(e) = forge_obs::register_obs_sub_actions(
        sub_actions,
        Arc::clone(&sink) as Arc<dyn forge_obs::ObsSink>,
    ) {
        eprintln!("forge-desktop: obs sub-action registration failed: {e}");
    }
    let seed = ObsInstallSeed::new(sink);

    let settings = Arc::clone(backend) as Arc<dyn SettingsRepo>;
    if !get_bool_setting(&*settings, OBS_CONNECT_ON_LAUNCH_KEY, true).await {
        return seed;
    }

    let creds = creds_of(backend);
    let connect = forge_obs::credentials::load_and_connect(&*creds, publisher(bus));
    let client = match tokio::time::timeout(CONNECT_GUARD, connect).await {
        Ok(Ok(client)) => client,
        Ok(Err(_)) => return seed,
        Err(_) => {
            eprintln!("forge-desktop: obs connect timed out");
            return seed;
        }
    };
    client.set_auto_reconnect(get_bool_setting(&*settings, OBS_AUTO_RECONNECT_KEY, true).await);
    seed.install(client);

    seed
}

async fn build_vtube(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
) -> VTubeInstallSeed {
    let sink = forge_vtube::SwitchableVTubeSink::new();
    if let Err(e) = forge_vtube::register_vtube_sub_actions(
        sub_actions,
        Arc::clone(&sink) as Arc<dyn forge_vtube::VTubeSink>,
    ) {
        eprintln!("forge-desktop: vtube sub-action registration failed: {e}");
    }
    let seed = VTubeInstallSeed::new(sink);

    let settings = Arc::clone(backend) as Arc<dyn SettingsRepo>;
    if !get_bool_setting(&*settings, VTUBE_CONNECT_ON_LAUNCH_KEY, true).await {
        return seed;
    }

    let creds = creds_of(backend);
    let connect =
        forge_vtube::credentials::load_and_connect(&*creds, publisher(bus), Arc::clone(&creds));
    let client = match tokio::time::timeout(CONNECT_GUARD, connect).await {
        Ok(Ok(client)) => client,
        Ok(Err(_)) => return seed,
        Err(_) => {
            eprintln!("forge-desktop: vtube connect timed out");
            return seed;
        }
    };
    client.set_auto_reconnect(get_bool_setting(&*settings, VTUBE_AUTO_RECONNECT_KEY, true).await);
    seed.install(client);

    seed
}

fn build_discord(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
) -> (Option<BuiltinObject>, Arc<forge_discord::DiscordClient>) {
    let client = forge_discord::DiscordClient::new(
        forge_discord::DiscordConfig::default(),
        publisher(bus),
        creds_of(backend),
    );
    if let Err(e) = forge_discord::register_discord_sub_actions(sub_actions, Arc::clone(&client)) {
        eprintln!("forge-desktop: discord sub-action registration failed: {e}");
    }
    let object = BuiltinObject {
        icon: SectionIcon::new("brand-discord"),
        status: client.clone(),
        health: client.clone(),
        content: client.clone(),
        quick: client.clone(),
        control: None,
        obs_client: None,
    };
    (Some(object), client)
}

async fn build_midi(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
) -> (Option<BuiltinObject>, Option<Arc<forge_midi::MidiClient>>) {
    let client = match forge_midi::MidiClient::start_with_midir(
        forge_midi::MidiConfig::default(),
        publisher(bus),
    ) {
        Ok(client) => client,
        Err(e) => {
            eprintln!("forge-desktop: MIDI init failed; integration unavailable: {e}");
            return (None, None);
        }
    };
    if let Err(e) = forge_midi::register_midi_sub_actions(sub_actions, Arc::clone(&client)) {
        eprintln!("forge-desktop: midi sub-action registration failed: {e}");
    }

    let settings = Arc::clone(backend) as Arc<dyn SettingsRepo>;
    if !get_bool_setting(&*settings, MIDI_ENABLED_KEY, true).await
        && let Err(e) = client.disable_input().await
    {
        eprintln!("forge-desktop: midi input could not be disabled at boot: {e}");
    }

    let object = BuiltinObject {
        icon: SectionIcon::new("piano"),
        status: client.clone(),
        health: client.clone(),
        content: client.clone(),
        quick: client.clone(),
        control: Some(client.clone()),
        obs_client: None,
    };
    (Some(object), Some(client))
}

async fn build_hotkey(
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
    main_thread: forge_hotkey::MainThreadLink,
) -> (
    Option<BuiltinObject>,
    Option<Arc<forge_hotkey::HotkeyClient>>,
    Option<Arc<HotkeyReconciler>>,
) {
    let settings = Arc::clone(backend) as Arc<dyn SettingsRepo>;
    let config = forge_hotkey::HotkeyConfig {
        hold_ceiling_secs: load_hold_ceiling(&*settings).await,
        ..forge_hotkey::HotkeyConfig::default()
    };
    let client = forge_hotkey::HotkeyClient::new(config, publisher(bus), main_thread).await;
    let reconciler = HotkeyReconciler::new(
        Arc::clone(&client),
        backend.trigger_instance_repo(),
        backend.soundboard_clips_repo(),
    );
    reconciler.reconcile().await;

    if !get_bool_setting(&*settings, HOTKEY_ENABLED_KEY, true).await
        && let Err(e) = client.disable().await
    {
        eprintln!("forge-desktop: hotkey engine could not be disabled at boot: {e}");
    }

    let object = BuiltinObject {
        icon: SectionIcon::new("keyboard"),
        status: client.clone(),
        health: client.clone(),
        content: client.clone(),
        quick: client.clone(),
        control: Some(client.clone()),
        obs_client: None,
    };
    (Some(object), Some(client), Some(reconciler))
}

async fn build_youtube(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
) -> (
    Option<BuiltinObject>,
    Option<Box<dyn LiveViewerSource>>,
    Option<YoutubeInstallSeed>,
) {
    let Some((client_id, client_secret)) = forge_platform_youtube::client_credentials() else {
        return (None, None, None);
    };
    let google = forge_platform_youtube::GoogleAuthFlow::new(client_id, client_secret);
    let manager = Arc::new(forge_platform_youtube::YoutubeCredentialsManager::new(
        creds_of(backend),
        google,
    ));

    let live_chat_id = forge_platform_youtube::LiveChatIdHandle::new();
    let active_broadcast = forge_platform_youtube::ActiveBroadcastIdHandle::new();
    let quota = Arc::new(tokio::sync::Mutex::new(
        forge_platform_youtube::QuotaState::default(),
    ));

    let send_manager = Arc::clone(&manager);
    let send = Arc::new(forge_platform_youtube::YoutubeSendChat::new(
        Arc::new(move || {
            let manager = Arc::clone(&send_manager);
            Box::pin(async move { manager.get_valid_access_token().await })
        }),
        live_chat_id.clone(),
        Arc::clone(&quota),
    ));
    let mod_manager = Arc::clone(&manager);
    let moderation = Arc::new(forge_platform_youtube::YoutubeModeration::new(
        Arc::new(move || {
            let manager = Arc::clone(&mod_manager);
            Box::pin(async move { manager.get_valid_access_token().await })
        }),
        live_chat_id.clone(),
        Arc::clone(&quota),
    ));
    let meta_manager = Arc::clone(&manager);
    let metadata = Arc::new(forge_platform_youtube::YoutubeStreamMetadata::new(
        Arc::new(move || {
            let manager = Arc::clone(&meta_manager);
            Box::pin(async move { manager.get_valid_access_token().await })
        }),
        active_broadcast.clone(),
        Arc::clone(&quota),
    ));
    let stats_manager = Arc::clone(&manager);
    let stream_stats = Arc::new(forge_platform_youtube::YoutubeStreamStats::new(
        Arc::new(move || {
            let manager = Arc::clone(&stats_manager);
            Box::pin(async move { manager.get_valid_access_token().await })
        }),
        active_broadcast.clone(),
        Arc::clone(&quota),
    ));
    let ad_break_manager = Arc::clone(&manager);
    let ad_break = Arc::new(forge_platform_youtube::YoutubeAdBreak::new(
        Arc::new(move || {
            let manager = Arc::clone(&ad_break_manager);
            Box::pin(async move { manager.get_valid_access_token().await })
        }),
        active_broadcast.clone(),
        Arc::clone(&quota),
    ));
    let thumbnail_manager = Arc::clone(&manager);
    let thumbnail = Arc::new(forge_platform_youtube::YoutubeThumbnail::new(
        Arc::new(move || {
            let manager = Arc::clone(&thumbnail_manager);
            Box::pin(async move { manager.get_valid_access_token().await })
        }),
        active_broadcast.clone(),
        Arc::clone(&quota),
    ));
    let lookup_manager = Arc::clone(&manager);
    let channel_lookup = Arc::new(forge_platform_youtube::YoutubeChannelLookup::new(
        Arc::new(move || {
            let manager = Arc::clone(&lookup_manager);
            Box::pin(async move { manager.get_valid_access_token().await })
        }),
        Arc::clone(&quota),
    ));
    if let Err(e) = forge_platform_youtube::register_youtube_sub_actions(
        sub_actions,
        send,
        moderation,
        metadata,
        stream_stats,
        ad_break,
        thumbnail,
        channel_lookup,
    ) {
        eprintln!("forge-desktop: youtube sub-action registration failed: {e}");
    }

    let install_seed = YoutubeInstallSeed {
        manager: Arc::clone(&manager),
        live_chat_id,
        active_broadcast,
        quota,
    };

    let creds = match manager.load().await {
        Ok(Some(creds)) => creds,
        Ok(None) => return (None, None, Some(install_seed)),
        Err(e) => {
            eprintln!("forge-desktop: failed to load youtube credentials: {e}");
            return (None, None, Some(install_seed));
        }
    };

    let stack =
        assemble_youtube_stack(install_seed.clone(), Arc::clone(bus), creds.channel_id).await;
    (
        Some(youtube_builtin_object(Arc::clone(&stack.bundle))),
        Some(stack.viewer_source),
        Some(install_seed),
    )
}

pub(crate) struct YoutubeStack {
    pub(crate) bundle: Arc<forge_platform_youtube::YoutubeIntegrationBundle>,
    pub(crate) viewer_source: Box<dyn LiveViewerSource>,
}

pub(crate) async fn assemble_youtube_stack(
    seed: YoutubeInstallSeed,
    bus: Arc<EventBus>,
    channel_id: String,
) -> YoutubeStack {
    let platform = Arc::new(forge_platform_youtube::YoutubePlatform::new(
        channel_id.clone(),
        Arc::clone(&seed.manager),
        seed.live_chat_id,
        seed.active_broadcast,
        Arc::clone(&seed.quota),
    ));
    let chat_platform: Arc<dyn ChatPlatform> = Arc::clone(&platform) as _;

    let (bundle, _health_tx) = forge_platform_youtube::YoutubeIntegrationBundle::new(
        channel_id,
        Arc::clone(&platform),
        seed.manager,
        seed.quota,
    );

    spawn_event_bridge(publisher(&bus), chat_platform.events(), "youtube");
    spawn_connect(Arc::clone(&chat_platform), "youtube");
    spawn_chat_send_bridge(bus, chat_platform, "youtube", EventSource::YouTube);

    let viewer_source = bundle.viewer_source();
    YoutubeStack {
        bundle,
        viewer_source,
    }
}

async fn build_kick(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
) -> (
    Option<BuiltinObject>,
    Option<Box<dyn LiveViewerSource>>,
    Option<KickInstallSeed>,
) {
    let Some((client_id, client_secret)) = forge_platform_kick::client_credentials() else {
        return (None, None, None);
    };
    let manager = Arc::new(forge_platform_kick::KickCredentialsManager::new(
        creds_of(backend),
        client_id,
        client_secret,
    ));

    let rate_limiter: Arc<dyn RateLimiter> =
        Arc::new(TokenBucketRateLimiter::new(60, Duration::from_secs(60)));
    let platform = Arc::new(forge_platform_kick::KickPlatform::new(
        Arc::clone(&manager),
        Arc::clone(&rate_limiter),
    ));
    let chat_platform: Arc<dyn ChatPlatform> = Arc::clone(&platform) as _;

    let sender = Arc::new(forge_platform_kick::KickSendChat::new(Arc::new(
        NoopRateLimiter,
    )));
    let moderation = Arc::new(forge_platform_kick::KickModeration::new(Arc::clone(
        &rate_limiter,
    )));
    let channel = Arc::new(forge_platform_kick::KickChannel::new(Arc::clone(
        &rate_limiter,
    )));
    let rewards = Arc::new(forge_platform_kick::KickRewards::new(Arc::clone(
        &rate_limiter,
    )));
    let categories = Arc::new(forge_platform_kick::KickCategories::new(Arc::clone(
        &rate_limiter,
    )));

    let install_seed = KickInstallSeed {
        manager: Arc::clone(&manager),
        rate_limiter: Arc::clone(&rate_limiter),
        platform: Arc::clone(&platform),
        channel: Arc::clone(&channel),
        rewards: Arc::clone(&rewards),
    };

    let manager_for_sub_actions = Arc::clone(&manager);
    let manager_for_broadcaster = Arc::clone(&manager);
    if let Err(e) = forge_platform_kick::register_kick_sub_actions(
        sub_actions,
        forge_platform_kick::KickSubActionDeps {
            client: Arc::clone(&sender),
            token_source: Arc::new(move || {
                let manager = Arc::clone(&manager_for_sub_actions);
                Box::pin(async move { manager.get_valid_access_token().await })
            }),
            broadcaster_id_source: Arc::new(move || {
                let manager = Arc::clone(&manager_for_broadcaster);
                Box::pin(async move { manager.user_id().await })
            }),
            moderation,
            channel: Arc::clone(&channel),
            rewards: Arc::clone(&rewards),
            categories,
        },
    ) {
        eprintln!("forge-desktop: kick sub-action registration failed: {e}");
    }

    spawn_chat_send_bridge(
        Arc::clone(bus),
        Arc::clone(&chat_platform),
        "kick",
        EventSource::Kick,
    );

    let creds = match manager.load().await {
        Ok(Some(creds)) => creds,
        Ok(None) => return (None, None, Some(install_seed)),
        Err(e) => {
            eprintln!("forge-desktop: failed to load kick credentials: {e}");
            return (None, None, Some(install_seed));
        }
    };

    let stack = assemble_kick_stack(
        manager,
        platform,
        rate_limiter,
        channel,
        rewards,
        publisher(bus),
        creds.username,
        creds.user_id,
    )
    .await;

    (
        Some(kick_builtin_object(stack.bundle)),
        Some(stack.viewer_source),
        Some(install_seed),
    )
}

pub(crate) struct KickStack {
    pub(crate) bundle: Arc<forge_platform_kick::KickIntegrationBundle>,
    pub(crate) viewer_source: Box<dyn LiveViewerSource>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn assemble_kick_stack(
    manager: Arc<forge_platform_kick::KickCredentialsManager>,
    platform: Arc<forge_platform_kick::KickPlatform>,
    rate_limiter: Arc<dyn RateLimiter>,
    channel: Arc<forge_platform_kick::KickChannel>,
    rewards: Arc<forge_platform_kick::KickRewards>,
    bus: Arc<dyn EventPublisher>,
    slug: String,
    user_id: u64,
) -> KickStack {
    let chat_platform: Arc<dyn ChatPlatform> = Arc::clone(&platform) as _;
    spawn_event_bridge(Arc::clone(&bus), chat_platform.events(), "kick");
    spawn_connect(chat_platform, "kick");

    let (poller_tx, mut poller_rx) = tokio::sync::mpsc::channel::<Event>(256);
    let bus_poller = Arc::clone(&bus);
    tokio::spawn(async move {
        while let Some(event) = poller_rx.recv().await {
            bus_poller.publish(event);
        }
    });

    let manager_for_poller = Arc::clone(&manager);
    let viewer_source = forge_platform_kick::spawn_kick_poller(
        channel,
        rewards,
        Arc::new(move || {
            let manager = Arc::clone(&manager_for_poller);
            Box::pin(async move { manager.get_valid_access_token().await })
        }),
        poller_tx,
    );
    let viewer_report_rx = viewer_source.subscribe();

    let (bundle, _health_tx) = forge_platform_kick::KickIntegrationBundle::new(
        slug,
        user_id,
        platform,
        manager,
        rate_limiter,
        viewer_report_rx,
    );

    KickStack {
        bundle,
        viewer_source: Box::new(viewer_source),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use forge_events::EventStream;
    use forge_platform_core::{AuthFlow, ConnectionState, PlatformCapabilities};
    use forge_runtime::NullEventLogRepo;
    use std::time::Duration;
    use tokio::sync::{Semaphore, broadcast, mpsc};

    struct RecordingPlatform {
        sends: mpsc::UnboundedSender<(String, String)>,
        gate: Option<Arc<Semaphore>>,
        failure: Option<String>,
        auth: AuthFlow,
        caps: PlatformCapabilities,
    }

    impl RecordingPlatform {
        fn spawn() -> (Arc<Self>, mpsc::UnboundedReceiver<(String, String)>) {
            Self::build(None, None)
        }

        fn gated() -> (
            Arc<Self>,
            mpsc::UnboundedReceiver<(String, String)>,
            Arc<Semaphore>,
        ) {
            let gate = Arc::new(Semaphore::new(0));
            let (platform, rx) = Self::build(Some(Arc::clone(&gate)), None);
            (platform, rx, gate)
        }

        fn failing(reason: &str) -> (Arc<Self>, mpsc::UnboundedReceiver<(String, String)>) {
            Self::build(None, Some(reason.to_owned()))
        }

        fn build(
            gate: Option<Arc<Semaphore>>,
            failure: Option<String>,
        ) -> (Arc<Self>, mpsc::UnboundedReceiver<(String, String)>) {
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
            let _ = self.sends.send((channel.to_string(), text.to_string()));
            if let Some(gate) = &self.gate {
                gate.acquire().await.unwrap().forget();
            }
            match &self.failure {
                Some(reason) => Err(PlatformError::Network {
                    reason: reason.clone(),
                }),
                None => Ok(()),
            }
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

    async fn expect_send(rx: &mut mpsc::UnboundedReceiver<(String, String)>) -> (String, String) {
        tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("a send_message call was expected but never arrived")
            .expect("mock send channel closed")
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
        // Why: must exceed the event bus broadcast capacity so the bridge's receiver lags.
        const FLOOD: usize = 2_048;
        let bus = test_bus();
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
            BuiltinContent, BuiltinHealth, BuiltinId, BuiltinStatus, CapabilityFlags,
            ConnectionState, DetailSection, HeaderAction, HealthDelta, HealthMetric, HealthStream,
            HealthValue, QuickAction, QuickActions, SectionIcon,
        };
        use forge_runtime::{
            ActionCancelRegistry, spawn_action_engine, spawn_live_viewer_aggregator,
        };
        use gpui::{AppContext as _, TestAppContext};

        use super::super::{BuiltinObject, BuiltinRegistry, ObsInstallSeed, VTubeInstallSeed};
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
            id: BuiltinId,
            deltas: Mutex<Option<mpsc::UnboundedReceiver<HealthDelta>>>,
        }

        impl BuiltinStatus for HeldStreamBuiltin {
            fn id(&self) -> &BuiltinId {
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
                id: BuiltinId::new("obs"),
                deltas: Mutex::new(Some(deltas_rx)),
            });
            let object = BuiltinObject {
                icon: SectionIcon::new("bug"),
                status: probe.clone(),
                health: probe.clone(),
                content: probe.clone(),
                quick: probe,
                control: None,
                obs_client: None,
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
                    BuiltinRegistry::default(),
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
            // Why: gpui releases a dropped entity on the next effect flush; its cancelled tasks' futures drop when the executor runs next.
            cx.update(|_| {});
            cx.run_until_parked();

            assert!(
                deltas_tx.is_closed(),
                "the closed screen's health task must end without waiting for a delta"
            );
        }
    }
}
