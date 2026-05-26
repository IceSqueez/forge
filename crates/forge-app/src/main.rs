use std::path::PathBuf;
use std::sync::Arc;

use forge_app::App;
use forge_app::Screen;
use forge_app::app::{theme_callback, update};
use forge_app::boot::{load_obs_and_connect, load_twitch_credential};
use forge_app::speak_bridge::SpeakBridge;
use forge_app::subscriptions::subscription;
use forge_app::view_router::view;
use forge_audio::{CpalSink, DeviceId, NullSink};
use forge_platform_core::paths;
use forge_platform_twitch::{ChatSendBridge, ChatSendBridgeHandle};
use forge_runtime::{
    ActionEngineHandle, CommandParser, CommandParserHandle, EventBus, QueueScheduler,
    QueueSchedulerHandle, ScriptRegistry, spawn_action_engine,
};
use forge_soundboard::{BusAudioEventSink, CpalSinkFactory, SoundboardPlayer};
use forge_speak_queue::{QueueConfig, QueueDeps, SpeakQueueHandle};
use forge_storage::{CredentialsRepo, DataProvider};
use forge_storage_sqlite::SqliteBackend;
use forge_tts_core::{EngineId, TtsRegistry};
use forge_tts_piper::{PiperEngine, PiperEngineFactory};
use forge_voice::{AssignmentStrategy, IgnoreProfile, SynthesisDefaults, VoiceAliasResolver};

fn default_db_path() -> PathBuf {
    paths::data_dir().join("forge.db")
}

fn init_tracing() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let console_layer = fmt::layer().with_target(false);

    let log_dir = paths::data_dir().join("logs");
    let (file_layer, guard) = match std::fs::create_dir_all(&log_dir) {
        Ok(()) => {
            let appender = tracing_appender::rolling::daily(&log_dir, "forge.log");
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let layer = fmt::layer()
                .with_writer(writer)
                .with_ansi(false)
                .with_target(true);
            (Some(layer), Some(guard))
        }
        Err(_) => (None, None),
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    if guard.is_some() {
        tracing::info!(path = %log_dir.display(), "file logging enabled");
    } else {
        tracing::warn!("file logging disabled: could not create log directory");
    }
    guard
}

fn boot_storage() -> (Arc<SqliteBackend>, bool) {
    let db_path = default_db_path();

    if let Some(parent) = db_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        tracing::error!("failed to create data directory: {e}");
        return open_memory_backend();
    }

    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!("failed to create tokio runtime for storage init: {e}");
            return open_memory_backend();
        }
    };

    match rt.block_on(SqliteBackend::open(&url)) {
        Ok(backend) => (Arc::new(backend), false),
        Err(e) => {
            tracing::error!("failed to open database at {}: {e}", db_path.display());
            open_memory_backend()
        }
    }
}

#[allow(clippy::expect_used)]
fn open_memory_backend() -> (Arc<SqliteBackend>, bool) {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime required for in-memory storage");
    let backend = rt
        .block_on(SqliteBackend::open("sqlite::memory:"))
        .expect("in-memory SQLite must always open");
    (Arc::new(backend), true)
}

struct RuntimeHandles {
    registry: Arc<ScriptRegistry>,
    engine: ActionEngineHandle,
    scheduler: QueueSchedulerHandle,
    parser: CommandParserHandle,
    chat_send_bridge: ChatSendBridgeHandle,
    speak_queue: Arc<SpeakQueueHandle>,
}

fn find_piper_binary() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let bundled = dir.join("piper");
        if bundled.exists() {
            return Some(bundled);
        }
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join("piper"))
            .find(|p| p.exists())
    })
}

fn default_audio_device_id() -> Option<DeviceId> {
    forge_audio::list_output_devices().ok().and_then(|devices| {
        devices
            .iter()
            .find(|d| d.is_default)
            .map(|d| d.id.clone())
            .or_else(|| devices.first().map(|d| d.id.clone()))
    })
}

