use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

pub const OBS_STUDIO_VERSION: &str = "31.1.2";
pub const OBS_WEBSOCKET_VERSION: &str = "5.6.2";
pub const RPC_VERSION: u64 = 1;

pub const OP_HELLO: u64 = 0;
pub const OP_IDENTIFY: u64 = 1;
pub const OP_IDENTIFIED: u64 = 2;
pub const OP_REIDENTIFY: u64 = 3;
pub const OP_EVENT: u64 = 5;
pub const OP_REQUEST: u64 = 6;
pub const OP_REQUEST_RESPONSE: u64 = 7;

pub const CLOSE_GOING_AWAY: u16 = 1001;
pub const CLOSE_MESSAGE_DECODE_ERROR: u16 = 4002;
pub const CLOSE_MISSING_DATA_FIELD: u16 = 4003;
pub const CLOSE_UNKNOWN_OP_CODE: u16 = 4006;
pub const CLOSE_NOT_IDENTIFIED: u16 = 4007;
pub const CLOSE_ALREADY_IDENTIFIED: u16 = 4008;
pub const CLOSE_AUTHENTICATION_FAILED: u16 = 4009;
pub const CLOSE_UNSUPPORTED_RPC_VERSION: u16 = 4010;

pub const STATUS_SUCCESS: u16 = 100;
pub const STATUS_UNKNOWN_REQUEST_TYPE: u16 = 204;
pub const STATUS_MISSING_REQUEST_FIELD: u16 = 300;
pub const STATUS_OUTPUT_RUNNING: u16 = 500;
pub const STATUS_OUTPUT_NOT_RUNNING: u16 = 501;
pub const STATUS_STUDIO_MODE_NOT_ACTIVE: u16 = 506;
pub const STATUS_RESOURCE_NOT_FOUND: u16 = 600;

pub const SUBSCRIBE_SCENES: u64 = 1 << 2;
pub const SUBSCRIBE_INPUTS: u64 = 1 << 3;
pub const SUBSCRIBE_OUTPUTS: u64 = 1 << 6;
pub const SUBSCRIBE_ALL: u64 = (1 << 12) - 1;

pub const OUTPUT_STARTING: &str = "OBS_WEBSOCKET_OUTPUT_STARTING";
pub const OUTPUT_STARTED: &str = "OBS_WEBSOCKET_OUTPUT_STARTED";
pub const OUTPUT_STOPPING: &str = "OBS_WEBSOCKET_OUTPUT_STOPPING";
pub const OUTPUT_STOPPED: &str = "OBS_WEBSOCKET_OUTPUT_STOPPED";

pub const CURRENT_PROGRAM_SCENE_CHANGED: &str = "CurrentProgramSceneChanged";
pub const STREAM_STATE_CHANGED: &str = "StreamStateChanged";
pub const INPUT_MUTE_STATE_CHANGED: &str = "InputMuteStateChanged";

pub fn authentication_string(password: &str, salt: &str, challenge: &str) -> String {
    let secret = STANDARD.encode(Sha256::digest(format!("{password}{salt}")));
    STANDARD.encode(Sha256::digest(format!("{secret}{challenge}")))
}

pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::Rng::fill_bytes(&mut rand::rng(), &mut bytes);
    STANDARD.encode(bytes)
}

pub fn frame(op: u64, data: Value) -> String {
    json!({ "op": op, "d": data }).to_string()
}

pub fn hello(authentication: Option<(&str, &str)>) -> String {
    let mut data = Map::new();
    data.insert("obsStudioVersion".to_owned(), json!(OBS_STUDIO_VERSION));
    data.insert(
        "obsWebSocketVersion".to_owned(),
        json!(OBS_WEBSOCKET_VERSION),
    );
    data.insert("rpcVersion".to_owned(), json!(RPC_VERSION));
    if let Some((challenge, salt)) = authentication {
        data.insert(
            "authentication".to_owned(),
            json!({ "challenge": challenge, "salt": salt }),
        );
    }
    frame(OP_HELLO, Value::Object(data))
}

pub fn identified() -> String {
    frame(
        OP_IDENTIFIED,
        json!({ "negotiatedRpcVersion": RPC_VERSION }),
    )
}

pub fn event(event_type: &str, intent: u64, data: &Value) -> String {
    frame(
        OP_EVENT,
        json!({ "eventType": event_type, "eventIntent": intent, "eventData": data }),
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub code: u16,
    pub comment: Option<String>,
    pub data: Option<Value>,
}

impl Answer {
    pub fn success(data: Option<Value>) -> Self {
        Self {
            code: STATUS_SUCCESS,
            comment: None,
            data,
        }
    }

    pub fn failure(code: u16, comment: impl Into<String>) -> Self {
        Self {
            code,
            comment: Some(comment.into()),
            data: None,
        }
    }
}

pub fn response(request_type: &str, request_id: &str, answer: &Answer) -> String {
    let mut status = Map::new();
    status.insert("result".to_owned(), json!(answer.code == STATUS_SUCCESS));
    status.insert("code".to_owned(), json!(answer.code));
    if let Some(comment) = &answer.comment {
        status.insert("comment".to_owned(), json!(comment));
    }
    let mut body = Map::new();
    body.insert("requestType".to_owned(), json!(request_type));
    body.insert("requestId".to_owned(), json!(request_id));
    body.insert("requestStatus".to_owned(), Value::Object(status));
    if let Some(data) = &answer.data {
        body.insert("responseData".to_owned(), data.clone());
    }
    frame(OP_REQUEST_RESPONSE, Value::Object(body))
}
