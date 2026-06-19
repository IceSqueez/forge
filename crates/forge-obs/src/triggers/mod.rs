mod audio_source_balance_changed;
mod audio_source_mute_changed;
mod audio_source_sync_offset_changed;
mod audio_source_volume_changed;
mod collection_list_changed;
mod filter_created;
mod filter_enabled_changed;
mod filter_removed;
mod profile_current_changed;
mod profile_list_changed;
mod record_file_changed;
mod record_paused;
mod record_resumed;
mod record_started;
mod record_starting;
mod record_status_changed;
mod record_stopped;
mod record_stopping;
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
mod source_scene_item_lock_changed;
mod source_scene_item_visibility_changed;
mod stream_started;
mod stream_starting;
mod stream_status_changed;
mod stream_stopped;
mod stream_stopping;
mod studio_disabled;
mod studio_enabled;
mod transition_ended;
mod transition_started;
mod transition_video_ended;

pub use audio_source_balance_changed::AudioSourceBalanceChangedDescriptor;
pub use audio_source_mute_changed::AudioSourceMuteChangedDescriptor;
pub use audio_source_sync_offset_changed::AudioSourceSyncOffsetChangedDescriptor;
pub use audio_source_volume_changed::AudioSourceVolumeChangedDescriptor;
pub use collection_list_changed::CollectionListChangedDescriptor;
pub use filter_created::FilterCreatedDescriptor;
pub use filter_enabled_changed::FilterEnabledChangedDescriptor;
pub use filter_removed::FilterRemovedDescriptor;
pub use profile_current_changed::ProfileCurrentChangedDescriptor;
pub use profile_list_changed::ProfileListChangedDescriptor;
pub use record_file_changed::RecordFileChangedDescriptor;
pub use record_paused::RecordPausedDescriptor;
pub use record_resumed::RecordResumedDescriptor;
pub use record_started::RecordStartedDescriptor;
pub use record_starting::RecordStartingDescriptor;
pub use record_status_changed::RecordStatusChangedDescriptor;
pub use record_stopped::RecordStoppedDescriptor;
pub use record_stopping::RecordStoppingDescriptor;
pub use scene_collection_changed::SceneCollectionChangedDescriptor;
pub use scene_collection_changing::SceneCollectionChangingDescriptor;
pub use scene_created::SceneCreatedDescriptor;
pub use scene_current_changed::SceneCurrentChangedDescriptor;
pub use scene_list_changed::SceneListChangedDescriptor;
pub use scene_preview_changed::ScenePreviewChangedDescriptor;
pub use scene_removed::SceneRemovedDescriptor;
pub use scene_renamed::SceneRenamedDescriptor;
pub use source_input_created::SourceInputCreatedDescriptor;
pub use source_input_removed::SourceInputRemovedDescriptor;
pub use source_input_renamed::SourceInputRenamedDescriptor;
pub use source_scene_item_lock_changed::SourceSceneItemLockChangedDescriptor;
pub use source_scene_item_visibility_changed::SourceSceneItemVisibilityChangedDescriptor;
pub use stream_started::StreamStartedDescriptor;
pub use stream_starting::StreamStartingDescriptor;
pub use stream_status_changed::StreamStatusChangedDescriptor;
pub use stream_stopped::StreamStoppedDescriptor;
pub use stream_stopping::StreamStoppingDescriptor;
pub use studio_disabled::StudioDisabledDescriptor;
pub use studio_enabled::StudioEnabledDescriptor;
pub use transition_ended::TransitionEndedDescriptor;
pub use transition_started::TransitionStartedDescriptor;
pub use transition_video_ended::TransitionVideoEndedDescriptor;

use forge_registry::{RegistryError, TriggerRegistry};

