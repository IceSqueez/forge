//! Shared test doubles for the VTube sub-action runners.
//!
//! Every runner test needs a `VTubeSink` and a throwaway `RunContext`.
//! Keeping one copy here stops the seven-way drift that recurs whenever the
//! `VTubeSink` trait gains a method.

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_registry::RunContext;
use forge_types::{ArgStack, EventId};

use crate::error::VTubeError;
use crate::sink::VTubeSink;

/// A `VTubeSink` test double covering the three usage modes the runner tests
/// need: success no-op, invocation tracking (`was_called`), and error
/// injection (`failing`). Every method records that it was invoked, so any
/// runner test can assert whether the sink was reached.
pub(crate) struct MockSink {
    fail: bool,
    called: AtomicBool,
}

impl MockSink {
    /// A sink whose every method succeeds.
    pub(crate) fn new() -> Self {
        Self {
            fail: false,
            called: AtomicBool::new(false),
        }
    }

    /// A sink whose every method returns `Err(VTubeError::NotConnected)`.
    pub(crate) fn failing() -> Self {
        Self {
            fail: true,
            called: AtomicBool::new(false),
        }
    }

    /// Whether any sink method has been invoked.
    pub(crate) fn was_called(&self) -> bool {
        self.called.load(Ordering::Acquire)
    }

    fn record(&self) -> Result<(), VTubeError> {
        self.called.store(true, Ordering::Release);
        if self.fail {
            Err(VTubeError::NotConnected)
        } else {
            Ok(())
        }
    }
}

#[async_trait]
impl VTubeSink for MockSink {
    async fn trigger_hotkey(&self, _: &str) -> Result<(), VTubeError> {
        self.record()
    }
    async fn set_expression(&self, _: &str, _: bool) -> Result<(), VTubeError> {
        self.record()
    }
    async fn set_param(&self, _: &str, _: f64) -> Result<(), VTubeError> {
        self.record()
    }
    async fn load_model(&self, _: &str) -> Result<(), VTubeError> {
        self.record()
    }
    async fn reset_params(&self) -> Result<(), VTubeError> {
        self.record()
    }
    async fn move_model(
        &self,
        _: Option<f64>,
        _: Option<f64>,
        _: Option<f64>,
        _: f64,
    ) -> Result<(), VTubeError> {
        self.record()
    }
    #[allow(clippy::too_many_arguments)]
    async fn move_item(
        &self,
        _: &str,
        _: Option<f64>,
        _: Option<f64>,
        _: Option<f64>,
        _: Option<f64>,
        _: Option<i64>,
        _: f64,
        _: &str,
    ) -> Result<(), VTubeError> {
        self.record()
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
