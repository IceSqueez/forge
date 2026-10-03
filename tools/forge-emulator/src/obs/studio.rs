use serde_json::{Value, json};

use super::config::{FakeInput, FakeObsConfig};
use super::protocol::{
    Answer, CURRENT_PROGRAM_SCENE_CHANGED, INPUT_MUTE_STATE_CHANGED, OBS_STUDIO_VERSION,
    OBS_WEBSOCKET_VERSION, OUTPUT_STARTED, OUTPUT_STARTING, OUTPUT_STOPPED, OUTPUT_STOPPING,
    RPC_VERSION, STATUS_MISSING_REQUEST_FIELD, STATUS_OUTPUT_NOT_RUNNING, STATUS_OUTPUT_RUNNING,
    STATUS_RESOURCE_NOT_FOUND, STATUS_STUDIO_MODE_NOT_ACTIVE, STATUS_UNKNOWN_REQUEST_TYPE,
    STREAM_STATE_CHANGED, SUBSCRIBE_INPUTS, SUBSCRIBE_OUTPUTS, SUBSCRIBE_SCENES,
};

pub const SUPPORTED_REQUESTS: [&str; 18] = [
    "GetVersion",
    "GetStats",
    "GetSceneList",
    "GetCurrentProgramScene",
    "SetCurrentProgramScene",
    "GetCurrentPreviewScene",
    "GetSceneItemList",
    "GetSceneItemEnabled",
    "GetSceneItemLocked",
    "GetInputList",
    "GetInputVolume",
    "GetInputMute",
    "SetInputMute",
    "ToggleInputMute",
    "GetStreamStatus",
    "StartStream",
    "StopStream",
    "GetRecordStatus",
];

const IDLE_TIMECODE: &str = "00:00:00.000";
const SOURCE_TYPE_INPUT: &str = "OBS_SOURCE_TYPE_INPUT";

#[derive(Debug, Clone, PartialEq)]
pub struct Pushed {
    pub event_type: &'static str,
    pub intent: u64,
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal(pub String);

pub(crate) struct Studio {
    scenes: Vec<String>,
    current: usize,
    inputs: Vec<FakeInput>,
    streaming: bool,
}

fn scene_uuid(index: usize) -> String {
    format!("00000000-0000-4000-a000-{:012x}", index + 1)
}

fn input_uuid(index: usize) -> String {
    format!("00000000-0000-4000-b000-{:012x}", index + 1)
}

fn missing(field: &str) -> Answer {
    Answer::failure(
        STATUS_MISSING_REQUEST_FIELD,
        format!("Your request is missing the `{field}` field."),
    )
}

impl Studio {
    pub(crate) fn new(config: &FakeObsConfig) -> Self {
        let current = config
            .current_scene
            .as_ref()
            .and_then(|current| config.scenes.iter().position(|scene| scene == current))
            .unwrap_or(0);
        Self {
            scenes: config.scenes.clone(),
            current,
            inputs: config.inputs.clone(),
            streaming: false,
        }
    }

    pub(crate) fn current_scene(&self) -> Option<&str> {
        self.scenes.get(self.current).map(String::as_str)
    }

    pub(crate) fn streaming(&self) -> bool {
        self.streaming
    }

    pub(crate) fn input_muted(&self, name: &str) -> Option<bool> {
        self.inputs
            .iter()
            .find(|input| input.name == name)
            .map(|input| input.muted)
    }

