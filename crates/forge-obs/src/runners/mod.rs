mod browser_refresh;
mod capture_screenshot;
mod filter_set_enabled;
mod media_restart;
mod profile_switch;
mod query_input_list;
mod query_input_settings;
mod query_record_status;
mod query_scene_list;
mod query_stream_status;
mod raw_request;
mod record_pause;
mod record_resume;
mod record_set_active;
mod record_set_directory;
mod record_start;
mod record_stop;
mod record_toggle_pause;
mod replay_save;
mod replay_set_active;
mod replay_start;
mod replay_stop;
mod scene_collection_switch;
mod set_input_settings;
mod set_mute;
mod set_preview_scene;
mod set_transition;
mod set_visible;
mod set_volume;
mod stream_send_caption;
mod stream_set_active;
mod stream_start;
mod stream_stop;
mod studio_disable;
mod studio_enable;
mod studio_set_enabled;
mod studio_trigger_transition;
mod switch_current;
mod virtualcam_set_active;

#[cfg(test)]
pub(crate) mod test_support;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};

pub use browser_refresh::BrowserRefreshRunner;
pub use capture_screenshot::CaptureScreenshotRunner;
pub use filter_set_enabled::FilterSetEnabledRunner;
pub use media_restart::MediaRestartRunner;
pub use profile_switch::ProfileSwitchRunner;
pub use query_input_list::QueryInputListRunner;
pub use query_input_settings::QueryInputSettingsRunner;
pub use query_record_status::QueryRecordStatusRunner;
pub use query_scene_list::QuerySceneListRunner;
pub use query_stream_status::QueryStreamStatusRunner;
pub use raw_request::RawRequestRunner;
pub use record_pause::RecordPauseRunner;
pub use record_resume::RecordResumeRunner;
pub use record_set_active::RecordSetActiveRunner;
pub use record_set_directory::RecordSetDirectoryRunner;
pub use record_start::RecordStartRunner;
pub use record_stop::RecordStopRunner;
pub use record_toggle_pause::RecordTogglePauseRunner;
pub use replay_save::ReplaySaveRunner;
pub use replay_set_active::ReplaySetActiveRunner;
pub use replay_start::ReplayStartRunner;
pub use replay_stop::ReplayStopRunner;
pub use scene_collection_switch::SceneCollectionSwitchRunner;
pub use set_input_settings::SetInputSettingsRunner;
pub use set_mute::SetMuteRunner;
pub use set_preview_scene::SetPreviewSceneRunner;
pub use set_transition::SetTransitionRunner;
pub use set_visible::SetVisibleRunner;
pub use set_volume::SetVolumeRunner;
pub use stream_send_caption::StreamSendCaptionRunner;
pub use stream_set_active::StreamSetActiveRunner;
pub use stream_start::StreamStartRunner;
pub use stream_stop::StreamStopRunner;
pub use studio_disable::StudioDisableRunner;
pub use studio_enable::StudioEnableRunner;
pub use studio_set_enabled::StudioSetEnabledRunner;
pub use studio_trigger_transition::StudioTriggerTransitionRunner;
pub use switch_current::SwitchCurrentSceneRunner;
pub use virtualcam_set_active::VirtualCamSetActiveRunner;

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
    reg.register(Box::new(StudioSetEnabledRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(FilterSetEnabledRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(BrowserRefreshRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(MediaRestartRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(StreamSetActiveRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(RecordSetActiveRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(VirtualCamSetActiveRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ReplaySetActiveRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(CaptureScreenshotRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(RecordSetDirectoryRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ProfileSwitchRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(SceneCollectionSwitchRunner::new(Arc::clone(
        &sink,
    ))))?;
    reg.register(Box::new(RawRequestRunner::new(sink)))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use forge_types::{ArgStack, SubActionOutcome, Variant};

    use super::*;
    use crate::runners::test_support::{MockSink, RecordingSink, make_ctx};

    /// Runners whose only config is an `on` flag that picks between two sink calls.
    const SET_ACTIVE_RUNNERS: &[(&str, &str, &str)] = &[
        ("obs.stream.set_active", "start_stream", "stop_stream"),
        ("obs.record.set_active", "start_record", "stop_record"),
        (
            "obs.virtualcam.set_active",
            "start_virtual_cam",
            "stop_virtual_cam",
        ),
        (
            "obs.replay.set_active",
            "start_replay_buffer",
            "stop_replay_buffer",
        ),
    ];

    /// Runners whose only config is one interpolated string forwarded to one sink call.
    const SINGLE_STRING_RUNNERS: &[(&str, &str, &str)] = &[
        ("obs.browser.refresh", "source", "refresh_browser_source"),
        ("obs.media.restart", "source", "restart_media_input"),
        ("obs.record.set_directory", "path", "set_record_directory"),
        ("obs.profile.switch", "name", "set_current_profile"),
        (
            "obs.scene_collection.switch",
            "name",
            "set_current_scene_collection",
        ),
    ];

    fn registry_with(sink: Arc<dyn ObsSink>) -> SubActionRegistry {
        let mut reg = SubActionRegistry::new();
        register_obs_sub_actions(&mut reg, sink).unwrap();
        reg
    }

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
            "obs.studio.set_enabled",
            "obs.stream.set_active",
            "obs.record.set_active",
            "obs.virtualcam.set_active",
            "obs.replay.set_active",
            "obs.filter.set_enabled",
            "obs.browser.refresh",
            "obs.media.restart",
            "obs.capture.screenshot",
            "obs.record.set_directory",
            "obs.profile.switch",
            "obs.scene_collection.switch",
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

    #[tokio::test]
    async fn set_active_runners_route_the_on_flag_to_exactly_one_matching_sink_call() {
        for (id, start_call, stop_call) in SET_ACTIVE_RUNNERS {
            for (on, expected) in [(true, start_call), (false, stop_call)] {
                let sink = RecordingSink::new();
                let reg = registry_with(Arc::clone(&sink) as Arc<dyn ObsSink>);
                let runner = reg.get(id).unwrap();
                let stack = ArgStack::new();
                let config = BTreeMap::from([("on".to_owned(), Variant::Bool(on))]);

                runner.execute(&config, &make_ctx(&stack)).await;

                assert_eq!(
                    sink.calls(),
                    vec![(*expected).to_owned()],
                    "{id} with on={on}",
                );
            }
        }
    }

    #[tokio::test]
    async fn studio_set_enabled_forwards_the_on_flag_as_the_studio_mode_argument() {
        for on in [true, false] {
            let sink = RecordingSink::new();
            let reg = registry_with(Arc::clone(&sink) as Arc<dyn ObsSink>);
            let runner = reg.get("obs.studio.set_enabled").unwrap();
            let stack = ArgStack::new();
            let config = BTreeMap::from([("on".to_owned(), Variant::Bool(on))]);

            runner.execute(&config, &make_ctx(&stack)).await;

            assert_eq!(sink.calls(), vec![format!("set_studio_mode({on})")]);
        }
    }

    #[test]
    fn on_flag_runners_reject_a_missing_or_non_bool_flag() {
        let reg = registry_with(Arc::new(MockSink));
        let bad_configs = [
            BTreeMap::new(),
            BTreeMap::from([("on".to_owned(), Variant::String("true".to_owned()))]),
            BTreeMap::from([("on".to_owned(), Variant::Int(1))]),
        ];
        for (id, _, _) in
            SET_ACTIVE_RUNNERS
                .iter()
                .chain(std::iter::once(&("obs.studio.set_enabled", "", "")))
        {
            let runner = reg.get(id).unwrap();
            for config in &bad_configs {
                assert!(
                    runner.validate_config(config).is_err(),
                    "{id} accepted {config:?}",
                );
            }
            assert!(
                runner
                    .validate_config(&BTreeMap::from([("on".to_owned(), Variant::Bool(false))]))
                    .is_ok(),
                "{id} rejected a valid flag",
            );
        }
    }

    #[tokio::test]
    async fn single_string_runners_forward_the_interpolated_value_to_their_sink_call() {
        for (id, key, sink_call) in SINGLE_STRING_RUNNERS {
            let sink = RecordingSink::new();
            let reg = registry_with(Arc::clone(&sink) as Arc<dyn ObsSink>);
            let runner = reg.get(id).unwrap();
            let stack = ArgStack::new().set(
                "picked".to_owned(),
                Variant::String("Late Night".to_owned()),
            );
            let config = BTreeMap::from([(
                (*key).to_owned(),
                Variant::String("%picked% Setup".to_owned()),
            )]);

            runner.execute(&config, &make_ctx(&stack)).await;

            assert_eq!(
                sink.calls(),
                vec![format!("{sink_call}(Late Night Setup)")],
                "{id} did not forward the interpolated value",
            );
        }
    }

    #[test]
    fn single_string_runners_reject_a_non_string_value() {
        let reg = registry_with(Arc::new(MockSink));
        for (id, key, _) in SINGLE_STRING_RUNNERS {
            let runner = reg.get(id).unwrap();
            for value in [Variant::Bool(true), Variant::Int(3)] {
                let config = BTreeMap::from([((*key).to_owned(), value.clone())]);
                assert!(
                    runner.validate_config(&config).is_err(),
                    "{id} accepted {value:?}",
                );
            }
            assert!(
                runner.validate_config(&BTreeMap::new()).is_err(),
                "{id} accepted a missing {key}",
            );
        }
    }

    #[tokio::test]
    async fn every_screen_roster_runner_reports_a_failed_outcome_when_the_sink_errors() {
        let cases: Vec<(&str, BTreeMap<String, Variant>)> = vec![
            ("obs.stream.set_active", on_flag(true)),
            ("obs.record.set_active", on_flag(false)),
            ("obs.virtualcam.set_active", on_flag(true)),
            ("obs.replay.set_active", on_flag(false)),
            ("obs.studio.set_enabled", on_flag(true)),
            ("obs.browser.refresh", one_string("source", "Overlay")),
            ("obs.media.restart", one_string("source", "Intro Video")),
            ("obs.record.set_directory", one_string("path", "/tmp/rec")),
            ("obs.profile.switch", one_string("name", "Streaming")),
            ("obs.scene_collection.switch", one_string("name", "Main")),
            (
                "obs.filter.set_enabled",
                BTreeMap::from([
                    ("source".to_owned(), Variant::String("Cam".to_owned())),
                    ("filter".to_owned(), Variant::String("Blur".to_owned())),
                    ("enabled".to_owned(), Variant::Bool(true)),
                ]),
            ),
            (
                "obs.capture.screenshot",
                BTreeMap::from([
                    ("source".to_owned(), Variant::String("Cam".to_owned())),
                    ("path".to_owned(), Variant::String("/tmp/a.png".to_owned())),
                ]),
            ),
        ];

        let reg = registry_with(RecordingSink::failing() as Arc<dyn ObsSink>);
        for (id, config) in cases {
            let stack = ArgStack::new();
            let runner = reg.get(id).unwrap();
            let (telemetry, extra) = runner.execute(&config, &make_ctx(&stack)).await;
            assert!(
                matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
                "{id} reported {:?} for a failing sink",
                telemetry.outcome,
            );
            assert_eq!(telemetry.kind, id);
            assert!(extra.is_none(), "{id} produced an arg stack");
        }
    }

    fn on_flag(on: bool) -> BTreeMap<String, Variant> {
        BTreeMap::from([("on".to_owned(), Variant::Bool(on))])
    }

    fn one_string(key: &str, value: &str) -> BTreeMap<String, Variant> {
        BTreeMap::from([(key.to_owned(), Variant::String(value.to_owned()))])
    }
}
