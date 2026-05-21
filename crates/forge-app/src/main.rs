use std::path::PathBuf;
use std::sync::Arc;

use forge_app::App;
use forge_app::Screen;
use forge_app::app::{
    load_obs_and_connect, load_twitch_credential, subscription, theme_callback, update, view,
};
use forge_platform_core::paths;
use forge_platform_twitch::{ChatSendBridge, ChatSendBridgeHandle};
use forge_runtime::{
    ActionEngineHandle, CommandParser, CommandParserHandle, EventBus, QueueScheduler,
    QueueSchedulerHandle, ScriptRegistry, spawn_action_engine,
};
use forge_soundboard::{BusAudioEventSink, CpalSinkFactory, SoundboardPlayer};
use forge_storage::{CredentialsRepo, DataProvider};
use forge_storage_sqlite::SqliteBackend;

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

    let registry = Arc::new(ScriptRegistry::new());
    if let Err(e) = rt.block_on(registry.load_all(dp.as_ref())) {
        tracing::warn!("script registry load failed at boot: {e}");
    }

    let engine = spawn_action_engine(
        Arc::clone(&bus),
        Arc::clone(&dp),
        Arc::clone(&registry),
        None,
        None,
        None,
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

    let (script_registry, action_engine, scheduler, command_parser, chat_send_bridge) =
        if storage_offline {
            (Arc::new(ScriptRegistry::new()), None, None, None, None)
        } else {
            match spawn_runtime(Arc::clone(&backend), Arc::clone(&bus)) {
                Some(h) => (
                    h.registry,
                    Some(h.engine),
                    Some(h.scheduler),
                    Some(h.parser),
                    Some(h.chat_send_bridge),
                ),
                None => (Arc::new(ScriptRegistry::new()), None, None, None, None),
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

    let backend_boot = Arc::clone(&backend);
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
        app.bus = Arc::clone(&bus_boot);
        app.chat_send_bridge = chat_send_bridge.clone();
        let obs_task = iced::Task::perform(
            load_obs_and_connect(Arc::clone(&backend_boot), Arc::clone(&bus_boot)),
            forge_app::Message::ObsBootResult,
        );
        let twitch_task = iced::Task::perform(
            load_twitch_credential(Arc::clone(&backend_boot)),
            forge_app::Message::TwitchBootResult,
        );
        let boot_task = match app.action_engine.clone() {
            Some(engine) => {
                let dp: std::sync::Arc<dyn forge_storage::DataProvider> =
                    Arc::clone(&backend_boot) as std::sync::Arc<dyn forge_storage::DataProvider>;
                let server_boot_task = iced::Task::perform(
                    forge_app::server_subsystem::load_server_settings_and_start(
                        dp,
                        Arc::clone(&bus_boot),
                        std::sync::Arc::new(engine),
                        Arc::clone(&app.server_subsystem),
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
        .theme(theme_callback)
        .font(forge_widgets::BOOTSTRAP_FONT_BYTES);
    for font_bytes in forge_widgets::load_fonts() {
        app = app.font(font_bytes);
    }
    app.run()
}