    pub(crate) fn answer(&mut self, request_type: &str, data: &Value) -> (Answer, Vec<Pushed>) {
        let answer = match request_type {
            "GetVersion" => Answer::success(Some(json!({
                "obsVersion": OBS_STUDIO_VERSION,
                "obsWebSocketVersion": OBS_WEBSOCKET_VERSION,
                "rpcVersion": RPC_VERSION,
                "availableRequests": SUPPORTED_REQUESTS,
                "supportedImageFormats": ["png"],
                "platform": "ubuntu",
                "platformDescription": "forge emulator",
            }))),
            "GetStats" => Answer::success(Some(json!({
                "cpuUsage": 1.5,
                "memoryUsage": 256.0,
                "availableDiskSpace": 100_000.0,
                "activeFps": 60.0,
                "averageFrameRenderTime": 0.5,
                "renderSkippedFrames": 0,
                "renderTotalFrames": 0,
                "outputSkippedFrames": 0,
                "outputTotalFrames": 0,
                "webSocketSessionIncomingMessages": 0,
                "webSocketSessionOutgoingMessages": 0,
            }))),
            "GetSceneList" => self.scene_list(),
            "GetCurrentProgramScene" => self.current_program_scene(),
            "SetCurrentProgramScene" => {
                return match self.scene_index(data) {
                    Ok(index) => (Answer::success(None), self.switch_to(index)),
                    Err(answer) => (answer, Vec::new()),
                };
            }
            "GetCurrentPreviewScene" => {
                Answer::failure(STATUS_STUDIO_MODE_NOT_ACTIVE, "Studio mode is not active.")
            }
            "GetSceneItemList" => match self.scene_index(data) {
                Ok(_) => Answer::success(Some(json!({ "sceneItems": self.scene_items() }))),
                Err(answer) => answer,
            },
            "GetSceneItemEnabled" => self.scene_item_flag(data, "sceneItemEnabled", true),
            "GetSceneItemLocked" => self.scene_item_flag(data, "sceneItemLocked", false),
            "GetInputList" => self.input_list(data),
            "GetInputVolume" => match self.input_index(data) {
                Ok(_) => Answer::success(Some(json!({
                    "inputVolumeMul": 1.0,
                    "inputVolumeDb": 0.0,
                }))),
                Err(answer) => answer,
            },
            "GetInputMute" => match self.input_index(data) {
                Ok(index) => {
                    Answer::success(Some(json!({ "inputMuted": self.inputs[index].muted })))
                }
                Err(answer) => answer,
            },
            "SetInputMute" => {
                let index = match self.input_index(data) {
                    Ok(index) => index,
                    Err(answer) => return (answer, Vec::new()),
                };
                let Some(muted) = data.get("inputMuted").and_then(Value::as_bool) else {
                    return (missing("inputMuted"), Vec::new());
                };
                return (Answer::success(None), self.mute(index, muted));
            }
            "ToggleInputMute" => {
                let index = match self.input_index(data) {
                    Ok(index) => index,
                    Err(answer) => return (answer, Vec::new()),
                };
                let muted = !self.inputs[index].muted;
                let pushed = self.mute(index, muted);
                return (
                    Answer::success(Some(json!({ "inputMuted": muted }))),
                    pushed,
                );
            }
            "GetStreamStatus" => Answer::success(Some(json!({
                "outputActive": self.streaming,
                "outputReconnecting": false,
                "outputTimecode": IDLE_TIMECODE,
                "outputDuration": 0,
                "outputCongestion": 0.0,
                "outputBytes": 0,
                "outputSkippedFrames": 0,
                "outputTotalFrames": 0,
            }))),
            "StartStream" | "StopStream" => {
                return match self.stream(request_type == "StartStream") {
                    Ok(pushed) => (Answer::success(None), pushed),
                    Err(answer) => (answer, Vec::new()),
                };
            }
            "GetRecordStatus" => Answer::success(Some(json!({
                "outputActive": false,
                "outputPaused": false,
                "outputTimecode": IDLE_TIMECODE,
                "outputDuration": 0,
                "outputBytes": 0,
            }))),
            _ => Answer::failure(
                STATUS_UNKNOWN_REQUEST_TYPE,
                "Your request type is not valid.",
            ),
        };
        (answer, Vec::new())
    }

    pub(crate) fn switch_scene(&mut self, scene: &str) -> Result<Vec<Pushed>, Refusal> {
        let index = self
            .scenes
            .iter()
            .position(|known| known == scene)
            .ok_or_else(|| Refusal(format!("the fake OBS has no scene named `{scene}`")))?;
        Ok(self.switch_to(index))
    }

    pub(crate) fn set_streaming(&mut self, active: bool) -> Result<Vec<Pushed>, Refusal> {
        self.stream(active)
            .map_err(|answer| Refusal(answer.comment.unwrap_or_default()))
    }

    pub(crate) fn set_input_mute(
        &mut self,
        input: &str,
        muted: bool,
    ) -> Result<Vec<Pushed>, Refusal> {
        let index = self
            .inputs
            .iter()
            .position(|known| known.name == input)
            .ok_or_else(|| Refusal(format!("the fake OBS has no input named `{input}`")))?;
        Ok(self.mute(index, muted))
    }

    fn switch_to(&mut self, index: usize) -> Vec<Pushed> {
        if index == self.current {
            return Vec::new();
        }
        self.current = index;
        vec![Pushed {
            event_type: CURRENT_PROGRAM_SCENE_CHANGED,
            intent: SUBSCRIBE_SCENES,
            data: json!({ "sceneName": self.scenes[index], "sceneUuid": scene_uuid(index) }),
        }]
    }

    fn mute(&mut self, index: usize, muted: bool) -> Vec<Pushed> {
        if self.inputs[index].muted == muted {
            return Vec::new();
        }
        self.inputs[index].muted = muted;
        vec![Pushed {
            event_type: INPUT_MUTE_STATE_CHANGED,
            intent: SUBSCRIBE_INPUTS,
            data: json!({
                "inputName": self.inputs[index].name,
                "inputUuid": input_uuid(index),
                "inputMuted": muted,
            }),
        }]
    }

