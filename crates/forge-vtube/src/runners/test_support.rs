use forge_events::test_support::NullPublisher;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use forge_registry::RunContext;
use forge_types::{ArgStack, EventId, MessageArg, UserText, Variant};

use crate::error::VTubeError;
use crate::sink::VTubeSink;

const SET_PARAM: &str = "set_param";

struct RecordedCall {
    method: &'static str,
    numbers: Vec<Option<f64>>,
    strings: Vec<String>,
}

#[derive(Default)]
pub(crate) struct MockSink {
    calls: Mutex<Vec<RecordedCall>>,
    failing: AtomicBool,
    failing_item_move: bool,
    rejected_value: Mutex<Option<f64>>,
}

impl MockSink {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn failing() -> Self {
        let sink = Self::new();
        sink.fail_from_now();
        sink
    }

    pub(crate) fn failing_item_move() -> Self {
        Self {
            failing_item_move: true,
            ..Self::new()
        }
    }

    pub(crate) fn fail_from_now(&self) {
        self.failing.store(true, Ordering::SeqCst);
    }

    pub(crate) fn reject_value(&self, value: f64) {
        *lock(&self.rejected_value) = Some(value);
    }

    pub(crate) fn was_called(&self) -> bool {
        !lock(&self.calls).is_empty()
    }