fn spawn_speak_queue(bus: Arc<EventBus>) -> Arc<SpeakQueueHandle> {
    let mut registry = TtsRegistry::new();
    if let Some(piper_binary) = find_piper_binary() {
        let voices_dir = PiperEngine::voices_dir(&paths::data_dir());
        if let Err(e) = std::fs::create_dir_all(&voices_dir) {
            tracing::warn!(
                path = %voices_dir.display(),
                error = %e,
                "failed to create piper voices dir"
            );
        }
        registry.register(
            EngineId("piper".into()),
            Arc::new(PiperEngineFactory {
                piper_binary: piper_binary.clone(),
                voices_dir,
                timeout: std::time::Duration::from_secs(30),
            }),
        );
        tracing::info!(binary = %piper_binary.display(), "registered Piper TTS engine");
    } else {
        tracing::warn!("Piper binary not found in <exe_dir>/piper or PATH; TTS disabled");
    }
    let registry = Arc::new(registry);

    let resolver = Arc::new(std::sync::RwLock::new(VoiceAliasResolver::new(
        vec![],
        AssignmentStrategy::DeterministicByName,
        IgnoreProfile {
            excluded_voice_ids: vec![],
            excluded_locales: vec![],
        },
        SynthesisDefaults::default(),
    )));
    let pipeline = Arc::new(forge_tts_pipeline::PipelineConfig::default());

    let audio_sink: Arc<dyn forge_audio::AudioSink> = match default_audio_device_id() {
        Some(device_id) => {
            let event_sink = Arc::new(forge_audio::NullAudioEventSink);
            tracing::info!(device = %device_id.0, "speak queue audio sink ready");
            Arc::new(CpalSink::new(device_id, None, None, event_sink))
        }
        None => {
            tracing::warn!("no audio output device found; speak queue using NullSink");
            Arc::new(NullSink)
        }
    };

    let deps = QueueDeps {
        registry,
        resolver,
        pipeline,
        audio_sink,
        event_bus: bus as Arc<dyn forge_events::EventPublisher>,
    };
    let config = QueueConfig::default();
    let (handle, _stream) = forge_speak_queue::spawn(config, deps);
    Arc::new(handle)
}

#[allow(clippy::expect_used)]
fn spawn_runtime(backend: Arc<SqliteBackend>, bus: Arc<EventBus>) -> Option<RuntimeHandles> {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("failed to create tokio runtime for runtime spawn: {e}");
            return None;
        }
    };

    let dp: Arc<dyn DataProvider> = Arc::clone(&backend) as Arc<dyn DataProvider>;

    let queues = match rt.block_on(dp.queue_repo().list()) {
        Ok(q) => q,
        Err(e) => {
            tracing::warn!("failed to load queues on boot, starting with empty set: {e}");
            vec![]
        }
    };

    let speak_queue = spawn_speak_queue(Arc::clone(&bus));
    let _viewer_tracker =
        forge_app::viewer_tracker::spawn(Arc::clone(&bus), backend.viewer_repo_arc());
    let speak_bridge_concrete = Arc::new(SpeakBridge::new(Arc::clone(&speak_queue)));
    let speak_dispatcher: Arc<dyn forge_runtime::SpeakDispatcher> = speak_bridge_concrete.clone();
    let speak_requester: Arc<dyn forge_script::SpeakRequester> = speak_bridge_concrete;

    let mut registry_mut = ScriptRegistry::new();
    registry_mut.set_speak_requester(speak_requester);
    let registry = Arc::new(registry_mut);
    if let Err(e) = rt.block_on(registry.load_all(dp.as_ref())) {
        tracing::warn!("script registry load failed at boot: {e}");
    }

    let engine = spawn_action_engine(
        Arc::clone(&bus),
        Arc::clone(&dp),
        Arc::clone(&registry),
        None,
        None,
        Some(speak_dispatcher),
    );
    let scheduler = QueueScheduler::spawn(engine.clone(), Arc::clone(&bus), queues);
    let parser = CommandParser::spawn(Arc::clone(&bus), Arc::clone(&dp), scheduler.clone());
    let chat_send_bridge = ChatSendBridge::spawn(
        Arc::clone(&bus),
        Arc::clone(&backend) as Arc<dyn CredentialsRepo>,
    );

    Some(RuntimeHandles {
        registry,
        engine,
        scheduler,
        parser,
        chat_send_bridge,
        speak_queue,
    })
}

