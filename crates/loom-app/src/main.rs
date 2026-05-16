use std::path::PathBuf;
use std::sync::Arc;

use loom_app::App;
use loom_app::Screen;
use loom_app::app::{subscription, theme_callback, update, view};
use loom_app::screen::OnboardingStep;
use loom_storage::SettingsRepo;
use loom_storage::reserved_keys;
use loom_storage_sqlite::SqliteBackend;

#[cfg(target_os = "linux")]
fn data_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            PathBuf::from(home).join(".local").join("share")
        });
    base.join("streamer-loom").join("streamer-loom.db")
}

#[cfg(target_os = "windows")]
fn data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("streamer-loom").join("streamer-loom.db")
}

#[cfg(target_os = "macos")]
fn data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_default();
    PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("streamer-loom")
        .join("streamer-loom.db")
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
    let result = rt.block_on(backend.get_string(reserved_keys::ONBOARDING_COMPLETED));
    match result {
        Ok(Some(ref v)) if v == "true" => Screen::Hub,
        _ => Screen::Onboarding(OnboardingStep::Welcome),
    }
}

fn main() -> iced::Result {
    tracing_subscriber::fmt().with_env_filter("info").init();
    tracing::info!("streamer-loom starting");

    let (backend, storage_offline) = boot_storage();
    let initial_screen = resolve_initial_screen(&backend);

    iced::application("streamer-loom", update, view)
        .subscription(subscription)
        .theme(theme_callback)
        .run_with(move || {
            let app = App::default_with(initial_screen, Arc::clone(&backend), storage_offline);
            (app, iced::Task::none())
        })
}
