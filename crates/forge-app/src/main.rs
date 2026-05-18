use std::path::PathBuf;
use std::sync::Arc;

use forge_app::App;
use forge_app::Screen;
use forge_app::app::{subscription, theme_callback, update, view};
use forge_app::screen::OnboardingStep;
use forge_platform_core::paths;
use forge_platform_twitch::{ChatSendBridge, ChatSendBridgeHandle};
use forge_runtime::{
    ActionEngineHandle, CommandParser, CommandParserHandle, EventBus, QueueScheduler,
    QueueSchedulerHandle, spawn_action_engine,
};
use forge_storage::{CredentialsRepo, DataProvider, SettingsRepo, reserved_keys};
use forge_storage_sqlite::SqliteBackend;

fn default_db_path() -> PathBuf {
    paths::data_dir().join("forge.db")
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

#[allow(clippy::expect_used)]
fn resolve_initial_screen(backend: &SqliteBackend) -> Screen {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime required for settings read");
    rt.block_on(async {
        let completed = backend
            .get_string(reserved_keys::ONBOARDING_COMPLETED)
            .await
            .ok()
            .flatten();
        if matches!(completed.as_deref(), Some("true")) {
            return Screen::Home;
        }
        let last_step = backend
            .get_string(reserved_keys::LAST_ONBOARDING_STEP)
            .await
            .ok()
            .flatten();
        let step = last_step
            .as_deref()
            .and_then(OnboardingStep::from_key)
            .unwrap_or(OnboardingStep::Welcome);
        Screen::Onboarding(step)
    })
}

struct RuntimeHandles {
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

    let engine = spawn_action_engine(Arc::clone(&bus), Arc::clone(&dp));
    let scheduler = QueueScheduler::spawn(engine.clone(), Arc::clone(&bus), queues);
    let parser = CommandParser::spawn(Arc::clone(&bus), Arc::clone(&dp), scheduler.clone());
    let chat_send_bridge = ChatSendBridge::spawn(
        Arc::clone(&bus),
        Arc::clone(&backend) as Arc<dyn CredentialsRepo>,
    );

    Some(RuntimeHandles {
        engine,
        scheduler,
        parser,
        chat_send_bridge,
    })
}

fn main() -> iced::Result {
    tracing_subscriber::fmt().with_env_filter("info").init();
    tracing::info!("forge starting");

    let (backend, storage_offline) = boot_storage();
    let initial_screen = resolve_initial_screen(&backend);

    let bus = Arc::new(EventBus::new());

    let (action_engine, scheduler, command_parser, chat_send_bridge) = if storage_offline {
        (None, None, None, None)
    } else {
        match spawn_runtime(Arc::clone(&backend), Arc::clone(&bus)) {
            Some(h) => (
                Some(h.engine),
                Some(h.scheduler),
                Some(h.parser),
                Some(h.chat_send_bridge),
            ),
            None => (None, None, None, None),
        }
    };

    let backend_boot = Arc::clone(&backend);
    let boot_screen = Arc::new(initial_screen);
    let bus_boot = Arc::clone(&bus);
    let boot = move || {
        let mut app = App::default_with(
            (*boot_screen).clone(),
            Arc::clone(&backend_boot),
            storage_offline,
            action_engine.clone(),
            scheduler.clone(),
            command_parser.clone(),
        );
        app.bus = Arc::clone(&bus_boot);
        app.chat_send_bridge = chat_send_bridge.clone();
        (app, iced::Task::none())
    };

    iced::application(boot, update, view)
        .title("forge")
        .subscription(subscription)
        .theme(theme_callback)
        .font(forge_widgets::BOOTSTRAP_FONT_BYTES)
        .run()
}
