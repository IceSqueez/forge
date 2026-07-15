use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventPublisher, EventSource, EventStream, EventsError};
use forge_platform_core::{
    BuiltinContent, BuiltinControl, BuiltinHealth, BuiltinId, BuiltinStatus, ChatPlatform,
    LiveViewerSource, PlatformError, QuickActions, RateLimitOutcome, RateLimiter, SectionIcon,
    TokenBucketRateLimiter,
};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::EventBus;
use forge_storage::{CredentialsRepo, DataProvider};

const CONNECT_GUARD: Duration = Duration::from_secs(5);

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

pub struct Integrations {
    pub builtins: HashMap<BuiltinId, BuiltinObject>,
    pub viewer_sources: Vec<Box<dyn LiveViewerSource>>,
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
) -> Integrations {
    register_platform_triggers(triggers);

    let mut builtins: HashMap<BuiltinId, BuiltinObject> = HashMap::new();
    let mut viewer_sources: Vec<Box<dyn LiveViewerSource>> = Vec::new();

    let mut insert = |id: &str, object: Option<BuiltinObject>| {
        if let Some(object) = object {
            builtins.insert(BuiltinId::new(id), object);
        }
    };

    insert("twitch", build_twitch(sub_actions, backend, bus).await);
    insert("obs", build_obs(sub_actions, backend, bus).await);
    insert("vtube", build_vtube(sub_actions, backend, bus).await);
    insert("discord", build_discord(sub_actions, backend, bus));
    insert("midi", build_midi(sub_actions, bus));
    insert("hotkey", build_hotkey(backend, bus).await);

    let (youtube, youtube_viewers) = build_youtube(sub_actions, backend, bus).await;
    if let Some(source) = youtube_viewers {
        viewer_sources.push(source);
    }
    insert("youtube", youtube);

    let (kick, kick_viewers) = build_kick(sub_actions, backend, bus).await;
    if let Some(source) = kick_viewers {
        viewer_sources.push(source);
    }
    insert("kick", kick);

    Integrations {
        builtins,
        viewer_sources,
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

fn spawn_event_bridge(bus: Arc<EventBus>, mut events: EventStream, label: &'static str) {
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

fn spawn_chat_send_bridge(
    bus: Arc<EventBus>,
    platform: Arc<dyn ChatPlatform>,
    target: &'static str,
    source: EventSource,
) {
    tokio::spawn(async move {
        let mut sub = bus.subscribe();
        loop {
            let event = match sub.recv().await {
                Ok(e) => e,
                Err(EventsError::BusClosed) => break,
                Err(EventsError::LaggingReceiver) => continue,
                Err(_) => continue,
            };
            if event.source != EventSource::Core || event.kind != "chat.send.request" {
                continue;
            }
            if event.payload.get("target").and_then(|v| v.as_str()) != Some(target) {
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
            match platform.send_message(target, &message).await {
                Ok(()) => bus.publish(Event::caused_by(
                    source,
                    "chat.send",
                    serde_json::json!({ "channel": target, "message": message }),
                    caused_by,
                )),
                Err(e) => bus.publish(Event::caused_by(
                    source,
                    "chat.send.failed",
                    serde_json::json!({ "target": target, "error": e.to_string() }),
                    caused_by,
                )),
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
) -> Option<BuiltinObject> {
    let client_id = forge_platform_twitch::client_id()?;
    let creds = creds_of(backend);

    let rate_limiter: Arc<dyn RateLimiter> =
        Arc::new(TokenBucketRateLimiter::new(800, Duration::from_secs(60)));
    let manager = Arc::new(forge_platform_twitch::TwitchCredentialsManager::new(
        Arc::clone(&creds),
        client_id.clone(),
    ));
    let transport: Arc<dyn forge_platform_twitch::HelixTransport> =
        Arc::new(
            forge_platform_twitch::HelixHttpTransport::new(
                rate_limiter,
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
    ) {
        eprintln!("forge-desktop: twitch sub-action registration failed: {e}");
    }

    let stored = forge_platform_twitch::credentials::load(&*creds)
        .await
        .ok()
        .flatten()?;

    let login = (!stored.login.is_empty()).then(|| stored.login.clone());
    let tracker = forge_platform_twitch::SubscriptionTracker::default();
    let config = forge_platform_twitch::ChatSessionConfig {
        client_id,
        broadcaster_id: stored.user_id.clone(),
        user_id: stored.user_id,
    };
    let chat = forge_platform_twitch::TwitchChat::new(
        stored.access_token,
        config.client_id.clone(),
        config.broadcaster_id.clone(),
        config.user_id.clone(),
        publisher(bus),
        tracker.clone(),
    );
    let handle = chat.start();
    let bundle = forge_platform_twitch::TwitchIntegrationBundle::new(
        login,
        config,
        publisher(bus),
        creds,
        tracker,
        handle,
    );

    Some(BuiltinObject {
        icon: SectionIcon::new("brand-twitch"),
        status: bundle.clone(),
        health: bundle.clone(),
        content: bundle.clone(),
        quick: bundle.clone(),
        control: Some(bundle as Arc<dyn BuiltinControl>),
        obs_client: None,
    })
}

async fn build_obs(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
) -> Option<BuiltinObject> {
    let sink = forge_obs::SwitchableObsSink::new();
    if let Err(e) = forge_obs::register_obs_sub_actions(
        sub_actions,
        Arc::clone(&sink) as Arc<dyn forge_obs::ObsSink>,
    ) {
        eprintln!("forge-desktop: obs sub-action registration failed: {e}");
    }

    let creds = creds_of(backend);
    let connect = forge_obs::credentials::load_and_connect(&*creds, publisher(bus));
    let client = match tokio::time::timeout(CONNECT_GUARD, connect).await {
        Ok(Ok(client)) => client,
        Ok(Err(_)) => return None,
        Err(_) => {
            eprintln!("forge-desktop: obs connect timed out");
            return None;
        }
    };
    sink.install(Arc::clone(&client));

    Some(BuiltinObject {
        icon: SectionIcon::new("broadcast"),
        status: client.clone(),
        health: client.clone(),
        content: client.clone(),
        quick: client.clone(),
        control: Some(Arc::clone(&client) as Arc<dyn BuiltinControl>),
        obs_client: Some(client),
    })
}

async fn build_vtube(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
) -> Option<BuiltinObject> {
    let sink = forge_vtube::SwitchableVTubeSink::new();
    if let Err(e) = forge_vtube::register_vtube_sub_actions(
        sub_actions,
        Arc::clone(&sink) as Arc<dyn forge_vtube::VTubeSink>,
    ) {
        eprintln!("forge-desktop: vtube sub-action registration failed: {e}");
    }

    let creds = creds_of(backend);
    let connect =
        forge_vtube::credentials::load_and_connect(&*creds, publisher(bus), Arc::clone(&creds));
    let client = match tokio::time::timeout(CONNECT_GUARD, connect).await {
        Ok(Ok(client)) => client,
        Ok(Err(_)) => return None,
        Err(_) => {
            eprintln!("forge-desktop: vtube connect timed out");
            return None;
        }
    };
    sink.install(Arc::clone(&client));

    Some(BuiltinObject {
        icon: SectionIcon::new("mood-smile"),
        status: client.clone(),
        health: client.clone(),
        content: client.clone(),
        quick: client.clone(),
        control: Some(client as Arc<dyn BuiltinControl>),
        obs_client: None,
    })
}

fn build_discord(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
) -> Option<BuiltinObject> {
    let client = forge_discord::DiscordClient::new(
        forge_discord::DiscordConfig::default(),
        publisher(bus),
        creds_of(backend),
    );
    if let Err(e) = forge_discord::register_discord_sub_actions(sub_actions, Arc::clone(&client)) {
        eprintln!("forge-desktop: discord sub-action registration failed: {e}");
    }
    Some(BuiltinObject {
        icon: SectionIcon::new("brand-discord"),
        status: client.clone(),
        health: client.clone(),
        content: client.clone(),
        quick: client,
        control: None,
        obs_client: None,
    })
}

fn build_midi(sub_actions: &mut SubActionRegistry, bus: &Arc<EventBus>) -> Option<BuiltinObject> {
    let client = match forge_midi::MidiClient::start_with_midir(
        forge_midi::MidiConfig::default(),
        publisher(bus),
    ) {
        Ok(client) => client,
        Err(e) => {
            eprintln!("forge-desktop: MIDI init failed; integration unavailable: {e}");
            return None;
        }
    };
    if let Err(e) = forge_midi::register_midi_sub_actions(sub_actions, Arc::clone(&client)) {
        eprintln!("forge-desktop: midi sub-action registration failed: {e}");
    }
    Some(BuiltinObject {
        icon: SectionIcon::new("piano"),
        status: client.clone(),
        health: client.clone(),
        content: client.clone(),
        quick: client,
        control: None,
        obs_client: None,
    })
}

async fn build_hotkey(
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
) -> Option<BuiltinObject> {
    let client =
        forge_hotkey::HotkeyClient::new(forge_hotkey::HotkeyConfig::default(), publisher(bus))
            .await;
    reregister_persisted_hotkeys(&client, backend).await;
    Some(BuiltinObject {
        icon: SectionIcon::new("keyboard"),
        status: client.clone(),
        health: client.clone(),
        content: client.clone(),
        quick: client,
        control: None,
        obs_client: None,
    })
}

async fn reregister_persisted_hotkeys(
    client: &Arc<forge_hotkey::HotkeyClient>,
    backend: &Arc<dyn DataProvider>,
) {
    const HOTKEY_PRESSED_KIND: &str = "hotkey.global.pressed";
    let instances = match backend.trigger_instance_repo().list_all().await {
        Ok(instances) => instances,
        Err(e) => {
            eprintln!("forge-desktop: failed to load persisted hotkey bindings: {e}");
            return;
        }
    };
    for instance in instances {
        if instance.kind_id != HOTKEY_PRESSED_KIND {
            continue;
        }
        let Some(forge_types::Variant::String(combo_str)) = instance.overrides.get("combo") else {
            continue;
        };
        let combo = match forge_hotkey::HotkeyCombo::parse(combo_str) {
            Ok(combo) => combo,
            Err(e) => {
                eprintln!(
                    "forge-desktop: persisted hotkey combo '{combo_str}' failed to parse: {e}"
                );
                continue;
            }
        };
        if let Err(e) = client.register(combo).await {
            eprintln!("forge-desktop: failed to re-register hotkey '{combo_str}': {e}");
        }
    }
}

async fn build_youtube(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
) -> (Option<BuiltinObject>, Option<Box<dyn LiveViewerSource>>) {
    let Some((client_id, client_secret)) = forge_platform_youtube::client_credentials() else {
        return (None, None);
    };
    let google = forge_platform_youtube::GoogleAuthFlow::new(client_id, client_secret);
    let manager = Arc::new(forge_platform_youtube::YoutubeCredentialsManager::new(
        creds_of(backend),
        google,
    ));
    let creds = match manager.load().await {
        Ok(Some(creds)) => creds,
        Ok(None) => return (None, None),
        Err(e) => {
            eprintln!("forge-desktop: failed to load youtube credentials: {e}");
            return (None, None);
        }
    };
    let channel_id = creds.channel_id.clone();

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
    if let Err(e) = forge_platform_youtube::register_youtube_sub_actions(
        sub_actions,
        send,
        moderation,
        metadata,
    ) {
        eprintln!("forge-desktop: youtube sub-action registration failed: {e}");
    }

    let platform = Arc::new(forge_platform_youtube::YoutubePlatform::new(
        channel_id.clone(),
        Arc::clone(&manager),
        live_chat_id,
        active_broadcast,
        Arc::clone(&quota),
    ));
    let chat_platform: Arc<dyn ChatPlatform> = Arc::clone(&platform) as _;

    let (bundle, _health_tx) = forge_platform_youtube::YoutubeIntegrationBundle::new(
        channel_id,
        Arc::clone(&platform),
        manager,
        quota,
    );

    spawn_event_bridge(Arc::clone(bus), chat_platform.events(), "youtube");
    spawn_connect(Arc::clone(&chat_platform), "youtube");
    spawn_chat_send_bridge(
        Arc::clone(bus),
        chat_platform,
        "youtube",
        EventSource::YouTube,
    );

    let viewer_source = bundle.viewer_source();
    let object = BuiltinObject {
        icon: SectionIcon::new("brand-youtube"),
        status: bundle.clone(),
        health: bundle.clone(),
        content: bundle.clone(),
        quick: bundle.clone(),
        control: Some(bundle as Arc<dyn BuiltinControl>),
        obs_client: None,
    };
    (Some(object), Some(viewer_source))
}

async fn build_kick(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
) -> (Option<BuiltinObject>, Option<Box<dyn LiveViewerSource>>) {
    let Some(client_id) = forge_platform_kick::client_credentials() else {
        return (None, None);
    };
    let http = reqwest::Client::new();
    let manager = Arc::new(forge_platform_kick::KickCredentialsManager::new(
        creds_of(backend),
        http.clone(),
        client_id,
    ));
    let creds = match manager.load().await {
        Ok(Some(creds)) => creds,
        Ok(None) => return (None, None),
        Err(e) => {
            eprintln!("forge-desktop: failed to load kick credentials: {e}");
            return (None, None);
        }
    };
    let slug = creds.username.clone();
    let broadcaster_user_id = creds.user_id;

    let rate_limiter: Arc<dyn RateLimiter> =
        Arc::new(TokenBucketRateLimiter::new(60, Duration::from_secs(60)));
    let platform = Arc::new(forge_platform_kick::KickPlatform::new(
        slug.clone(),
        Arc::clone(&manager),
        Arc::clone(&rate_limiter),
    ));
    let chat_platform: Arc<dyn ChatPlatform> = Arc::clone(&platform) as _;

    spawn_event_bridge(Arc::clone(bus), chat_platform.events(), "kick");
    spawn_connect(Arc::clone(&chat_platform), "kick");

    let (poller_tx, mut poller_rx) = tokio::sync::mpsc::channel::<Event>(256);
    let bus_poller = Arc::clone(bus);
    tokio::spawn(async move {
        while let Some(event) = poller_rx.recv().await {
            bus_poller.publish(event);
        }
    });

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

    let manager_for_sub_actions = Arc::clone(&manager);
    if let Err(e) = forge_platform_kick::register_kick_sub_actions(
        sub_actions,
        forge_platform_kick::KickSubActionDeps {
            client: Arc::clone(&sender),
            token_source: Arc::new(move || {
                let manager = Arc::clone(&manager_for_sub_actions);
                Box::pin(async move { manager.get_valid_access_token().await })
            }),
            broadcaster_user_id,
            moderation,
            channel: Arc::clone(&channel),
            rewards: Arc::clone(&rewards),
        },
    ) {
        eprintln!("forge-desktop: kick sub-action registration failed: {e}");
    }

    let manager_for_poller = Arc::clone(&manager);
    forge_platform_kick::spawn_kick_poller(
        channel,
        rewards,
        Arc::new(move || {
            let manager = Arc::clone(&manager_for_poller);
            Box::pin(async move { manager.get_valid_access_token().await })
        }),
        poller_tx,
    );

    let (viewer_poll, viewer_source) = forge_platform_kick::KickViewerPoll::new(slug.clone(), http);
    tokio::spawn(viewer_poll.run());

    let (bundle, _health_tx) =
        forge_platform_kick::KickIntegrationBundle::new(slug, Arc::clone(&platform), manager);

    spawn_chat_send_bridge(Arc::clone(bus), chat_platform, "kick", EventSource::Kick);

    let object = BuiltinObject {
        icon: SectionIcon::new("brand-kick"),
        status: bundle.clone(),
        health: bundle.clone(),
        content: bundle.clone(),
        quick: bundle.clone(),
        control: Some(bundle as Arc<dyn BuiltinControl>),
        obs_client: None,
    };
    (Some(object), Some(Box::new(viewer_source)))
}
