use serde_json::{Value, json};

pub const API_NAME: &str = "VTubeStudioPublicAPI";
pub const API_VERSION: &str = "1.0";
pub const VTUBE_STUDIO_VERSION: &str = "1.32.0";
pub const API_ERROR: &str = "APIError";
pub const EXPRESSION_FILE_SUFFIX: &str = ".exp3.json";
const MAX_REQUEST_ID_CHARS: usize = 64;
const MIN_PLUGIN_NAME_CHARS: usize = 3;
const MAX_PLUGIN_NAME_CHARS: usize = 32;

pub const ERROR_JSON_INVALID: i64 = 2;
pub const ERROR_API_NAME_INVALID: i64 = 3;
pub const ERROR_API_VERSION_INVALID: i64 = 4;
pub const ERROR_REQUEST_ID_INVALID: i64 = 5;
pub const ERROR_REQUEST_TYPE_MISSING: i64 = 6;
pub const ERROR_REQUEST_TYPE_UNKNOWN: i64 = 7;
pub const ERROR_REQUIRES_AUTHENTICATION: i64 = 8;
pub const ERROR_REQUIRES_PERMISSION: i64 = 9;
pub const ERROR_TOKEN_REQUEST_DENIED: i64 = 50;
pub const ERROR_PLUGIN_NAME_INVALID: i64 = 52;
pub const ERROR_DEVELOPER_NAME_INVALID: i64 = 53;
pub const ERROR_TOKEN_MISSING: i64 = 100;
pub const ERROR_PLUGIN_NAME_MISSING: i64 = 101;
pub const ERROR_PLUGIN_DEVELOPER_MISSING: i64 = 102;
pub const ERROR_MODEL_ID_MISSING: i64 = 150;
pub const ERROR_MODEL_ID_NOT_FOUND: i64 = 152;
pub const ERROR_HOTKEY_NO_MODEL_LOADED: i64 = 201;
pub const ERROR_HOTKEY_NOT_FOUND: i64 = 202;
pub const ERROR_HOTKEY_LIVE2D_ITEM_NOT_FOUND: i64 = 207;
pub const ERROR_TINT_NO_MODEL_LOADED: i64 = 250;
pub const ERROR_TINT_MATCH_OR_COLOR_MISSING: i64 = 251;
pub const ERROR_TINT_COLOR_INVALID: i64 = 252;
pub const ERROR_MOVE_MODEL_NO_MODEL_LOADED: i64 = 300;
pub const ERROR_MOVE_MODEL_MISSING_FIELDS: i64 = 301;
pub const ERROR_MOVE_MODEL_OUT_OF_RANGE: i64 = 302;
pub const ERROR_INJECT_NO_DATA: i64 = 450;
pub const ERROR_INJECT_VALUE_INVALID: i64 = 451;
pub const ERROR_INJECT_WEIGHT_INVALID: i64 = 452;
pub const ERROR_INJECT_PARAMETER_NOT_FOUND: i64 = 453;
pub const ERROR_INJECT_MODE_UNKNOWN: i64 = 455;
pub const ERROR_EXPRESSION_STATE_INVALID_FILE: i64 = 600;
pub const ERROR_EXPRESSION_STATE_FILE_NOT_FOUND: i64 = 601;
pub const ERROR_EXPRESSION_INVALID_FILE: i64 = 650;
pub const ERROR_EXPRESSION_FILE_NOT_FOUND: i64 = 651;
pub const ERROR_EXPRESSION_NO_MODEL_LOADED: i64 = 652;
pub const ERROR_PHYSICS_NO_MODEL_LOADED: i64 = 700;
pub const ERROR_PHYSICS_NO_OVERRIDES: i64 = 703;
pub const ERROR_PHYSICS_GROUP_NOT_FOUND: i64 = 704;
pub const ERROR_PHYSICS_NO_OVERRIDE_VALUE: i64 = 705;
pub const ERROR_ITEM_FILE_NAME_MISSING: i64 = 750;
pub const ERROR_ITEM_FILE_NAME_NOT_FOUND: i64 = 751;
pub const ERROR_ITEM_SCENE_FULL: i64 = 754;
pub const ERROR_ITEM_ORDER_TAKEN: i64 = 756;
pub const ERROR_ITEM_LOAD_VALUES_INVALID: i64 = 757;
pub const ERROR_ITEM_MOVE_INSTANCE_NOT_FOUND: i64 = 900;
pub const ERROR_ITEM_MOVE_FADE_MODE_INVALID: i64 = 901;
pub const ERROR_ITEM_MOVE_ORDER_INVALID: i64 = 902;
pub const ERROR_EVENT_TYPE_UNKNOWN: i64 = 950;
pub const ERROR_ITEM_PIN_ITEM_NOT_LOADED: i64 = 1050;
pub const ERROR_ITEM_PIN_ANGLE_OR_SIZE_TYPE_INVALID: i64 = 1051;
pub const ERROR_ITEM_PIN_MODEL_NOT_FOUND: i64 = 1052;
pub const ERROR_ITEM_PIN_ART_MESH_NOT_FOUND: i64 = 1053;

