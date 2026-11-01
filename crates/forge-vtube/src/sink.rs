use async_trait::async_trait;

use forge_types::Variant;

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

#[async_trait]
pub trait VTubeSink: Send + Sync {
    async fn trigger_hotkey(&self, hotkey_id: &str) -> Result<(), VTubeError>;

    async fn set_expression(&self, expression_file: &str, active: bool) -> Result<(), VTubeError>;

    async fn set_param(&self, param_id: &str, value: f64) -> Result<(), VTubeError>;

    async fn load_model(&self, model_id: &str) -> Result<(), VTubeError>;

    async fn deactivate_all_expressions(&self) -> Result<(), VTubeError>;

    async fn move_model(
        &self,
        x: Option<f64>,
        y: Option<f64>,
        rotation: Option<f64>,
        size: Option<f64>,
        time_in_seconds: f64,
    ) -> Result<(), VTubeError>;

    async fn move_item(&self, request: &ItemMove<'_>) -> Result<(), VTubeError>;

    async fn get_current_model(&self) -> Result<Variant, VTubeError>;

    async fn get_hotkeys(&self) -> Result<Variant, VTubeError>;

    async fn get_expressions(&self) -> Result<Variant, VTubeError>;

    async fn get_parameters(&self) -> Result<Variant, VTubeError>;

    async fn get_items(&self) -> Result<Variant, VTubeError>;

    async fn pin_item(&self, request: &ItemPin<'_>) -> Result<(), VTubeError>;

    async fn load_item(&self, request: &ItemLoad<'_>) -> Result<Variant, VTubeError>;

    async fn unload_all_items(&self) -> Result<(), VTubeError>;

    async fn tint_all_art_meshes(
        &self,
        color_r: i64,
        color_g: i64,
        color_b: i64,
        color_a: i64,
        mix_with_scene_lighting: Option<f64>,
    ) -> Result<(), VTubeError>;

    async fn set_physics_override(
        &self,
        strength: f64,
        override_seconds: f64,
    ) -> Result<(), VTubeError>;
}

pub(crate) mod result_keys {
    pub(crate) const NAME: &str = "name";
    pub(crate) const ID: &str = "id";
    pub(crate) const LOADED: &str = "loaded";
    pub(crate) const NAMES: &str = "names";
    pub(crate) const IDS: &str = "ids";
    pub(crate) const ACTIVE: &str = "active";
    pub(crate) const COUNT: &str = "count";
    pub(crate) const INSTANCE_IDS: &str = "instance_ids";
    pub(crate) const FILE_NAMES: &str = "file_names";
    pub(crate) const INSTANCE_ID: &str = "instance_id";
    pub(crate) const FILE_NAME: &str = "file_name";
}
