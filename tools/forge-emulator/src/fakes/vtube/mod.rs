mod config;
mod fake;
mod ledger;
mod protocol;
mod studio;

pub use config::{FakeModel, FakeVTubeConfig};
pub use fake::FakeVTube;
pub use ledger::{TokenCheck, VTubeLedger, VTubePushedEvent, VTubeRequest, VTubeSession};
pub use protocol::{
    ERROR_HOTKEY_LIVE2D_ITEM_NOT_FOUND, ERROR_HOTKEY_NO_MODEL_LOADED, ERROR_HOTKEY_NOT_FOUND,
    ERROR_MOVE_MODEL_MISSING_FIELDS, ERROR_MOVE_MODEL_OUT_OF_RANGE, HOTKEY_TRIGGERED_EVENT,
    ITEM_EVENT, MODEL_CONFIG_CHANGED_EVENT, MODEL_LOADED_EVENT, SUBSCRIBABLE_EVENTS,
    TRACKING_STATUS_CHANGED_EVENT,
};
pub use studio::{SUPPORTED_REQUESTS, hotkey_id, model_id};
