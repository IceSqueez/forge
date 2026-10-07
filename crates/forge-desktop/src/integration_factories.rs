use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_platform_core::{
    ChatPlatform, PlatformEndpoints, RateLimiter, SectionIcon, TokenBucketRateLimiter,
};
use forge_registry::SubActionRegistry;
use forge_runtime::EventBus;
use forge_storage::{
    BanLedgerRepo, CredentialsRepo, DataProvider, SettingsRepo, SoundboardClipsRepo, StorageError,
    TriggerInstanceRepo, get_bool_setting, has_credentials_for,
};
use forge_types::{IntegrationId, PlatformId};
use futures_util::future::BoxFuture;
use tokio::sync::mpsc;

use crate::clip_hotkeys::clip_bindings_of;
use crate::hotkey_bindings::persisted_hotkey_combos;
use crate::hotkey_sync::HotkeyReconciler;
use crate::integration_supervisor::{IntegrationFactory, RunningIntegration, TaskGroup};
use crate::integrations::{
    BuiltinObject, ObsInstallSeed, VTubeInstallSeed, creds_of, kick_builtin_object, publisher,
    spawn_chat_send_bridge, spawn_connect, spawn_event_bridge, twitch_builtin_object,
    youtube_builtin_object,
};
use crate::obs_credentials_form::{OBS_AUTO_RECONNECT_KEY, OBS_CONNECT_ON_LAUNCH_KEY};
use crate::vtube_connect_form::{VTUBE_AUTO_RECONNECT_KEY, VTUBE_CONNECT_ON_LAUNCH_KEY};

const CONNECT_GUARD: Duration = Duration::from_secs(5);
const KICK_API_BUDGET_CAPACITY: u32 = 60;
const KICK_API_BUDGET_WINDOW: Duration = Duration::from_secs(60);
const KICK_POLLER_QUEUE_CAPACITY: usize = 256;

fn idle_with_tasks(tasks: TaskGroup) -> RunningIntegration {
    RunningIntegration::idle().with_teardown(Box::pin(async move { tasks.abort_all() }))
}

pub(crate) struct TwitchFactory {
    bus: Arc<EventBus>,
    endpoints: PlatformEndpoints,
    client_id: String,
    creds: Arc<dyn CredentialsRepo>,
    manager: Arc<forge_platform_twitch::TwitchCredentialsManager>,
    lifecycle: forge_platform_twitch::TwitchLifecycle,
    rate_limiter: Arc<dyn RateLimiter>,
}

pub(crate) fn wire_twitch(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
    endpoints: &PlatformEndpoints,
) -> Option<TwitchFactory> {
    let client_id = forge_platform_twitch::client_id()?;
    let creds = creds_of(backend);
    let lifecycle = forge_platform_twitch::TwitchLifecycle::new();
    let manager = Arc::new(forge_platform_twitch::TwitchCredentialsManager::new(
        endpoints,
        Arc::clone(&creds),
        client_id.clone(),
    ));
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
    Some(TwitchFactory {
        bus: Arc::clone(bus),
        endpoints: endpoints.clone(),
        client_id,
        creds,
        manager,
        lifecycle,
        rate_limiter,
    })
}

#[async_trait]
impl IntegrationFactory for TwitchFactory {
    fn id(&self) -> IntegrationId {
        forge_platform_twitch::TWITCH_INTEGRATION.id
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        has_credentials_for(self.creds.as_ref(), &self.id()).await
    }

