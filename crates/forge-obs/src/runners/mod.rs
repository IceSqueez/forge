mod raw_request;
mod record_start;
mod record_stop;
mod set_mute;
mod set_visible;
mod stream_start;
mod stream_stop;
mod switch_current;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};

pub use raw_request::RawRequestRunner;
pub use record_start::RecordStartRunner;
pub use record_stop::RecordStopRunner;
pub use set_mute::SetMuteRunner;
pub use set_visible::SetVisibleRunner;
pub use stream_start::StreamStartRunner;
pub use stream_stop::StreamStopRunner;
pub use switch_current::SwitchCurrentSceneRunner;

use crate::ObsSink;

pub fn register_obs_sub_actions(
    reg: &mut SubActionRegistry,
    sink: Arc<dyn ObsSink>,
) -> Result<(), RegistryError> {
    reg.register(Box::new(SwitchCurrentSceneRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(SetVisibleRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(SetMuteRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(RecordStartRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(RecordStopRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(StreamStartRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(StreamStopRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(RawRequestRunner::new(sink)))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use async_trait::async_trait;
    use forge_types::Variant;

    use super::*;
    use crate::ObsError;

    struct MockSink;

    #[async_trait]
    impl ObsSink for MockSink {
        async fn set_scene(&self, _: &str) -> Result<(), ObsError> {
            Ok(())
        }
        async fn set_source_visible(&self, _: &str, _: &str, _: bool) -> Result<(), ObsError> {
            Ok(())
        }
        async fn set_input_mute(&self, _: &str, _: bool) -> Result<(), ObsError> {
            Ok(())
        }
        async fn start_record(&self) -> Result<(), ObsError> {
            Ok(())
        }
        async fn stop_record(&self) -> Result<(), ObsError> {
            Ok(())
        }
        async fn start_stream(&self) -> Result<(), ObsError> {
            Ok(())
        }
        async fn stop_stream(&self) -> Result<(), ObsError> {
            Ok(())
        }
        async fn raw_request(&self, _: &str, _: &Variant) -> Result<Variant, ObsError> {
            Ok(Variant::Object(BTreeMap::new()))
        }
    }

    #[test]
    fn all_expected_runner_ids_are_present() {
        let mut reg = SubActionRegistry::new();
        register_obs_sub_actions(&mut reg, Arc::new(MockSink)).unwrap();
        for id in &[
            "obs.scenes.switch_current",
            "obs.sources.set_visible",
            "obs.audio.set_mute",
            "obs.record.start",
            "obs.record.stop",
            "obs.stream.start",
            "obs.stream.stop",
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
