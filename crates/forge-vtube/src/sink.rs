use async_trait::async_trait;

use forge_types::Variant;

use crate::error::VTubeError;

#[async_trait]
pub trait VTubeSink: Send + Sync {
    async fn trigger_hotkey(&self, hotkey_id: &str) -> Result<(), VTubeError>;

    async fn set_expression(&self, expression_file: &str, active: bool) -> Result<(), VTubeError>;

    async fn set_param(&self, param_id: &str, value: f64) -> Result<(), VTubeError>;

    async fn load_model(&self, model_id: &str) -> Result<(), VTubeError>;

    async fn reset_params(&self) -> Result<(), VTubeError>;

    async fn move_model(
        &self,
        x: Option<f64>,
        y: Option<f64>,
        rotation: Option<f64>,
        size: Option<f64>,
        time_in_seconds: f64,
    ) -> Result<(), VTubeError>;

    #[expect(
        clippy::too_many_arguments,
        reason = "each argument is one field of the outbound request"
    )]
    async fn move_item(
        &self,
        item_instance_id: &str,
        x: Option<f64>,
        y: Option<f64>,
        size: Option<f64>,
        rotation: Option<f64>,
        order: Option<i64>,
        time_in_seconds: f64,
        fade_mode: &str,
    ) -> Result<(), VTubeError>;

    async fn get_current_model(&self) -> Result<Variant, VTubeError>;

    async fn get_hotkeys(&self) -> Result<Variant, VTubeError>;

    async fn get_expressions(&self) -> Result<Variant, VTubeError>;

    async fn get_parameters(&self) -> Result<Variant, VTubeError>;

    async fn get_items(&self) -> Result<Variant, VTubeError>;

    #[expect(
        clippy::too_many_arguments,
        reason = "each argument is one field of the outbound request"
    )]
    async fn pin_item(
        &self,
        item_instance_id: &str,
        pin: bool,
        angle_relative_to: &str,
        size_relative_to: &str,
        vertex_pin_type: &str,
        model_id: &str,
        art_mesh_id: &str,
        angle: f64,
        size: f64,
    ) -> Result<(), VTubeError>;

    #[expect(
        clippy::too_many_arguments,
        reason = "each argument is one field of the outbound request"
    )]
    async fn load_item(
        &self,
        file_name: &str,
        x: Option<f64>,
        y: Option<f64>,
        size: Option<f64>,
        rotation: Option<f64>,
        fade_time: Option<f64>,
        order: Option<i64>,
        unload_on_disconnect: bool,
    ) -> Result<Variant, VTubeError>;

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