    async fn start(&self) -> Result<RunningIntegration, String> {
        let stored = forge_platform_twitch::credentials::load(self.creds.as_ref())
            .await
            .ok()
            .flatten();
        let user_id = stored
            .as_ref()
            .map(|stored| stored.user_id.clone())
            .unwrap_or_default();
        let platform = Arc::new(forge_platform_twitch::TwitchPlatform::new(
            forge_platform_twitch::ChatSessionConfig {
                client_id: self.client_id.clone(),
                broadcaster_id: user_id.clone(),
                user_id,
                endpoints: self.endpoints.clone(),
            },
            Arc::clone(&self.creds),
            Arc::clone(&self.manager),
            forge_platform_twitch::SubscriptionTracker::default(),
            Arc::clone(&self.rate_limiter),
            self.lifecycle.clone(),
        ));
        let chat_platform: Arc<dyn ChatPlatform> = Arc::clone(&platform) as _;
        spawn_event_bridge(publisher(&self.bus), chat_platform.events(), "twitch");
        let mut tasks = spawn_chat_send_bridge(
            Arc::clone(&self.bus),
            Arc::clone(&chat_platform),
            "twitch",
            EventSource::Twitch,
        );
        let Some(stored) = stored else {
            return Ok(idle_with_tasks(tasks));
        };
        let login = (!stored.login.is_empty()).then_some(stored.login);
        let bundle = forge_platform_twitch::TwitchIntegrationBundle::new(login, platform);
        tasks.track(spawn_connect(chat_platform, "twitch"));
        let viewers = bundle.viewer_source();
        let object = twitch_builtin_object(Arc::clone(&bundle));
        let teardown = Box::pin(async move {
            tasks.abort_all();
            bundle.shutdown().await;
        });
        Ok(RunningIntegration::idle()
            .with_object(object)
            .with_viewers(PlatformId::Twitch, viewers)
            .with_teardown(teardown))
    }
}

pub(crate) struct YoutubeFactory {
    endpoints: PlatformEndpoints,
    bus: Arc<EventBus>,
    creds: Arc<dyn CredentialsRepo>,
    manager: Arc<forge_platform_youtube::YoutubeCredentialsManager>,
    live_chat_id: forge_platform_youtube::LiveChatIdHandle,
    active_broadcast: forge_platform_youtube::ActiveBroadcastIdHandle,
    quota: forge_platform_youtube::SharedQuota,
    ban_ledger: Arc<dyn BanLedgerRepo>,
}

