use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_registry::RunContext;
use forge_types::{ArgStack, EventId, Variant};

use crate::error::VTubeError;
use crate::sink::VTubeSink;

pub(crate) struct MockSink {
    fail: bool,
    called: AtomicBool,
}

impl MockSink {
    pub(crate) fn new() -> Self {
        Self {
            fail: false,
            called: AtomicBool::new(false),
        }
    }

    pub(crate) fn failing() -> Self {
        Self {
            fail: true,
            called: AtomicBool::new(false),
        }
    }

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

    fn record_lookup(&self, data: Variant) -> Result<Variant, VTubeError> {
        self.called.store(true, Ordering::Release);
        if self.fail {
            Err(VTubeError::NotConnected)
        } else {
            Ok(data)
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
    async fn get_current_model(&self) -> Result<Variant, VTubeError> {
        self.record_lookup(Variant::Object(BTreeMap::from([
            ("name".to_owned(), Variant::String("MyAvatar".to_owned())),
            ("id".to_owned(), Variant::String("model-abc".to_owned())),
            ("loaded".to_owned(), Variant::Bool(true)),
        ])))
    }
    async fn get_hotkeys(&self) -> Result<Variant, VTubeError> {
        self.record_lookup(Variant::Object(BTreeMap::from([
            (
                "names".to_owned(),
                Variant::Array(vec![
                    Variant::String("Wave".to_owned()),
                    Variant::String("Blush".to_owned()),
                ]),
            ),
            (
                "ids".to_owned(),
                Variant::Array(vec![
                    Variant::String("hk-1".to_owned()),
                    Variant::String("hk-2".to_owned()),
                ]),
            ),
            ("count".to_owned(), Variant::Int(2)),
        ])))
    }
    async fn get_expressions(&self) -> Result<Variant, VTubeError> {
        self.record_lookup(Variant::Object(BTreeMap::from([
            (
                "names".to_owned(),
                Variant::Array(vec![
                    Variant::String("Smile.exp3.json".to_owned()),
                    Variant::String("Angry.exp3.json".to_owned()),
                ]),
            ),
            (
                "active".to_owned(),
                Variant::Array(vec![Variant::Bool(true), Variant::Bool(false)]),
            ),
            ("count".to_owned(), Variant::Int(2)),
        ])))
    }
    async fn get_parameters(&self) -> Result<Variant, VTubeError> {
        self.record_lookup(Variant::Object(BTreeMap::from([
            (
                "names".to_owned(),
                Variant::Array(vec![
                    Variant::String("FaceAngleX".to_owned()),
                    Variant::String("MouthOpen".to_owned()),
                ]),
            ),
            ("count".to_owned(), Variant::Int(2)),
        ])))
    }
    async fn get_items(&self) -> Result<Variant, VTubeError> {
        self.record_lookup(Variant::Object(BTreeMap::from([
            (
                "instance_ids".to_owned(),
                Variant::Array(vec![Variant::String("inst-1".to_owned())]),
            ),
            (
                "file_names".to_owned(),
                Variant::Array(vec![Variant::String("crown.png".to_owned())]),
            ),
            ("count".to_owned(), Variant::Int(1)),
        ])))
    }
}

struct NoopPublisher;

impl EventPublisher for NoopPublisher {
    fn publish(&self, _: Event) {}
}

pub(crate) fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
    RunContext::leaf(stack, 0, EventId::new(), &NoopPublisher)
}
