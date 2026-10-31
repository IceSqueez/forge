use std::sync::Arc;

use forge_storage::settings::core_settings::{
    BACKUP_SCHEDULE_ENABLED, BACKUP_SCHEDULE_KEY, BACKUP_SCHEDULE_LAST_SUCCESS_AT,
};
use forge_storage::test_support::MemorySettings;
use time::OffsetDateTime;

use super::TestBackend;

pub struct GatedSchedule {
    pub handle: forge_transfer::ScheduledBackupHandle,
    pub export_gate: Arc<tokio::sync::Notify>,
    _data_dir: tempfile::TempDir,
}

pub fn gated_running_schedule() -> GatedSchedule {
    let key = forge_transfer::create_schedule_key(&forge_transfer::Passphrase::new(
        "a gated schedule passphrase".to_owned(),
    ))
    .unwrap();
    let schedule = enabled_schedule_over_test_backend(
        Some(key.encode().unwrap()),
        forge_transfer::TransferExclusivity::new(),
    );
    schedule.handle.run_now();
    schedule
}

pub fn gated_running_schedule_sharing(
    exclusivity: forge_transfer::TransferExclusivity,
) -> GatedSchedule {
    let key = forge_transfer::create_schedule_key(&forge_transfer::Passphrase::new(
        "a gated schedule passphrase".to_owned(),
    ))
    .unwrap();
    let schedule = enabled_schedule_over_test_backend(Some(key.encode().unwrap()), exclusivity);
    schedule.handle.run_now();
    schedule
}

pub fn keyless_enabled_schedule() -> GatedSchedule {
    enabled_schedule_over_test_backend(None, forge_transfer::TransferExclusivity::new())
}

fn enabled_schedule_over_test_backend(
    encoded_key: Option<String>,
    exclusivity: forge_transfer::TransferExclusivity,
) -> GatedSchedule {
    let export_gate = Arc::new(tokio::sync::Notify::new());
    let last_success = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let settings = MemorySettings::new();
    settings.seed(&BACKUP_SCHEDULE_ENABLED, &true).unwrap();
    settings
        .seed(&BACKUP_SCHEDULE_LAST_SUCCESS_AT, &Some(last_success))
        .unwrap();
    if let Some(encoded) = encoded_key {
        settings.seed(&BACKUP_SCHEDULE_KEY, &Some(encoded)).unwrap();
    }
    let backend = TestBackend::with_gated_export(settings, Arc::clone(&export_gate));
    let data_dir = tempfile::tempdir().unwrap();
    let parts = forge_transfer::ScheduledBackupParts {
        provider: backend,
        data_dir: data_dir.path().to_path_buf(),
        exclusivity,
        clock: Arc::new(forge_transfer::SystemBackupClock),
    };
    let handle = forge_transfer::spawn_scheduled_backups(parts);
    GatedSchedule {
        handle,
        export_gate,
        _data_dir: data_dir,
    }
}
