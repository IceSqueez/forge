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
}

struct NoopPublisher;

impl EventPublisher for NoopPublisher {
    fn publish(&self, _: Event) {}
}

/// Builds a `RunContext` borrowing the given arg stack with a no-op publisher.
pub(crate) fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
    RunContext {
        arg_stack: stack,
        index: 0,
        parent_event_id: EventId::new(),
        publisher: &NoopPublisher,
    }
}
