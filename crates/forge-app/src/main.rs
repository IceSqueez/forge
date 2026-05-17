use std::path::PathBuf;
use std::sync::Arc;

use forge_app::App;
use forge_app::Screen;
use forge_app::app::{subscription, theme_callback, update, view};
use forge_app::screen::OnboardingStep;
use forge_storage::SettingsRepo;
use forge_storage::reserved_keys;
use forge_storage_sqlite::SqliteBackend;

#[cfg(target_os = "linux")]
fn data_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            PathBuf::from(home).join(".local").join("share")
        });
    base.join("forge").join("forge.db")
}

#[cfg(target_os = "windows")]
fn data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("forge").join("forge.db")
}

#[cfg(target_os = "macos")]
fn data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_default();
    PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("forge")
        .join("forge.db")
}

fn boot_storage() -> (Arc<SqliteBackend>, bool) {
    let db_path = data_dir();

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
            return Screen::Hub;
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

fn main() -> iced::Result {
    tracing_subscriber::fmt().with_env_filter("info").init();
    tracing::info!("forge starting");

    let (backend, storage_offline) = boot_storage();
    let initial_screen = resolve_initial_screen(&backend);

    let backend_boot = Arc::clone(&backend);
    let boot_screen = Arc::new(initial_screen);
    let boot = move || {
        let app = App::default_with(
            (*boot_screen).clone(),
            Arc::clone(&backend_boot),
            storage_offline,
        );
        (app, iced::Task::none())
    };

    iced::application(boot, update, view)
        .title("forge")
        .subscription(subscription)
        .theme(theme_callback)
        .run()
}
