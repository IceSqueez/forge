//! Shared test doubles for the OBS sub-action runners.
//!
//! Every runner test needs a no-op `ObsSink` and a throwaway `RunContext`.
//! Keeping one copy here stops the five-way drift that recurs whenever the
//! `ObsSink` trait gains a method.

use std::collections::BTreeMap;

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_registry::RunContext;
use forge_types::{ArgStack, EventId, Variant};

use crate::{ObsError, ObsSink};

/// An `ObsSink` whose every method succeeds without side effects.
pub(crate) struct MockSink;

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
    async fn set_preview_scene(&self, _: &str) -> Result<(), ObsError> {
        Ok(())
    }
    async fn set_current_scene_transition(&self, _: &str) -> Result<(), ObsError> {
        Ok(())
    }
    async fn set_input_volume_db(&self, _: &str, _: f64) -> Result<(), ObsError> {
        Ok(())
    }
    async fn set_input_settings(&self, _: &str, _: &Variant, _: bool) -> Result<(), ObsError> {
        Ok(())
    }
    async fn pause_record(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn resume_record(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn toggle_record_pause(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn send_stream_caption(&self, _: &str) -> Result<(), ObsError> {
        Ok(())
    }
    async fn start_replay_buffer(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn stop_replay_buffer(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn save_replay_buffer(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn set_studio_mode(&self, _: bool) -> Result<(), ObsError> {
        Ok(())
    }
    async fn trigger_studio_transition(&self) -> Result<(), ObsError> {
        Ok(())
    }
    async fn get_scene_list(&self) -> Result<Variant, ObsError> {
        let mut obj = BTreeMap::new();
        obj.insert(
            "all_names".to_owned(),
            Variant::Array(vec![
                Variant::String("Intro".to_owned()),
                Variant::String("Gameplay".to_owned()),
            ]),
        );
        obj.insert("current".to_owned(), Variant::String("Gameplay".to_owned()));
        Ok(Variant::Object(obj))
    }
    async fn get_input_list(&self) -> Result<Variant, ObsError> {
        let mut obj = BTreeMap::new();
        obj.insert(
            "all_names".to_owned(),
            Variant::Array(vec![
                Variant::String("Mic".to_owned()),
                Variant::String("Desktop Audio".to_owned()),
            ]),
        );
        Ok(Variant::Object(obj))
    }
    async fn get_record_status(&self) -> Result<Variant, ObsError> {
        let mut obj = BTreeMap::new();
        obj.insert("is_active".to_owned(), Variant::Bool(true));
        obj.insert("is_paused".to_owned(), Variant::Bool(false));
        obj.insert("duration_ms".to_owned(), Variant::Int(12_000));
        Ok(Variant::Object(obj))
    }
    async fn get_stream_status(&self) -> Result<Variant, ObsError> {
        let mut obj = BTreeMap::new();
        obj.insert("is_active".to_owned(), Variant::Bool(true));
        obj.insert("duration_ms".to_owned(), Variant::Int(45_000));
        Ok(Variant::Object(obj))
    }
    async fn get_input_settings(&self, _: &str) -> Result<Variant, ObsError> {
        let mut settings = BTreeMap::new();
        settings.insert("text".to_owned(), Variant::String("hello".to_owned()));
        let mut obj = BTreeMap::new();
        obj.insert(
            "kind".to_owned(),
            Variant::String("text_ft2_source_v2".to_owned()),
        );
        obj.insert("settings".to_owned(), Variant::Object(settings));
        Ok(Variant::Object(obj))
    }
}

pub(crate) struct NoopPublisher;

impl EventPublisher for NoopPublisher {
    fn publish(&self, _: Event) {}
}

/// Builds a `RunContext` borrowing the given arg stack with a no-op publisher.
pub(crate) fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
    RunContext::leaf(stack, 0, EventId::new(), &NoopPublisher)
}
