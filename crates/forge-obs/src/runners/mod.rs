mod query_input_list;
mod query_input_settings;
mod query_record_status;
mod query_scene_list;
mod query_stream_status;
mod raw_request;
mod record_pause;
mod record_resume;
mod record_start;
mod record_stop;
mod record_toggle_pause;
mod replay_save;
mod replay_start;
mod replay_stop;
mod set_input_settings;
mod set_mute;
mod set_preview_scene;
mod set_transition;
mod set_visible;
mod set_volume;
mod stream_send_caption;
mod stream_start;
mod stream_stop;
mod studio_disable;
mod studio_enable;
mod studio_trigger_transition;
mod switch_current;

#[cfg(test)]
pub(crate) mod test_support;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};

pub use query_input_list::QueryInputListRunner;
pub use query_input_settings::QueryInputSettingsRunner;
pub use query_record_status::QueryRecordStatusRunner;
pub use query_scene_list::QuerySceneListRunner;
pub use query_stream_status::QueryStreamStatusRunner;
pub use raw_request::RawRequestRunner;
pub use record_pause::RecordPauseRunner;
pub use record_resume::RecordResumeRunner;
pub use record_start::RecordStartRunner;
pub use record_stop::RecordStopRunner;
pub use record_toggle_pause::RecordTogglePauseRunner;
pub use replay_save::ReplaySaveRunner;
pub use replay_start::ReplayStartRunner;
pub use replay_stop::ReplayStopRunner;
pub use set_input_settings::SetInputSettingsRunner;
pub use set_mute::SetMuteRunner;
pub use set_preview_scene::SetPreviewSceneRunner;
pub use set_transition::SetTransitionRunner;
pub use set_visible::SetVisibleRunner;
pub use set_volume::SetVolumeRunner;
pub use stream_send_caption::StreamSendCaptionRunner;
pub use stream_start::StreamStartRunner;
pub use stream_stop::StreamStopRunner;
pub use studio_disable::StudioDisableRunner;
pub use studio_enable::StudioEnableRunner;
pub use studio_trigger_transition::StudioTriggerTransitionRunner;
pub use switch_current::SwitchCurrentSceneRunner;

use crate::ObsSink;

pub fn register_obs_sub_actions(
    reg: &mut SubActionRegistry,
    sink: Arc<dyn ObsSink>,
) -> Result<(), RegistryError> {
    reg.register(Box::new(QueryInputListRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(QueryInputSettingsRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(QueryRecordStatusRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(QuerySceneListRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(QueryStreamStatusRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(SwitchCurrentSceneRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(SetVisibleRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(SetMuteRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(RecordStartRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(RecordStopRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(RecordPauseRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(RecordResumeRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(RecordTogglePauseRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(StreamStartRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(StreamStopRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(StreamSendCaptionRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(SetPreviewSceneRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(SetTransitionRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(SetVolumeRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(SetInputSettingsRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ReplayStartRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ReplayStopRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ReplaySaveRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(StudioEnableRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(StudioDisableRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(StudioTriggerTransitionRunner::new(Arc::clone(
        &sink,
    ))))?;
    reg.register(Box::new(RawRequestRunner::new(sink)))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::runners::test_support::MockSink;

    #[test]
    fn all_expected_runner_ids_are_present() {
        let mut reg = SubActionRegistry::new();
        register_obs_sub_actions(&mut reg, Arc::new(MockSink)).unwrap();
        for id in &[
            "obs.scenes.get_list",
            "obs.sources.get_list",
            "obs.record.get_status",
            "obs.stream.get_status",
            "obs.sources.get_input_settings",
            "obs.scenes.switch_current",
            "obs.scenes.set_preview",
            "obs.scenes.set_transition",
            "obs.sources.set_visible",
            "obs.sources.set_input_settings",
            "obs.audio.set_mute",
            "obs.audio.set_volume",
            "obs.record.start",
            "obs.record.stop",
            "obs.record.pause",
            "obs.record.resume",
            "obs.record.toggle_pause",
            "obs.stream.start",
            "obs.stream.stop",
            "obs.stream.send_caption",
            "obs.replay.start",
            "obs.replay.stop",
            "obs.replay.save",
            "obs.studio.enable",
            "obs.studio.disable",
            "obs.studio.trigger_transition",
            "obs.misc.raw_request",
        ] {
            assert!(reg.get(id).is_some(), "missing runner: {id}");
        }
    }

    #[test]
    fn duplicate_registration_returns_error() {
        let mut reg = SubActionRegistry::new();
        register_obs_sub_actions(&mut reg, Arc::new(MockSink)).unwrap();
        let result = register_obs_sub_actions(&mut reg, Arc::new(MockSink));
        assert!(result.is_err());
    }
}
