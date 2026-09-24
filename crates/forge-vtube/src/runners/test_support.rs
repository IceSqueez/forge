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
    fail_item_move: bool,
    called: AtomicBool,
}

impl MockSink {
    pub(crate) fn new() -> Self {
        Self {
            fail: false,
            fail_item_move: false,
            called: AtomicBool::new(false),
        }
    }

    pub(crate) fn failing() -> Self {
        Self {
            fail: true,
            fail_item_move: false,
            called: AtomicBool::new(false),
        }
    }

    /// Serves every call but rejects `move_item`, reproducing a spawn that lands in the scene
    /// and then fails to travel.
    pub(crate) fn failing_item_move() -> Self {
        Self {
            fail: false,
            fail_item_move: true,
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
        if self.fail_item_move {
            self.called.store(true, Ordering::Release);
            return Err(VTubeError::NotConnected);
        }
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

    async fn pin_item(
        &self,
        _: &str,
        _: bool,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: f64,
        _: f64,
    ) -> Result<(), VTubeError> {
        self.record()
    }

    async fn load_item(
        &self,
        _: &str,
        _: Option<f64>,
        _: Option<f64>,
        _: Option<f64>,
        _: Option<f64>,
        _: Option<f64>,
        _: Option<i64>,
        _: bool,
    ) -> Result<Variant, VTubeError> {
        self.record_lookup(Variant::Object(BTreeMap::from([
            (
                "instance_id".to_owned(),
                Variant::String("inst-new-1".to_owned()),
            ),
            (
                "file_name".to_owned(),
                Variant::String("crown.png".to_owned()),
            ),
        ])))
    }

    async fn unload_all_items(&self) -> Result<(), VTubeError> {
        self.record()
    }

    async fn tint_all_art_meshes(
        &self,
        _: i64,
        _: i64,
        _: i64,
        _: i64,
        _: Option<f64>,
    ) -> Result<(), VTubeError> {
        self.record()
    }

    async fn set_physics_override(&self, _: f64, _: f64) -> Result<(), VTubeError> {
        self.record()
    }
}

struct NoopPublisher;

impl EventPublisher for NoopPublisher {
    fn publish(&self, _: Event) {}
}

pub(crate) fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
    RunContext::leaf(stack, 0, EventId::new(), &NoopPublisher)
}

/// Records every sink call with its numeric arguments in declaration order (integers widened
/// to `f64`), so a test can check exactly what number a runner handed to VTube Studio.
type RecordedCall = (&'static str, Vec<Option<f64>>);

#[derive(Default)]
pub(crate) struct RecordingSink {
    calls: std::sync::Mutex<Vec<RecordedCall>>,
}

impl RecordingSink {
    pub(crate) fn numbers_sent_to(&self, method: &str) -> Option<Vec<Option<f64>>> {
        self.calls
            .lock()
            .ok()?
            .iter()
            .rev()
            .find(|(name, _)| *name == method)
            .map(|(_, args)| args.clone())
    }

    pub(crate) fn was_called(&self) -> bool {
        self.calls.lock().is_ok_and(|calls| !calls.is_empty())
    }

    fn record(&self, method: &'static str, args: Vec<Option<f64>>) {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push((method, args));
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn widen(v: Option<i64>) -> Option<f64> {
    v.map(|i| i as f64)
}

#[async_trait]
impl VTubeSink for RecordingSink {
    async fn trigger_hotkey(&self, _: &str) -> Result<(), VTubeError> {
        self.record("trigger_hotkey", Vec::new());
        Ok(())
    }
    async fn set_expression(&self, _: &str, _: bool) -> Result<(), VTubeError> {
        self.record("set_expression", Vec::new());
        Ok(())
    }
    async fn set_param(&self, _: &str, value: f64) -> Result<(), VTubeError> {
        self.record("set_param", vec![Some(value)]);
        Ok(())
    }
    async fn load_model(&self, _: &str) -> Result<(), VTubeError> {
        self.record("load_model", Vec::new());
        Ok(())
    }
    async fn reset_params(&self) -> Result<(), VTubeError> {
        self.record("reset_params", Vec::new());
        Ok(())
    }
    async fn move_model(
        &self,
        x: Option<f64>,
        y: Option<f64>,
        rotation: Option<f64>,
        size: Option<f64>,
        time_in_seconds: f64,
    ) -> Result<(), VTubeError> {
        self.record(
            "move_model",
            vec![x, y, rotation, size, Some(time_in_seconds)],
        );
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    async fn move_item(
        &self,
        _: &str,
        x: Option<f64>,
        y: Option<f64>,
        size: Option<f64>,
        rotation: Option<f64>,
        order: Option<i64>,
        time_in_seconds: f64,
        _: &str,
    ) -> Result<(), VTubeError> {
        self.record(
            "move_item",
            vec![x, y, size, rotation, widen(order), Some(time_in_seconds)],
        );
        Ok(())
    }
    async fn get_current_model(&self) -> Result<Variant, VTubeError> {
        Ok(Variant::Object(BTreeMap::new()))
    }
    async fn get_hotkeys(&self) -> Result<Variant, VTubeError> {
        Ok(Variant::Object(BTreeMap::new()))
    }
    async fn get_expressions(&self) -> Result<Variant, VTubeError> {
        Ok(Variant::Object(BTreeMap::new()))
    }
    async fn get_parameters(&self) -> Result<Variant, VTubeError> {
        Ok(Variant::Object(BTreeMap::new()))
    }
    async fn get_items(&self) -> Result<Variant, VTubeError> {
        Ok(Variant::Object(BTreeMap::new()))
    }
    #[allow(clippy::too_many_arguments)]
    async fn pin_item(
        &self,
        _: &str,
        _: bool,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        angle: f64,
        size: f64,
    ) -> Result<(), VTubeError> {
        self.record("pin_item", vec![Some(angle), Some(size)]);
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    async fn load_item(
        &self,
        _: &str,
        x: Option<f64>,
        y: Option<f64>,
        size: Option<f64>,
        rotation: Option<f64>,
        fade_time: Option<f64>,
        order: Option<i64>,
        _: bool,
    ) -> Result<Variant, VTubeError> {
        self.record(
            "load_item",
            vec![x, y, size, rotation, fade_time, widen(order)],
        );
        Ok(Variant::Object(BTreeMap::from([(
            "instance_id".to_owned(),
            Variant::String("inst-new-1".to_owned()),
        )])))
    }
    async fn unload_all_items(&self) -> Result<(), VTubeError> {
        self.record("unload_all_items", Vec::new());
        Ok(())
    }
    async fn tint_all_art_meshes(
        &self,
        r: i64,
        g: i64,
        b: i64,
        a: i64,
        mix_with_scene_lighting: Option<f64>,
    ) -> Result<(), VTubeError> {
        self.record(
            "tint_all_art_meshes",
            vec![
                widen(Some(r)),
                widen(Some(g)),
                widen(Some(b)),
                widen(Some(a)),
                mix_with_scene_lighting,
            ],
        );
        Ok(())
    }
    async fn set_physics_override(&self, strength: f64, seconds: f64) -> Result<(), VTubeError> {
        self.record("set_physics_override", vec![Some(strength), Some(seconds)]);
        Ok(())
    }
}
