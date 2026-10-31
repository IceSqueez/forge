use std::path::PathBuf;
use std::sync::Arc;

use forge_events::EventPublisher;
use forge_net::TaskOwner;
use forge_runtime::{
    EventBus, FirstChatLedger, LatestValues, NotificationPreferences,
    spawn_chat_history_persistence, spawn_event_log_bridge, spawn_latest_projector,
    spawn_notification_policy, spawn_viewer_tracker,
};
use forge_storage::{DataProvider, SettingsRepo};
use forge_transfer::{
    ScheduledBackupParts, SystemBackupClock, TransferExclusivity, spawn_scheduled_backups,
};
use forge_types::Shared;

use crate::host_handles::CoreHandles;
use crate::safety_copy_prune::spawn_boot_prune;

pub(crate) async fn build_core(
    backend: &Arc<dyn DataProvider>,
    data_dir: PathBuf,
    anonymous_donor: Shared<String>,
) -> CoreHandles {
    let settings_repo: Arc<dyn SettingsRepo> = Arc::clone(backend) as Arc<dyn SettingsRepo>;
    let viewer_repo = backend.viewer_repo();
    let first_chats = match FirstChatLedger::load(viewer_repo.as_ref()).await {
        Ok(ledger) => Some(ledger),
        Err(e) => {
            tracing::warn!(error = %e, "viewer history unreadable; first-time chatters are not detected");
            None
        }
    };
    let bus = EventBus::with_first_chats(backend.event_log_repo(), first_chats);
    EventBus::spawn_flush_task(Arc::clone(&bus));
    let services = TaskOwner::new();
    spawn_event_log_bridge(Arc::clone(&bus), &services);
    let notification_policy = spawn_notification_policy(
        &bus,
        NotificationPreferences::load(settings_repo.as_ref()).await,
        &services,
    );
    let transfer_exclusivity = TransferExclusivity::new();
    spawn_boot_prune(
        Arc::clone(backend),
        data_dir.clone(),
        transfer_exclusivity.clone(),
    );
    let scheduled_backups = spawn_scheduled_backups(ScheduledBackupParts {
        provider: Arc::clone(backend),
        data_dir,
        exclusivity: transfer_exclusivity.clone(),
        clock: Arc::new(SystemBackupClock),
    });
    let chat_history_retention = spawn_chat_history_persistence(
        Arc::clone(&bus),
        backend.chat_history_repo(),
        settings_repo,
    );
    spawn_viewer_tracker(Arc::clone(&bus), viewer_repo);

    let latest_values = LatestValues::load(
        backend.latest_value_repo(),
        Arc::clone(&bus) as Arc<dyn EventPublisher>,
        anonymous_donor,
    )
    .await;
    spawn_latest_projector(&bus, latest_values.clone(), &services);

    CoreHandles {
        bus,
        services,
        latest_values,
        notification_policy,
        chat_history_retention,
        transfer_exclusivity,
        scheduled_backups,
    }
}