    pub(crate) fn methods_called(&self) -> Vec<&'static str> {
        lock(&self.calls).iter().map(|call| call.method).collect()
    }

    pub(crate) fn numbers_sent_to(&self, method: &str) -> Option<Vec<Option<f64>>> {
        lock(&self.calls)
            .iter()
            .rev()
            .find(|call| call.method == method)
            .map(|call| call.numbers.clone())
    }

    pub(crate) fn strings_sent_to(&self, method: &str) -> Option<Vec<String>> {
        lock(&self.calls)
            .iter()
            .rev()
            .find(|call| call.method == method)
            .map(|call| call.strings.clone())
    }

    pub(crate) fn params_sent(&self) -> Vec<(String, f64)> {
        lock(&self.calls)
            .iter()
            .filter(|call| call.method == SET_PARAM)
            .flat_map(|call| {
                call.strings
                    .iter()
                    .cloned()
                    .zip(call.numbers.iter().flatten().copied())
            })
            .collect()
    }

    pub(crate) fn sends_to_param(&self, param_id: &str) -> usize {
        self.params_sent()
            .iter()
            .filter(|(id, _)| id == param_id)
            .count()
    }

    pub(crate) fn pin_targets(&self) -> Vec<(String, String)> {
        lock(&self.calls)
            .iter()
            .filter(|call| call.method == "pin_item")
            .filter_map(|call| match call.strings.as_slice() {
                [model_id, art_mesh_id] => Some((model_id.clone(), art_mesh_id.clone())),
                _ => None,
            })
            .collect()
    }

    fn record(
        &self,
        method: &'static str,
        numbers: Vec<Option<f64>>,
        strings: &[&str],
    ) -> Result<(), VTubeError> {
        lock(&self.calls).push(RecordedCall {
            method,
            numbers,
            strings: strings.iter().map(|text| (*text).to_owned()).collect(),
        });
        if self.failing.load(Ordering::SeqCst) {
            Err(VTubeError::NotConnected)
        } else {
            Ok(())
        }
    }

    fn record_lookup(&self, method: &'static str, data: Variant) -> Result<Variant, VTubeError> {
        self.record(method, Vec::new(), &[]).map(|()| data)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[expect(
    clippy::cast_precision_loss,
    reason = "test fixture values are small and bounded"
)]
fn widen(v: Option<i64>) -> Option<f64> {
    v.map(|i| i as f64)
}

#[async_trait]
impl VTubeSink for MockSink {
    async fn trigger_hotkey(&self, hotkey_id: &str) -> Result<(), VTubeError> {
        self.record("trigger_hotkey", Vec::new(), &[hotkey_id])
    }
    async fn set_expression(&self, file: &str, _: bool) -> Result<(), VTubeError> {
        self.record("set_expression", Vec::new(), &[file])
    }
    async fn set_param(&self, param_id: &str, value: f64) -> Result<(), VTubeError> {
        let outcome = self.record(SET_PARAM, vec![Some(value)], &[param_id]);
        if *lock(&self.rejected_value) == Some(value) {
            return Err(VTubeError::NotConnected);
        }
        outcome
    }
    async fn load_model(&self, model_id: &str) -> Result<(), VTubeError> {
        self.record("load_model", Vec::new(), &[model_id])
    }
    async fn deactivate_all_expressions(&self) -> Result<(), VTubeError> {
        self.record("deactivate_all_expressions", Vec::new(), &[])
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
            &[],
        )
    }
    async fn move_item(
        &self,
        item_id: &str,
        x: Option<f64>,
        y: Option<f64>,
        size: Option<f64>,
        rotation: Option<f64>,
        order: Option<i64>,
        time_in_seconds: f64,
        fade_mode: &str,
    ) -> Result<(), VTubeError> {
        let outcome = self.record(
            "move_item",
            vec![x, y, size, rotation, widen(order), Some(time_in_seconds)],
            &[item_id, fade_mode],
        );
        if self.failing_item_move {
            return Err(VTubeError::NotConnected);
        }
        outcome
    }
    async fn get_current_model(&self) -> Result<Variant, VTubeError> {
        self.record_lookup(
            "get_current_model",
            Variant::Object(BTreeMap::from([
                ("name".to_owned(), Variant::String("MyAvatar".to_owned())),
                ("id".to_owned(), Variant::String("model-abc".to_owned())),
                ("loaded".to_owned(), Variant::Bool(true)),
            ])),
        )
    }
    async fn get_hotkeys(&self) -> Result<Variant, VTubeError> {
        self.record_lookup(
            "get_hotkeys",
            Variant::Object(BTreeMap::from([
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
            ])),
        )
    }
    async fn get_expressions(&self) -> Result<Variant, VTubeError> {
        self.record_lookup(
            "get_expressions",
            Variant::Object(BTreeMap::from([
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
            ])),
        )
    }
    async fn get_parameters(&self) -> Result<Variant, VTubeError> {
        self.record_lookup(
            "get_parameters",
            Variant::Object(BTreeMap::from([
                (
                    "names".to_owned(),
                    Variant::Array(vec![
                        Variant::String("FaceAngleX".to_owned()),
                        Variant::String("MouthOpen".to_owned()),
                    ]),
                ),
                ("count".to_owned(), Variant::Int(2)),
            ])),
        )
    }
    async fn get_items(&self) -> Result<Variant, VTubeError> {
        self.record_lookup(
            "get_items",
            Variant::Object(BTreeMap::from([
                (
                    "instance_ids".to_owned(),
                    Variant::Array(vec![Variant::String("inst-1".to_owned())]),
                ),
                (
                    "file_names".to_owned(),
                    Variant::Array(vec![Variant::String("crown.png".to_owned())]),
                ),
                ("count".to_owned(), Variant::Int(1)),
            ])),
        )
    }

    async fn pin_item(
        &self,
        _: &str,
        _: bool,
        _: &str,
        _: &str,
        _: &str,
        model_id: &str,
        art_mesh_id: &str,
        angle: f64,
        size: f64,
    ) -> Result<(), VTubeError> {
        self.record(
            "pin_item",
            vec![Some(angle), Some(size)],
            &[model_id, art_mesh_id],
        )
    }

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
            &[],
        )?;
        Ok(Variant::Object(BTreeMap::from([
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
        self.record("unload_all_items", Vec::new(), &[])
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
            &[],
        )
    }

    async fn set_physics_override(&self, strength: f64, seconds: f64) -> Result<(), VTubeError> {
        self.record(
            "set_physics_override",
            vec![Some(strength), Some(seconds)],
            &[],
        )
    }
}

pub(crate) fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
    RunContext::leaf(stack, 0, EventId::new(), &NullPublisher)
}

pub(crate) fn rejected_field(text: &UserText) -> Option<&str> {
    let UserText::Message(message) = text else {
        return None;
    };
    match message.arg("field") {
        Some(MessageArg::StepFields { keys, .. }) if keys.len() == 1 => Some(keys[0].as_str()),
        _ => None,
    }
}
