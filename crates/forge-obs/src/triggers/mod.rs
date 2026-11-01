mod audio_source;
mod audio_source_balance_changed;
mod audio_source_mute_changed;
mod audio_source_sync_offset_changed;
mod audio_source_volume_changed;
mod collection_list_changed;
mod connection_auth_failed;
mod connection_connected;
mod connection_disconnected;
mod families;
mod filter_created;
mod filter_enabled_changed;
mod filter_removed;
mod lifecycle;
mod profile_current_changed;
mod profile_list_changed;
mod record_file_changed;
mod record_starting;
mod record_status_changed;
mod scene_collection_changed;
mod scene_collection_changing;
mod scene_created;
mod scene_current_changed;
mod scene_list_changed;
mod scene_preview_changed;
mod scene_removed;
mod scene_renamed;
mod source_input_created;
mod source_input_removed;
mod source_input_renamed;
mod source_scene_item_created;
mod source_scene_item_lock_changed;
mod source_scene_item_removed;
mod source_scene_item_visibility_changed;
mod stream_starting;
mod stream_status_changed;
mod studio_enabled;
mod transition_started;
mod virtualcam_started;
mod virtualcam_status_changed;
mod virtualcam_stopped;

use forge_registry::EventTriggerSpec;
use forge_registry::{RegistryError, TriggerRegistry};

const TRIGGERS: &[&EventTriggerSpec] = &[
    &connection_auth_failed::CONNECTION_AUTH_FAILED,
    &connection_connected::CONNECTION_CONNECTED,
    &connection_disconnected::CONNECTION_DISCONNECTED,
    &scene_current_changed::SCENE_CURRENT_CHANGED,
    &scene_created::SCENE_CREATED,
    &scene_removed::SCENE_REMOVED,
    &scene_renamed::SCENE_RENAMED,
    &audio_source_mute_changed::AUDIO_SOURCE_MUTE_CHANGED,
    &audio_source_volume_changed::AUDIO_SOURCE_VOLUME_CHANGED,
    &audio_source_balance_changed::AUDIO_SOURCE_BALANCE_CHANGED,
    &audio_source_sync_offset_changed::AUDIO_SOURCE_SYNC_OFFSET_CHANGED,
    &scene_preview_changed::SCENE_PREVIEW_CHANGED,
    &scene_list_changed::SCENE_LIST_CHANGED,
    &scene_collection_changing::SCENE_COLLECTION_CHANGING,
    &scene_collection_changed::SCENE_COLLECTION_CHANGED,
    &collection_list_changed::COLLECTION_LIST_CHANGED,
    &profile_current_changed::PROFILE_CURRENT_CHANGED,
    &profile_list_changed::PROFILE_LIST_CHANGED,
    &stream_starting::STREAM_STARTING,
    &lifecycle::STREAMING_STARTED,
    &lifecycle::STREAMING_STOPPING,
    &lifecycle::STREAMING_STOPPED,
    &stream_status_changed::STREAM_STATUS_CHANGED,
    &record_starting::RECORD_STARTING,
    &lifecycle::RECORDING_STARTED,
    &lifecycle::RECORDING_STOPPING,
    &lifecycle::RECORDING_STOPPED,
    &lifecycle::RECORDING_PAUSED,
    &lifecycle::RECORDING_RESUMED,
    &record_file_changed::RECORD_FILE_CHANGED,
    &record_status_changed::RECORD_STATUS_CHANGED,
    &source_input_created::SOURCE_INPUT_CREATED,
    &source_input_removed::SOURCE_INPUT_REMOVED,
    &source_input_renamed::SOURCE_INPUT_RENAMED,
    &source_scene_item_created::SOURCE_SCENE_ITEM_CREATED,
    &source_scene_item_removed::SOURCE_SCENE_ITEM_REMOVED,
    &source_scene_item_lock_changed::SOURCE_SCENE_ITEM_LOCK_CHANGED,
    &source_scene_item_visibility_changed::SOURCE_SCENE_ITEM_VISIBILITY_CHANGED,
    &studio_enabled::STUDIO_ENABLED,
    &lifecycle::STUDIO_DISABLED,
    &transition_started::TRANSITION_STARTED,
    &lifecycle::TRANSITION_ENDED,
    &lifecycle::TRANSITION_VIDEO_ENDED,
    &filter_created::FILTER_CREATED,
    &filter_removed::FILTER_REMOVED,
    &filter_enabled_changed::FILTER_ENABLED_CHANGED,
    &virtualcam_started::VIRTUALCAM_STARTED,
    &virtualcam_stopped::VIRTUALCAM_STOPPED,
    &virtualcam_status_changed::VIRTUALCAM_STATUS_CHANGED,
];

pub fn register_obs_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    let mut reg = reg.owned_by(crate::OBS_INTEGRATION.id);
    for spec in TRIGGERS {
        reg.register(Box::new((*spec).clone()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_expected_trigger_ids_are_registered() {
        let mut reg = TriggerRegistry::new();
        register_obs_triggers(&mut reg).unwrap();
        for id in [
            "obs.audio.source_mute_changed",
            "obs.audio.source_volume_changed",
            "obs.audio.source_balance_changed",
            "obs.audio.source_sync_offset_changed",
            "obs.scene.changed",
            "obs.scene.created",
            "obs.scene.removed",
            "obs.scene.renamed",
            "obs.scene.preview_changed",
            "obs.scene.list_changed",
            "obs.collection.changing",
            "obs.collection.changed",
            "obs.collection.list_changed",
            "obs.profile.current_changed",
            "obs.profile.list_changed",
            "obs.streaming.starting",
            "obs.streaming.started",
            "obs.streaming.stopping",
            "obs.streaming.stopped",
            "obs.streaming.status_changed",
            "obs.recording.starting",
            "obs.recording.started",
            "obs.recording.stopping",
            "obs.recording.stopped",
            "obs.recording.paused",
            "obs.recording.resumed",
            "obs.recording.file_changed",
            "obs.recording.status_changed",
            "obs.source.input_created",
            "obs.source.input_removed",
            "obs.source.input_renamed",
            "obs.source.scene_item_created",
            "obs.source.scene_item_removed",
            "obs.source.lock_changed",
            "obs.source.visibility_changed",
            "obs.studio.enabled",
            "obs.studio.disabled",
            "obs.transition.started",
            "obs.transition.ended",
            "obs.transition.video_ended",
            "obs.filter.created",
            "obs.filter.removed",
            "obs.filter.enabled_changed",
            "obs.virtualcam.started",
            "obs.virtualcam.stopped",
            "obs.virtualcam.status_changed",
            "obs.connection.connected",
            "obs.connection.disconnected",
            "obs.connection.auth_failed",
        ] {
            assert!(reg.get(id).is_some(), "missing trigger: {id}");
        }
    }
}
