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
pub use pool::{SqlitePools, connect, connect_pools, connect_pools_with_busy_timeout};
pub use provider::SqliteBackend;
pub use repos::action::SqliteActionRepo;
pub use repos::ban_ledger::SqliteBanLedgerRepo;
pub use repos::chat_history::SqliteChatHistoryRepo;
pub use repos::credentials::SqliteCredentialsRepo;
pub(crate) use repos::donation::SqliteDonationRepo;
pub use repos::event_log::SqliteEventLogRepo;
pub use repos::globals::SqliteGlobalsRepo;
pub use repos::history::SqliteHistoryRepo;
pub(crate) use repos::latest_value::SqliteLatestValueRepo;
pub use repos::media::SqliteMediaRepo;
pub use repos::overlay::SqliteOverlayRepo;
pub use repos::queue::SqliteQueueRepo;
pub use repos::scheduled_run::SqliteScheduledRunRepo;
pub use repos::script::SqliteScriptRepo;
pub use repos::settings::SqliteSettingsRepo;
pub(crate) use repos::trigger_instance::SqliteTriggerInstanceRepo;
pub use repos::tts_filters::SqliteTtsFiltersRepo;
pub use repos::user_globals::SqliteUserGlobalsRepo;
pub use repos::viewer::SqliteViewerRepo;
pub use repos::voice_alias::SqliteVoiceAliasRepo;
pub use schema::migrations::{
    BASELINE_VERSION, JournalState, MIGRATIONS, apply as apply_migrations,
};
pub use schema::schema_probe::{
    SqliteDatabaseFileSchema, inspect_database_file, read_applied_schema_version,
};

pub use repos::soundboard::SqliteSoundboardClipsRepo;
pub use schema::migrations::classify as classify_journal;
pub use schema::registry_migration::migrate_registry_format;
