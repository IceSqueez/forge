use async_trait::async_trait;

use crate::error::VTubeError;
use crate::runners::choices::{AngleRelativeTo, FadeMode, SizeRelativeTo, VertexPinType};

#[derive(Debug, Clone, Copy, Default)]
pub struct ItemPlacement {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub size: Option<f64>,
    pub rotation: Option<f64>,
    pub order: Option<i64>,
}

#[derive(Clone, Copy)]
pub struct ItemMove<'a> {
    pub item_instance_id: &'a str,
    pub placement: ItemPlacement,
    pub time_in_seconds: f64,
    pub fade_mode: FadeMode,
}

#[derive(Clone, Copy)]
pub struct ItemPin<'a> {
    pub item_instance_id: &'a str,
    pub pin: bool,
    pub angle_relative_to: AngleRelativeTo,
    pub size_relative_to: SizeRelativeTo,
    pub vertex_pin_type: VertexPinType,
    pub model_id: &'a str,
    pub art_mesh_id: &'a str,
    pub angle: f64,
    pub size: f64,
}

#[derive(Clone, Copy)]
pub struct ItemLoad<'a> {
    pub file_name: &'a str,
    pub placement: ItemPlacement,
    pub fade_time: Option<f64>,
    pub unload_on_disconnect: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelMove {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub rotation: Option<f64>,
    pub size: Option<f64>,
    pub time_in_seconds: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelTint {
    pub color_r: i64,
    pub color_g: i64,
    pub color_b: i64,
    pub color_a: i64,
    pub mix_with_scene_lighting: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentModel {
    pub name: String,
    pub id: String,
    pub loaded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyList {
    pub names: Vec<String>,
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpressionList {
    pub names: Vec<String>,
    pub active: Vec<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterList {
    pub names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemList {
    pub instance_ids: Vec<String>,
    pub file_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedItem {
    pub instance_id: String,
    pub file_name: String,
}

#[async_trait]
pub trait VTubeSink: Send + Sync {
    async fn trigger_hotkey(&self, hotkey_id: &str) -> Result<(), VTubeError>;

    async fn set_expression(&self, expression_file: &str, active: bool) -> Result<(), VTubeError>;

    async fn set_param(&self, param_id: &str, value: f64) -> Result<(), VTubeError>;

    async fn load_model(&self, model_id: &str) -> Result<(), VTubeError>;

    async fn deactivate_all_expressions(&self) -> Result<(), VTubeError>;

    async fn move_model(&self, request: &ModelMove) -> Result<(), VTubeError>;

    async fn move_item(&self, request: &ItemMove<'_>) -> Result<(), VTubeError>;

    async fn get_current_model(&self) -> Result<CurrentModel, VTubeError>;

    async fn get_hotkeys(&self) -> Result<HotkeyList, VTubeError>;

    async fn get_expressions(&self) -> Result<ExpressionList, VTubeError>;

    async fn get_parameters(&self) -> Result<ParameterList, VTubeError>;

    async fn get_items(&self) -> Result<ItemList, VTubeError>;

    async fn pin_item(&self, request: &ItemPin<'_>) -> Result<(), VTubeError>;

    async fn load_item(&self, request: &ItemLoad<'_>) -> Result<LoadedItem, VTubeError>;

    async fn unload_all_items(&self) -> Result<(), VTubeError>;

    async fn tint_all_art_meshes(&self, request: &ModelTint) -> Result<(), VTubeError>;

    async fn set_physics_override(
        &self,
        strength: f64,
        override_seconds: f64,
    ) -> Result<(), VTubeError>;
}