    fn stream(&mut self, start: bool) -> Result<Vec<Pushed>, Answer> {
        match (start, self.streaming) {
            (true, true) => {
                return Err(Answer::failure(
                    STATUS_OUTPUT_RUNNING,
                    "The stream output is already running.",
                ));
            }
            (false, false) => {
                return Err(Answer::failure(
                    STATUS_OUTPUT_NOT_RUNNING,
                    "The stream output is not running.",
                ));
            }
            _ => {}
        }
        self.streaming = start;
        let (transition, settled) = if start {
            (OUTPUT_STARTING, OUTPUT_STARTED)
        } else {
            (OUTPUT_STOPPING, OUTPUT_STOPPED)
        };
        Ok([(transition, !start), (settled, start)]
            .into_iter()
            .map(|(state, active)| Pushed {
                event_type: STREAM_STATE_CHANGED,
                intent: SUBSCRIBE_OUTPUTS,
                data: json!({ "outputActive": active, "outputState": state }),
            })
            .collect())
    }

    fn scene_list(&self) -> Answer {
        let scenes: Vec<Value> = self
            .scenes
            .iter()
            .enumerate()
            .rev()
            .enumerate()
            .map(|(position, (index, name))| {
                json!({ "sceneIndex": position, "sceneName": name, "sceneUuid": scene_uuid(index) })
            })
            .collect();
        Answer::success(Some(json!({
            "currentProgramSceneName": self.scenes[self.current],
            "currentProgramSceneUuid": scene_uuid(self.current),
            "currentPreviewSceneName": null,
            "currentPreviewSceneUuid": null,
            "scenes": scenes,
        })))
    }

    fn current_program_scene(&self) -> Answer {
        let name = &self.scenes[self.current];
        let uuid = scene_uuid(self.current);
        Answer::success(Some(json!({
            "sceneName": name,
            "sceneUuid": uuid,
            "currentProgramSceneName": name,
            "currentProgramSceneUuid": uuid,
        })))
    }

    fn scene_items(&self) -> Vec<Value> {
        (0..)
            .zip(&self.inputs)
            .map(|(index, input)| {
                json!({
                    "sceneItemId": index + 1,
                    "sceneItemIndex": index,
                    "sourceName": input.name,
                    "sourceType": SOURCE_TYPE_INPUT,
                    "inputKind": input.kind,
                    "isGroup": null,
                })
            })
            .collect()
    }

    fn scene_item_flag(&self, data: &Value, field: &str, value: bool) -> Answer {
        if let Err(answer) = self.scene_index(data) {
            return answer;
        }
        let Some(item_id) = data.get("sceneItemId").and_then(Value::as_u64) else {
            return missing("sceneItemId");
        };
        let known = usize::try_from(item_id)
            .ok()
            .is_some_and(|id| (1..=self.inputs.len()).contains(&id));
        if !known {
            return Answer::failure(
                STATUS_RESOURCE_NOT_FOUND,
                format!("No scene items were found in the specified scene by that ID: {item_id}"),
            );
        }
        Answer::success(Some(json!({ field: value })))
    }

    fn input_list(&self, data: &Value) -> Answer {
        let kind = data.get("inputKind").and_then(Value::as_str);
        let inputs: Vec<Value> = self
            .inputs
            .iter()
            .enumerate()
            .filter(|(_, input)| kind.is_none_or(|kind| input.kind == kind))
            .map(|(index, input)| {
                json!({
                    "inputName": input.name,
                    "inputUuid": input_uuid(index),
                    "inputKind": input.kind,
                    "unversionedInputKind": input.kind,
                })
            })
            .collect();
        Answer::success(Some(json!({ "inputs": inputs })))
    }

    fn scene_index(&self, data: &Value) -> Result<usize, Answer> {
        let found = match (
            data.get("sceneName").and_then(Value::as_str),
            data.get("sceneUuid").and_then(Value::as_str),
        ) {
            (Some(name), _) => self.scenes.iter().position(|scene| scene == name),
            (None, Some(uuid)) => (0..self.scenes.len()).find(|index| scene_uuid(*index) == uuid),
            (None, None) => return Err(missing("sceneName")),
        };
        found.ok_or_else(|| {
            Answer::failure(
                STATUS_RESOURCE_NOT_FOUND,
                "No source was found by the name of `sceneName`.",
            )
        })
    }

    fn input_index(&self, data: &Value) -> Result<usize, Answer> {
        let found = match (
            data.get("inputName").and_then(Value::as_str),
            data.get("inputUuid").and_then(Value::as_str),
        ) {
            (Some(name), _) => self.inputs.iter().position(|input| input.name == name),
            (None, Some(uuid)) => (0..self.inputs.len()).find(|index| input_uuid(*index) == uuid),
            (None, None) => return Err(missing("inputName")),
        };
        found.ok_or_else(|| {
            Answer::failure(
                STATUS_RESOURCE_NOT_FOUND,
                "No source was found by the name of `inputName`.",
            )
        })
    }
}
