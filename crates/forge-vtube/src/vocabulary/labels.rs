use forge_types::MessageCode;

use crate::vocabulary::INTEGRATION_ID;

pub(crate) const AVAILABLE_EXPRESSIONS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_available_expressions");
pub(crate) const COLOR_TINT_OVERLAY: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_color_tint_overlay");
pub(crate) const ENABLED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "label_enabled");
pub(crate) const EXPRESSION: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_expression");
pub(crate) const EXPRESSIONS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_expressions");
pub(crate) const HOTKEY: MessageCode = MessageCode::from_static(INTEGRATION_ID, "label_hotkey");
pub(crate) const HOTKEYS_ANIMATION: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_hotkeys_animation");
pub(crate) const ITEM: MessageCode = MessageCode::from_static(INTEGRATION_ID, "label_item");
pub(crate) const ITEM_FILE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_item_file");
pub(crate) const ITEMS: MessageCode = MessageCode::from_static(INTEGRATION_ID, "label_items");
pub(crate) const LOAD_ITEM: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_load_item");
pub(crate) const LOAD_MODEL: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_load_model");
pub(crate) const MODEL: MessageCode = MessageCode::from_static(INTEGRATION_ID, "label_model");
pub(crate) const MODEL_SCENE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_model_scene");
pub(crate) const MOVE_SCALE_MODEL: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_move_scale_model");
pub(crate) const PIN_ITEM_TO_MODEL: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_pin_item_to_model");
pub(crate) const PLAY_ANIMATION: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_play_animation");
pub(crate) const REMOVE_ALL_ITEMS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_remove_all_items");
pub(crate) const RESET_TO_IDLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_reset_to_idle");
pub(crate) const SET_EXPRESSION: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_set_expression");
pub(crate) const THROW_ITEM: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_throw_item");
pub(crate) const TOGGLE_EXPRESSION: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_toggle_expression");
pub(crate) const TOGGLE_PHYSICS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_toggle_physics");
pub(crate) const TRIGGER_HOTKEY: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_trigger_hotkey");