pub fn register_obs_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    reg.register(Box::new(SceneCurrentChangedDescriptor))?;
    reg.register(Box::new(SceneCreatedDescriptor))?;
    reg.register(Box::new(SceneRemovedDescriptor))?;
    reg.register(Box::new(SceneRenamedDescriptor))?;
    reg.register(Box::new(AudioSourceMuteChangedDescriptor))?;
    reg.register(Box::new(AudioSourceVolumeChangedDescriptor))?;
    reg.register(Box::new(AudioSourceBalanceChangedDescriptor))?;
    reg.register(Box::new(AudioSourceSyncOffsetChangedDescriptor))?;
    reg.register(Box::new(ScenePreviewChangedDescriptor))?;
    reg.register(Box::new(SceneListChangedDescriptor))?;
    reg.register(Box::new(SceneCollectionChangingDescriptor))?;
    reg.register(Box::new(SceneCollectionChangedDescriptor))?;
    reg.register(Box::new(CollectionListChangedDescriptor))?;
    reg.register(Box::new(ProfileCurrentChangedDescriptor))?;
    reg.register(Box::new(ProfileListChangedDescriptor))?;
    reg.register(Box::new(StreamStartingDescriptor))?;
    reg.register(Box::new(StreamStartedDescriptor))?;
    reg.register(Box::new(StreamStoppingDescriptor))?;
    reg.register(Box::new(StreamStoppedDescriptor))?;
    reg.register(Box::new(StreamStatusChangedDescriptor))?;
    reg.register(Box::new(RecordStartingDescriptor))?;
    reg.register(Box::new(RecordStartedDescriptor))?;
    reg.register(Box::new(RecordStoppingDescriptor))?;
    reg.register(Box::new(RecordStoppedDescriptor))?;
    reg.register(Box::new(RecordPausedDescriptor))?;
    reg.register(Box::new(RecordResumedDescriptor))?;
    reg.register(Box::new(RecordFileChangedDescriptor))?;
    reg.register(Box::new(RecordStatusChangedDescriptor))?;
    reg.register(Box::new(SourceInputCreatedDescriptor))?;
    reg.register(Box::new(SourceInputRemovedDescriptor))?;
    reg.register(Box::new(SourceInputRenamedDescriptor))?;
    reg.register(Box::new(SourceSceneItemLockChangedDescriptor))?;
    reg.register(Box::new(SourceSceneItemVisibilityChangedDescriptor))?;
    reg.register(Box::new(StudioEnabledDescriptor))?;
    reg.register(Box::new(StudioDisabledDescriptor))?;
    reg.register(Box::new(TransitionStartedDescriptor))?;
    reg.register(Box::new(TransitionEndedDescriptor))?;
    reg.register(Box::new(TransitionVideoEndedDescriptor))?;
    reg.register(Box::new(FilterCreatedDescriptor))?;
    reg.register(Box::new(FilterRemovedDescriptor))?;
    reg.register(Box::new(FilterEnabledChangedDescriptor))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
            "obs.scenes.current_changed",
            "obs.scenes.preview_changed",
            "obs.scenes.list_changed",
            "obs.collection.changing",
            "obs.collection.current_changed",
            "obs.stream.starting",
            "obs.stream.started",
            "obs.stream.stopping",
            "obs.stream.stopped",
            "obs.stream.status_changed",
            "obs.record.starting",
            "obs.record.started",
            "obs.record.stopping",
            "obs.record.stopped",
            "obs.record.paused",
            "obs.record.resumed",
            "obs.record.file_changed",
            "obs.record.status_changed",
            "obs.sources.input_created",
            "obs.sources.input_removed",
            "obs.sources.input_renamed",
            "obs.sources.scene_item_lock_changed",
            "obs.sources.scene_item_visibility_changed",
            "obs.studio.enabled",
            "obs.studio.disabled",
            "obs.transition.started",
            "obs.transition.ended",
            "obs.transition.video_ended",
            "obs.filters.created",
            "obs.filters.removed",
            "obs.filters.enabled_changed",
        ] {
            assert!(reg.get(id).is_some(), "missing trigger: {id}");
        }
    }

    #[test]
    fn duplicate_registration_returns_error() {
        let mut reg = TriggerRegistry::new();
        register_obs_triggers(&mut reg).unwrap();
        let result = register_obs_triggers(&mut reg);
        assert!(result.is_err());
    }
}