pub(crate) fn wire_youtube(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
    endpoints: &PlatformEndpoints,
) -> Option<YoutubeFactory> {
    let (client_id, client_secret) = forge_platform_youtube::client_credentials()?;
    let google = forge_platform_youtube::GoogleAuthFlow::new(endpoints, client_id, client_secret);
    let creds = creds_of(backend);
    let manager = Arc::new(forge_platform_youtube::YoutubeCredentialsManager::new(
        Arc::clone(&creds),
        google,
    ));
    let live_chat_id = forge_platform_youtube::LiveChatIdHandle::new();
    let active_broadcast = forge_platform_youtube::ActiveBroadcastIdHandle::new();
    let quota = forge_platform_youtube::SharedQuota::default();
    let ban_ledger = backend.ban_ledger_repo();

    let token_source = || {
        let manager = Arc::clone(&manager);
        Arc::new(move || {
            let manager = Arc::clone(&manager);
            Box::pin(async move { manager.get_valid_access_token().await }) as BoxFuture<'static, _>
        })
    };
    let send = Arc::new(forge_platform_youtube::YoutubeSendChat::new(
        endpoints,
        token_source(),
        live_chat_id.clone(),
        quota.clone(),
    ));
    let broadcaster_source = {
        let manager = Arc::clone(&manager);
        Arc::new(move || {
            let manager = Arc::clone(&manager);
            Box::pin(async move { manager.broadcaster().await }) as BoxFuture<'static, _>
        })
    };
    let moderation = Arc::new(forge_platform_youtube::YoutubeModeration::new(
        endpoints,
        token_source(),
        broadcaster_source,
        live_chat_id.clone(),
        quota.clone(),
        Arc::clone(&ban_ledger),
    ));
    let metadata = Arc::new(forge_platform_youtube::YoutubeStreamMetadata::new(
        endpoints,
        token_source(),
        active_broadcast.clone(),
        quota.clone(),
    ));
    let stream_stats = Arc::new(forge_platform_youtube::YoutubeStreamStats::new(
        endpoints,
        token_source(),
        active_broadcast.clone(),
        quota.clone(),
    ));
    let ad_break = Arc::new(forge_platform_youtube::YoutubeAdBreak::new(
        endpoints,
        token_source(),
        active_broadcast.clone(),
        quota.clone(),
    ));
    let thumbnail = Arc::new(forge_platform_youtube::YoutubeThumbnail::new(
        endpoints,
        token_source(),
        active_broadcast.clone(),
        quota.clone(),
    ));
    let channel_lookup = Arc::new(forge_platform_youtube::YoutubeChannelLookup::new(
        endpoints,
        token_source(),
        quota.clone(),
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
    Some(YoutubeFactory {
        endpoints: endpoints.clone(),
        bus: Arc::clone(bus),
        creds,
        manager,
        live_chat_id,
        active_broadcast,
        quota,
        ban_ledger,
    })
}

#[async_trait]
impl IntegrationFactory for YoutubeFactory {
    fn id(&self) -> IntegrationId {
        forge_platform_youtube::YOUTUBE_INTEGRATION.id
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        has_credentials_for(self.creds.as_ref(), &self.id()).await
    }

    async fn start(&self) -> Result<RunningIntegration, String> {
        let Some(stored) = self.manager.load().await.map_err(|e| e.to_string())? else {
            return Ok(RunningIntegration::idle());
        };
        let platform = Arc::new(forge_platform_youtube::YoutubePlatform::new(
            &self.endpoints,
            stored.channel_id.clone(),
            Arc::clone(&self.manager),
            self.live_chat_id.clone(),
            self.active_broadcast.clone(),
            self.quota.clone(),
            Arc::clone(&self.ban_ledger),
        ));
        let chat_platform: Arc<dyn ChatPlatform> = Arc::clone(&platform) as _;
        let (bundle, _health_tx) = forge_platform_youtube::YoutubeIntegrationBundle::new(
            stored.channel_id,
            platform,
            Arc::clone(&self.manager),
            self.quota.clone(),
        );

        let mut tasks = TaskGroup::default();
        tasks.track(spawn_event_bridge(
            publisher(&self.bus),
            chat_platform.events(),
            "youtube",
        ));
        tasks.track(spawn_connect(Arc::clone(&chat_platform), "youtube"));
        tasks.absorb(spawn_chat_send_bridge(
            Arc::clone(&self.bus),
            chat_platform,
            "youtube",
            EventSource::YouTube,
        ));

        let viewers = bundle.viewer_source();
        let object = youtube_builtin_object(Arc::clone(&bundle));
        let teardown = Box::pin(async move {
            tasks.abort_all();
            bundle.shutdown().await;
        });
        Ok(RunningIntegration::idle()
            .with_object(object)
            .with_viewers(PlatformId::YouTube, viewers)
            .with_teardown(teardown))
    }
}

pub(crate) struct KickFactory {
    bus: Arc<EventBus>,
    creds: Arc<dyn CredentialsRepo>,
    manager: Arc<forge_platform_kick::KickCredentialsManager>,
    rate_limiter: Arc<dyn RateLimiter>,
    platform: Arc<forge_platform_kick::KickPlatform>,
    channel: Arc<forge_platform_kick::KickChannel>,
    rewards: Arc<forge_platform_kick::KickRewards>,
}

pub(crate) fn wire_kick(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
    endpoints: &PlatformEndpoints,
) -> Option<KickFactory> {
    let (client_id, client_secret) = forge_platform_kick::client_credentials()?;
    let creds = creds_of(backend);
    let manager = Arc::new(forge_platform_kick::KickCredentialsManager::new(
        endpoints,
        Arc::clone(&creds),
        client_id,
        client_secret,
    ));
    let rate_limiter: Arc<dyn RateLimiter> = Arc::new(TokenBucketRateLimiter::new(
        KICK_API_BUDGET_CAPACITY,
        KICK_API_BUDGET_WINDOW,
    ));
    let ban_ledger = backend.ban_ledger_repo();
    let platform = Arc::new(forge_platform_kick::KickPlatform::new(
        endpoints,
        Arc::clone(&manager),
        Arc::clone(&rate_limiter),
        Arc::clone(&ban_ledger),
    ));
    let sender = Arc::new(forge_platform_kick::KickSendChat::new(
        endpoints,
        Arc::clone(&rate_limiter),
    ));
    let moderation = Arc::new(forge_platform_kick::KickModeration::new(
        endpoints,
        Arc::clone(&rate_limiter),
        ban_ledger,
    ));
    let channel = Arc::new(forge_platform_kick::KickChannel::new(
        endpoints,
        Arc::clone(&rate_limiter),
    ));
    let rewards = Arc::new(forge_platform_kick::KickRewards::new(
        endpoints,
        Arc::clone(&rate_limiter),
    ));
    let categories = Arc::new(forge_platform_kick::KickCategories::new(
        endpoints,
        Arc::clone(&rate_limiter),
    ));

    let manager_for_sub_actions = Arc::clone(&manager);
    let manager_for_broadcaster = Arc::clone(&manager);
    if let Err(e) = forge_platform_kick::register_kick_sub_actions(
        sub_actions,
        forge_platform_kick::KickSubActionDeps {
            client: sender,
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
    Some(KickFactory {
        bus: Arc::clone(bus),
        creds,
        manager,
        rate_limiter,
        platform,
        channel,
        rewards,
    })
}

#[async_trait]
impl IntegrationFactory for KickFactory {
    fn id(&self) -> IntegrationId {
        forge_platform_kick::KICK_INTEGRATION.id
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        has_credentials_for(self.creds.as_ref(), &self.id()).await
    }

    async fn start(&self) -> Result<RunningIntegration, String> {
        let chat_platform: Arc<dyn ChatPlatform> = Arc::clone(&self.platform) as _;
        let mut tasks = spawn_chat_send_bridge(
            Arc::clone(&self.bus),
            Arc::clone(&chat_platform),
            "kick",
            EventSource::Kick,
        );
        let stored = match self.manager.load().await {
            Ok(Some(stored)) => stored,
            Ok(None) => return Ok(idle_with_tasks(tasks)),
            Err(e) => {
                tasks.abort_all();
                return Err(e.to_string());
            }
        };

        let bus = publisher(&self.bus);
        tasks.track(spawn_event_bridge(
            Arc::clone(&bus),
            chat_platform.events(),
            "kick",
        ));
        tasks.track(spawn_connect(chat_platform, "kick"));

        let (poller_tx, mut poller_rx) = mpsc::channel::<Event>(KICK_POLLER_QUEUE_CAPACITY);
        tasks.track(tokio::spawn(async move {
            while let Some(event) = poller_rx.recv().await {
                bus.publish(event);
            }
        }));
        let poller_manager = Arc::clone(&self.manager);
        let (viewer_source, poller) = forge_platform_kick::spawn_kick_poller(
            Arc::clone(&self.channel),
            Arc::clone(&self.rewards),
            Arc::new(move || {
                let manager = Arc::clone(&poller_manager);
                Box::pin(async move { manager.get_valid_access_token().await })
            }),
            poller_tx,
        );
        let viewer_report_rx = viewer_source.subscribe();
        let poller_auth_rx = viewer_source.subscribe_auth();
        let (bundle, _health_tx) = forge_platform_kick::KickIntegrationBundle::new(
            stored.username,
            stored.user_id,
            Arc::clone(&self.platform),
            Arc::clone(&self.manager),
            Arc::clone(&self.rate_limiter),
            viewer_report_rx,
            poller_auth_rx,
        );

        let object = kick_builtin_object(Arc::clone(&bundle));
        let teardown = Box::pin(async move {
            poller.stop();
            tasks.abort_all();
            bundle.shutdown().await;
        });
        Ok(RunningIntegration::idle()
            .with_object(object)
            .with_viewers(PlatformId::Kick, Box::new(viewer_source))
            .with_teardown(teardown))
    }
}

pub(crate) struct ObsFactory {
    bus: Arc<EventBus>,
    creds: Arc<dyn CredentialsRepo>,
    settings: Arc<dyn SettingsRepo>,
    seed: ObsInstallSeed,
}

pub(crate) fn wire_obs(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
) -> ObsFactory {
    let sink = forge_obs::SwitchableObsSink::new();
    if let Err(e) = forge_obs::register_obs_sub_actions(
        sub_actions,
        Arc::clone(&sink) as Arc<dyn forge_obs::ObsSink>,
    ) {
        eprintln!("forge-desktop: obs sub-action registration failed: {e}");
    }
    ObsFactory {
        bus: Arc::clone(bus),
        creds: creds_of(backend),
        settings: Arc::clone(backend) as Arc<dyn SettingsRepo>,
        seed: ObsInstallSeed::new(sink),
    }
}

impl ObsFactory {
    pub(crate) fn seed(&self) -> ObsInstallSeed {
        self.seed.clone()
    }
}

#[async_trait]
impl IntegrationFactory for ObsFactory {
    fn id(&self) -> IntegrationId {
        forge_obs::OBS_INTEGRATION.id
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        has_credentials_for(self.creds.as_ref(), &self.id()).await
    }

    async fn start(&self) -> Result<RunningIntegration, String> {
        let seed = self.seed.clone();
        let running = RunningIntegration::idle().with_teardown(Box::pin({
            let seed = seed.clone();
            async move { seed.disconnect_live().await }
        }));
        if seed.live().is_some()
            || !get_bool_setting(self.settings.as_ref(), OBS_CONNECT_ON_LAUNCH_KEY, true).await
        {
            return Ok(running);
        }
        let connect =
            forge_obs::credentials::load_and_connect(self.creds.as_ref(), publisher(&self.bus));
        match tokio::time::timeout(CONNECT_GUARD, connect).await {
            Ok(Ok(client)) => {
                client.set_auto_reconnect(
                    get_bool_setting(self.settings.as_ref(), OBS_AUTO_RECONNECT_KEY, true).await,
                );
                seed.install(client);
            }
            Ok(Err(_)) => {}
            Err(_) => eprintln!("forge-desktop: obs connect timed out"),
        }
        Ok(running)
    }
}

pub(crate) struct VTubeFactory {
    bus: Arc<EventBus>,
    creds: Arc<dyn CredentialsRepo>,
    settings: Arc<dyn SettingsRepo>,
    seed: VTubeInstallSeed,
}

pub(crate) fn wire_vtube(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
) -> VTubeFactory {
    let sink = forge_vtube::SwitchableVTubeSink::new();
    if let Err(e) = forge_vtube::register_vtube_sub_actions(
        sub_actions,
        Arc::clone(&sink) as Arc<dyn forge_vtube::VTubeSink>,
    ) {
        eprintln!("forge-desktop: vtube sub-action registration failed: {e}");
    }
    VTubeFactory {
        bus: Arc::clone(bus),
        creds: creds_of(backend),
        settings: Arc::clone(backend) as Arc<dyn SettingsRepo>,
        seed: VTubeInstallSeed::new(sink),
    }
}

impl VTubeFactory {
    pub(crate) fn seed(&self) -> VTubeInstallSeed {
        self.seed.clone()
    }
}

#[async_trait]
impl IntegrationFactory for VTubeFactory {
    fn id(&self) -> IntegrationId {
        forge_vtube::VTUBE_INTEGRATION.id
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        has_credentials_for(self.creds.as_ref(), &self.id()).await
    }

    async fn start(&self) -> Result<RunningIntegration, String> {
        let seed = self.seed.clone();
        let running = RunningIntegration::idle().with_teardown(Box::pin({
            let seed = seed.clone();
            async move { seed.disconnect_live().await }
        }));
        if seed.live().is_some()
            || !get_bool_setting(self.settings.as_ref(), VTUBE_CONNECT_ON_LAUNCH_KEY, true).await
        {
            return Ok(running);
        }
        let connect = forge_vtube::credentials::load_and_connect(
            self.creds.as_ref(),
            publisher(&self.bus),
            Arc::clone(&self.creds),
        );
        match tokio::time::timeout(CONNECT_GUARD, connect).await {
            Ok(Ok(client)) => {
                client.set_auto_reconnect(
                    get_bool_setting(self.settings.as_ref(), VTUBE_AUTO_RECONNECT_KEY, true).await,
                );
                seed.install(client);
            }
            Ok(Err(_)) => {}
            Err(_) => eprintln!("forge-desktop: vtube connect timed out"),
        }
        Ok(running)
    }
}

pub(crate) struct DiscordFactory {
    creds: Arc<dyn CredentialsRepo>,
    client: Arc<forge_discord::DiscordClient>,
}

pub(crate) fn wire_discord(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
) -> DiscordFactory {
    let client = forge_discord::DiscordClient::new(
        forge_discord::DiscordConfig::default(),
        publisher(bus),
        creds_of(backend),
    );
    if let Err(e) = forge_discord::register_discord_sub_actions(sub_actions, Arc::clone(&client)) {
        eprintln!("forge-desktop: discord sub-action registration failed: {e}");
    }
    DiscordFactory {
        creds: creds_of(backend),
        client,
    }
}

impl DiscordFactory {
    pub(crate) fn client(&self) -> Arc<forge_discord::DiscordClient> {
        Arc::clone(&self.client)
    }
}

#[async_trait]
impl IntegrationFactory for DiscordFactory {
    fn id(&self) -> IntegrationId {
        forge_discord::DISCORD_INTEGRATION.id
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        has_credentials_for(self.creds.as_ref(), &self.id()).await
    }

    async fn start(&self) -> Result<RunningIntegration, String> {
        let client = Arc::clone(&self.client);
        Ok(RunningIntegration::idle().with_object(BuiltinObject {
            icon: SectionIcon::new("brand-discord"),
            status: client.clone(),
            health: client.clone(),
            content: client.clone(),
            quick: client,
            control: None,
            collections: None,
            obs_client: None,
            vtube_client: None,
            follow: None,
            ban_list: None,
        }))
    }
}

pub(crate) struct MidiFactory {
    bus: Arc<EventBus>,
    triggers: Arc<dyn TriggerInstanceRepo>,
    owned_kinds: Vec<String>,
    sink: Arc<forge_midi::SwitchableMidiSink>,
}

pub(crate) fn wire_midi(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
    owned_kinds: Vec<String>,
) -> MidiFactory {
    let sink = forge_midi::SwitchableMidiSink::new();
    if let Err(e) = forge_midi::register_midi_sub_actions(
        sub_actions,
        Arc::clone(&sink) as Arc<dyn forge_midi::MidiSink>,
    ) {
        eprintln!("forge-desktop: midi sub-action registration failed: {e}");
    }
    MidiFactory {
        bus: Arc::clone(bus),
        triggers: backend.trigger_instance_repo(),
        owned_kinds,
        sink,
    }
}

impl MidiFactory {
    pub(crate) fn sink(&self) -> Arc<forge_midi::SwitchableMidiSink> {
        Arc::clone(&self.sink)
    }
}

#[async_trait]
impl IntegrationFactory for MidiFactory {
    fn id(&self) -> IntegrationId {
        forge_midi::MIDI_INTEGRATION.id
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        Ok(self
            .triggers
            .list_user_defined()
            .await?
            .iter()
            .any(|instance| self.owned_kinds.contains(&instance.kind_id)))
    }

    async fn start(&self) -> Result<RunningIntegration, String> {
        let client = forge_midi::MidiClient::start_with_midir(
            forge_midi::MidiConfig::default(),
            publisher(&self.bus),
        )
        .map_err(|e| e.to_string())?;
        self.sink.install(Arc::clone(&client));
        let object = BuiltinObject {
            icon: SectionIcon::new("piano"),
            status: client.clone(),
            health: client.clone(),
            content: client.clone(),
            quick: client.clone(),
            control: Some(client),
            collections: None,
            obs_client: None,
            vtube_client: None,
            follow: None,
            ban_list: None,
        };
        let sink = Arc::clone(&self.sink);
        let teardown = Box::pin(async move {
            if let Some(client) = sink.take()
                && let Err(e) = client.shutdown().await
            {
                tracing::warn!(error = %e, "midi engine did not acknowledge shutdown");
            }
        });
        Ok(RunningIntegration::idle()
            .with_object(object)
            .with_teardown(teardown))
    }
}

pub(crate) struct HotkeyFactory {
    client: Arc<forge_hotkey::HotkeyClient>,
    triggers: Arc<dyn TriggerInstanceRepo>,
    clips: Arc<dyn SoundboardClipsRepo>,
}

pub(crate) async fn wire_hotkey(
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
    main_thread: forge_hotkey::MainThreadLink,
) -> (HotkeyFactory, Arc<HotkeyReconciler>) {
    let settings = Arc::clone(backend) as Arc<dyn SettingsRepo>;
    let config = forge_hotkey::HotkeyConfig {
        hold_ceiling_secs: crate::hotkey_bindings::load_hold_ceiling(settings.as_ref()).await,
        ..forge_hotkey::HotkeyConfig::default()
    };
    let client = forge_hotkey::HotkeyClient::new(config, publisher(bus), main_thread).await;
    if let Err(e) = client.disable().await {
        eprintln!("forge-desktop: hotkey engine could not be held idle at boot: {e}");
    }
    let reconciler = HotkeyReconciler::new(
        Arc::clone(&client),
        backend.trigger_instance_repo(),
        backend.soundboard_clips_repo(),
    );
    reconciler.reconcile().await;
    let factory = HotkeyFactory {
        client,
        triggers: backend.trigger_instance_repo(),
        clips: backend.soundboard_clips_repo(),
    };
    (factory, reconciler)
}

impl HotkeyFactory {
    pub(crate) fn client(&self) -> Arc<forge_hotkey::HotkeyClient> {
        Arc::clone(&self.client)
    }
}

#[async_trait]
impl IntegrationFactory for HotkeyFactory {
    fn id(&self) -> IntegrationId {
        forge_hotkey::HOTKEY_INTEGRATION.id
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        let instances = self.triggers.list_all().await?;
        if !persisted_hotkey_combos(&instances).is_empty() {
            return Ok(true);
        }
        Ok(!clip_bindings_of(&self.clips.list().await?).is_empty())
    }

    async fn start(&self) -> Result<RunningIntegration, String> {
        let failures = self.client.enable().await.map_err(|e| e.to_string())?;
        if !failures.is_empty() {
            tracing::warn!(
                failed = failures.len(),
                "some hotkey bindings could not be claimed"
            );
        }
        let client = Arc::clone(&self.client);
        let object = BuiltinObject {
            icon: SectionIcon::new("keyboard"),
            status: client.clone(),
            health: client.clone(),
            content: client.clone(),
            quick: client.clone(),
            control: Some(client.clone()),
            collections: None,
            obs_client: None,
            vtube_client: None,
            follow: None,
            ban_list: None,
        };
        let teardown = Box::pin(async move {
            if let Err(e) = client.disable().await {
                tracing::warn!(error = %e, "hotkey engine did not release its bindings");
            }
        });
        Ok(RunningIntegration::idle()
            .with_object(object)
            .with_teardown(teardown))
    }
}
