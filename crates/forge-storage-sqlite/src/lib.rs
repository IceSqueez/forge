#![cfg_attr(
    not(any(test, feature = "test-support")),
    expect(
        unreachable_pub,
        reason = "these modules are public only behind test-support"
    )
)]

mod batch;
mod codec;
mod credential_reseal_v063;
mod error;
#[cfg(feature = "settings-test-support")]
mod foreign_settings;
mod pool;
mod provider;
mod repos;
mod schema;
mod snapshot;
mod tasks;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
mod unit_of_work;

pub use error::SqliteStorageError;
pub use provider::SqliteBackend;
pub use schema::schema_probe::SqliteDatabaseFileSchema;

#[cfg(feature = "test-support")]
pub use pool::{SqlitePools, connect, connect_pools, connect_pools_with_busy_timeout};
#[cfg(feature = "test-support")]
pub use repos::action::SqliteActionRepo;
#[cfg(feature = "test-support")]
pub use repos::ban_ledger::SqliteBanLedgerRepo;
#[cfg(feature = "test-support")]
pub use repos::chat_history::SqliteChatHistoryRepo;
#[cfg(feature = "test-support")]
pub use repos::credentials::SqliteCredentialsRepo;
#[cfg(feature = "test-support")]
pub use repos::event_log::SqliteEventLogRepo;
#[cfg(feature = "test-support")]
pub use repos::globals::SqliteGlobalsRepo;
#[cfg(feature = "test-support")]
pub use repos::history::SqliteHistoryRepo;
#[cfg(feature = "test-support")]
pub use repos::media::SqliteMediaRepo;
#[cfg(feature = "test-support")]
pub use repos::overlay::SqliteOverlayRepo;
#[cfg(feature = "test-support")]
pub use repos::queue::SqliteQueueRepo;
#[cfg(feature = "test-support")]
pub use repos::scheduled_run::SqliteScheduledRunRepo;
#[cfg(feature = "test-support")]
pub use repos::script::SqliteScriptRepo;
#[cfg(feature = "test-support")]
pub use repos::settings::SqliteSettingsRepo;
#[cfg(feature = "test-support")]
pub use repos::soundboard::SqliteSoundboardClipsRepo;
#[cfg(feature = "test-support")]
pub use repos::tts_filters::SqliteTtsFiltersRepo;
#[cfg(feature = "test-support")]
pub use repos::user_globals::SqliteUserGlobalsRepo;
#[cfg(feature = "test-support")]
pub use repos::viewer::SqliteViewerRepo;
#[cfg(feature = "test-support")]
pub use repos::voice_alias::SqliteVoiceAliasRepo;
#[cfg(feature = "test-support")]
pub use schema::migrations::classify as classify_journal;
#[cfg(feature = "test-support")]
pub use schema::migrations::{
    BASELINE_VERSION, JournalState, MIGRATIONS, apply as apply_migrations,
};
#[cfg(feature = "test-support")]
pub use schema::registry_migration::migrate_registry_format;
#[cfg(feature = "test-support")]
pub use schema::schema_probe::{inspect_database_file, read_applied_schema_version};