fn main() -> iced::Result {
    let _log_guard = init_tracing();
    tracing::info!("forge starting");

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!("failed to create tokio runtime: {e}");
            return Ok(());
        }
    };
    let _runtime_guard = runtime.enter();

    let (backend, storage_offline) = boot_storage();
    let initial_screen = Screen::Home;

    let event_log = backend.event_log_repo_arc();
    let bus = EventBus::new(event_log);
    EventBus::spawn_flush_task(Arc::clone(&bus));

    let (script_registry, action_engine, scheduler, command_parser, chat_send_bridge, speak_queue) =
        if storage_offline {
            (
                Arc::new(ScriptRegistry::new()),
                None,
                None,
                None,
                None,
                None,
            )
        } else {
            match spawn_runtime(Arc::clone(&backend), Arc::clone(&bus)) {
                Some(h) => (
                    h.registry,
                    Some(h.engine),
                    Some(h.scheduler),
                    Some(h.parser),
                    Some(h.chat_send_bridge),
                    Some(h.speak_queue),
                ),
                None => (
                    Arc::new(ScriptRegistry::new()),
                    None,
                    None,
                    None,
                    None,
                    None,
                ),
            }
        };

    let sound_player: Option<Arc<SoundboardPlayer>> = if storage_offline {
        None
    } else {
        let clips_repo = backend.soundboard_clips_repo_arc();
        Some(Arc::new(SoundboardPlayer::new(
            Arc::new(CpalSinkFactory),
            Arc::new(BusAudioEventSink::new(Arc::clone(&bus))),
            clips_repo,
        )))
    };

    let backend_boot: Arc<dyn DataProvider> = backend.clone();
    let boot_screen = Arc::new(initial_screen);
    let bus_boot = Arc::clone(&bus);
    let boot = move || {
        let mut app = App::default_with(
            (*boot_screen).clone(),
            Arc::clone(&backend_boot),
            storage_offline,
            Arc::clone(&script_registry),
            action_engine.clone(),
            scheduler.clone(),
            command_parser.clone(),
            sound_player.clone(),
        );
        app.rt.bus = Arc::clone(&bus_boot);
        app.rt.chat_send_bridge = chat_send_bridge.clone();
        app.rt.speak_queue = speak_queue.clone();
        let obs_creds: Arc<dyn forge_storage::CredentialsRepo> =
            Arc::clone(&backend_boot) as Arc<dyn forge_storage::CredentialsRepo>;
        let obs_task = iced::Task::perform(
            load_obs_and_connect(obs_creds, Arc::clone(&bus_boot)),
            forge_app::Message::ObsBootResult,
        );
        let twitch_creds: Arc<dyn forge_storage::CredentialsRepo> =
            Arc::clone(&backend_boot) as Arc<dyn forge_storage::CredentialsRepo>;
        let twitch_task = iced::Task::perform(
            load_twitch_credential(twitch_creds),
            forge_app::Message::TwitchBootResult,
        );
        let boot_task = match app.rt.action_engine.clone() {
            Some(engine) => {
                let dp = Arc::clone(&backend_boot);
                let server_boot_task = iced::Task::perform(
                    forge_app::server_subsystem::load_server_settings_and_start(
                        dp,
                        Arc::clone(&bus_boot),
                        std::sync::Arc::new(engine),
                        Arc::clone(&app.rt.server_subsystem),
                    ),
                    forge_app::Message::ServerBootResult,
                );
                iced::Task::batch([obs_task, twitch_task, server_boot_task])
            }
            None => iced::Task::batch([obs_task, twitch_task]),
        };
        (app, boot_task)
    };

    let mut app = iced::application(boot, update, view)
        .title("forge")
        .subscription(subscription)
        .theme(theme_callback);
    for font_bytes in forge_widgets::load_fonts() {
        app = app.font(font_bytes);
    }
    app.run()
}