pub const MODEL_LOADED_EVENT: &str = "ModelLoadedEvent";
pub const MODEL_CONFIG_CHANGED_EVENT: &str = "ModelConfigChangedEvent";
pub const HOTKEY_TRIGGERED_EVENT: &str = "HotkeyTriggeredEvent";
pub const TRACKING_STATUS_CHANGED_EVENT: &str = "TrackingStatusChangedEvent";
pub const EXPRESSION_TOGGLED_EVENT: &str = "ExpressionToggledEvent";
pub const ITEM_EVENT: &str = "ItemEvent";

pub const SUBSCRIBABLE_EVENTS: [&str; 16] = [
    "TestEvent",
    MODEL_LOADED_EVENT,
    TRACKING_STATUS_CHANGED_EVENT,
    "BackgroundChangedEvent",
    MODEL_CONFIG_CHANGED_EVENT,
    "ModelMovedEvent",
    "ModelOutlineEvent",
    HOTKEY_TRIGGERED_EVENT,
    EXPRESSION_TOGGLED_EVENT,
    "ModelAnimationEvent",
    ITEM_EVENT,
    "ModelClickedEvent",
    "PostProcessingEvent",
    "Live2DCubismEditorConnectedEvent",
    "ArtMeshTrackingEvent",
    "ArtMeshOutlineEvent",
];

pub fn timestamp_ms() -> i64 {
    let nanos = time::OffsetDateTime::now_utc().unix_timestamp_nanos();
    i64::try_from(nanos / 1_000_000).unwrap_or(i64::MAX)
}

pub fn message(message_type: &str, request_id: &str, data: Value) -> String {
    json!({
        "apiName": API_NAME,
        "apiVersion": API_VERSION,
        "timestamp": timestamp_ms(),
        "requestID": request_id,
        "messageType": message_type,
        "data": data,
    })
    .to_string()
}

pub fn response_type(request_type: &str) -> String {
    match request_type.strip_suffix("Request") {
        Some(stem) => format!("{stem}Response"),
        None => format!("{request_type}Response"),
    }
}

pub fn api_error(request_id: &str, error_id: i64, reason: &str) -> String {
    message(
        API_ERROR,
        request_id,
        json!({ "errorID": error_id, "message": reason }),
    )
}

pub fn request_id_is_valid(request_id: &str) -> bool {
    !request_id.is_empty() && request_id.len() <= MAX_REQUEST_ID_CHARS && request_id.is_ascii()
}

pub fn plugin_name_is_valid(name: &str) -> bool {
    (MIN_PLUGIN_NAME_CHARS..=MAX_PLUGIN_NAME_CHARS).contains(&name.chars().count())
}

pub fn generated_id() -> String {
    let mut bytes = [0u8; 16];
    rand::Rng::fill_bytes(&mut rand::rng(), &mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
