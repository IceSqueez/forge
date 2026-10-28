use serde_json::{Map, Value, json};

use super::config::FakeVTubeConfig;
use super::protocol::{
    ERROR_EXPRESSION_FILE_NOT_FOUND, ERROR_EXPRESSION_INVALID_FILE,
    ERROR_EXPRESSION_NO_MODEL_LOADED, ERROR_EXPRESSION_STATE_FILE_NOT_FOUND,
    ERROR_EXPRESSION_STATE_INVALID_FILE, ERROR_HOTKEY_LIVE2D_ITEM_NOT_FOUND,
    ERROR_HOTKEY_NO_MODEL_LOADED, ERROR_HOTKEY_NOT_FOUND, ERROR_INJECT_MODE_UNKNOWN,
    ERROR_INJECT_NO_DATA, ERROR_INJECT_PARAMETER_NOT_FOUND, ERROR_INJECT_VALUE_INVALID,
    ERROR_INJECT_WEIGHT_INVALID, ERROR_ITEM_FILE_NAME_MISSING, ERROR_ITEM_FILE_NAME_NOT_FOUND,
    ERROR_ITEM_LOAD_VALUES_INVALID, ERROR_ITEM_MOVE_FADE_MODE_INVALID,
    ERROR_ITEM_MOVE_INSTANCE_NOT_FOUND, ERROR_ITEM_MOVE_ORDER_INVALID, ERROR_ITEM_ORDER_TAKEN,
    ERROR_ITEM_PIN_ANGLE_OR_SIZE_TYPE_INVALID, ERROR_ITEM_PIN_ART_MESH_NOT_FOUND,
    ERROR_ITEM_PIN_ITEM_NOT_LOADED, ERROR_ITEM_PIN_MODEL_NOT_FOUND, ERROR_ITEM_SCENE_FULL,
    ERROR_MODEL_ID_MISSING, ERROR_MODEL_ID_NOT_FOUND, ERROR_MOVE_MODEL_MISSING_FIELDS,
    ERROR_MOVE_MODEL_NO_MODEL_LOADED, ERROR_MOVE_MODEL_OUT_OF_RANGE, ERROR_PHYSICS_GROUP_NOT_FOUND,
    ERROR_PHYSICS_NO_MODEL_LOADED, ERROR_PHYSICS_NO_OVERRIDE_VALUE, ERROR_PHYSICS_NO_OVERRIDES,
    ERROR_REQUEST_TYPE_UNKNOWN, ERROR_REQUIRES_PERMISSION, ERROR_TINT_COLOR_INVALID,
    ERROR_TINT_MATCH_OR_COLOR_MISSING, ERROR_TINT_NO_MODEL_LOADED, EXPRESSION_FILE_SUFFIX,
    EXPRESSION_TOGGLED_EVENT, HOTKEY_TRIGGERED_EVENT, ITEM_EVENT, MODEL_CONFIG_CHANGED_EVENT,
    MODEL_LOADED_EVENT, TRACKING_STATUS_CHANGED_EVENT,
};

pub const SUPPORTED_REQUESTS: [&str; 22] = [
    "APIStateRequest",
    "AuthenticationTokenRequest",
    "AuthenticationRequest",
    "EventSubscriptionRequest",
    "CurrentModelRequest",
    "AvailableModelsRequest",
    "ModelLoadRequest",
    "MoveModelRequest",
    "HotkeysInCurrentModelRequest",
    "HotkeyTriggerRequest",
    "ExpressionStateRequest",
    "ExpressionActivationRequest",
    "ColorTintRequest",
    "FaceFoundRequest",
    "InputParameterListRequest",
    "InjectParameterDataRequest",
    "SetCurrentModelPhysicsRequest",
    "ItemListRequest",
    "ItemLoadRequest",
    "ItemUnloadRequest",
    "ItemMoveRequest",
    "ItemPinRequest",
];

const HOTKEY_TYPE: &str = "TriggerAnimation";
const HOTKEY_DESCRIPTION: &str = "Triggers an animation";
const ADDED_BY_VTUBE_STUDIO: &str = "VTube Studio";
const TOTAL_ITEMS_ALLOWED: usize = 60;
const LOWEST_ORDER: i64 = -30;
const HIGHEST_ORDER: i64 = 30;
const MODEL_ORDER: i64 = 0;
const IGNORED_BELOW: f64 = -1000.0;
const POSITION_LIMIT: f64 = 1000.0;
const MAX_FADE_SECONDS: f64 = 2.0;
const MAX_MOVE_MODEL_SECONDS: f64 = 2.0;
const MAX_COLOR_CHANNEL: i64 = 255;
const INJECT_VALUE_LIMIT: f64 = 1_000_000.0;
const TINTED_ART_MESHES: u64 = 24;
const LIVE2D_PARAMETERS: usize = 29;
const FADE_MODES: [&str; 6] = [
    "linear",
    "easeIn",
    "easeOut",
    "easeBoth",
    "overshoot",
    "zip",
];
const ANGLE_RELATIVE_TO: [&str; 4] = [
    "RelativeToWorld",
    "RelativeToCurrentItemRotation",
    "RelativeToModel",
    "RelativeToPinPosition",
];
const SIZE_RELATIVE_TO: [&str; 2] = ["RelativeToWorld", "RelativeToCurrentItemSize"];
const VERTEX_PIN_TYPES: [&str; 3] = ["Provided", "Center", "Random"];

#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Done(Value),
    Refused { error_id: i64, reason: String },
}

impl Answer {
    pub fn refused(error_id: i64, reason: impl Into<String>) -> Self {
        Self::Refused {
            error_id,
            reason: reason.into(),
        }
    }

    pub fn error_id(&self) -> Option<i64> {
        match self {
            Self::Done(_) => None,
            Self::Refused { error_id, .. } => Some(*error_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pushed {
    pub event_name: &'static str,
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal(pub String);

struct Model {
    name: String,
    hotkeys: Vec<String>,
    expressions: Vec<(String, bool)>,
}

struct Item {
    instance_id: String,
    file: String,
    order: i64,
    x: f64,
    y: f64,
    pinned: Option<(String, String)>,
    loaded_by: Option<u64>,
}

pub(crate) struct Studio {
    models: Vec<Model>,
    current: Option<usize>,
    item_files: Vec<String>,
    scene: Vec<Item>,
    next_instance: u64,
    parameters: Vec<String>,
    face_found: bool,
}

pub fn model_id(index: usize) -> String {
    format!("{:032x}", index + 1)
}

pub fn hotkey_id(model: usize, hotkey: usize) -> String {
    format!("{:016x}{:016x}", model + 1, hotkey + 1)
}

fn expression_name(file: &str) -> &str {
    file.strip_suffix(EXPRESSION_FILE_SUFFIX).unwrap_or(file)
}

fn item_type(file: &str) -> &'static str {
    let lower = file.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "PNG"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "JPG"
    } else if lower.ends_with(".gif") {
        "GIF"
    } else {
        "AnimationFolder"
    }
}

fn text<'a>(data: &'a Value, field: &str) -> Option<&'a str> {
    data.get(field).and_then(Value::as_str)
}

fn number(data: &Value, field: &str) -> Option<f64> {
    data.get(field).and_then(Value::as_f64)
}

fn flag(data: &Value, field: &str) -> bool {
    data.get(field).and_then(Value::as_bool).unwrap_or(false)
}

fn outside(value: Option<f64>, low: f64, high: f64) -> bool {
    value.is_some_and(|value| !(low..=high).contains(&value))
}

impl Studio {
    pub(crate) fn new(config: &FakeVTubeConfig) -> Self {
        let current = match &config.current_model {
            Some(name) => config.models.iter().position(|model| &model.name == name),
            None => (!config.models.is_empty()).then_some(0),
        };
        Self {
            models: config
                .models
                .iter()
                .map(|model| Model {
                    name: model.name.clone(),
                    hotkeys: model.hotkeys.clone(),
                    expressions: model
                        .expressions
                        .iter()
                        .map(|file| (file.clone(), false))
                        .collect(),
                })
                .collect(),
            current,
            item_files: config.items.clone(),
            scene: Vec::new(),
            next_instance: 1,
            parameters: config.parameters.clone(),
            face_found: config.face_found,
        }
    }

    pub(crate) fn current_model(&self) -> Option<&str> {
        self.current.map(|index| self.models[index].name.as_str())
    }

    pub(crate) fn face_found(&self) -> bool {
        self.face_found
    }

    pub(crate) fn expression_active(&self, file: &str) -> Option<bool> {
        let model = &self.models[self.current?];
        model
            .expressions
            .iter()
            .find(|(known, _)| known == file)
            .map(|(_, active)| *active)
    }

    pub(crate) fn items_in_scene(&self) -> Vec<String> {
        self.scene.iter().map(|item| item.file.clone()).collect()
    }

    pub(crate) fn answer(
        &mut self,
        session: u64,
        message_type: &str,
        data: &Value,
    ) -> (Answer, Vec<Pushed>) {
        match message_type {
            "CurrentModelRequest" => (self.current_model_answer(), Vec::new()),
            "AvailableModelsRequest" => (self.available_models(), Vec::new()),
            "ModelLoadRequest" => self.model_load(data),
            "MoveModelRequest" => (self.move_model(data), Vec::new()),
            "HotkeysInCurrentModelRequest" => (self.hotkey_list(data), Vec::new()),
            "HotkeyTriggerRequest" => self.hotkey_trigger(data),
            "ExpressionStateRequest" => (self.expression_state(data), Vec::new()),
            "ExpressionActivationRequest" => self.expression_activation(data),
            "ColorTintRequest" => (self.color_tint(data), Vec::new()),
            "FaceFoundRequest" => (
                Answer::Done(json!({ "found": self.face_found })),
                Vec::new(),
            ),
            "InputParameterListRequest" => (self.parameter_list(), Vec::new()),
            "InjectParameterDataRequest" => (self.inject_parameters(data), Vec::new()),
            "SetCurrentModelPhysicsRequest" => (self.physics(data), Vec::new()),
            "ItemListRequest" => (self.item_list(data), Vec::new()),
            "ItemLoadRequest" => self.item_load(session, data),
            "ItemUnloadRequest" => self.item_unload(session, data),
            "ItemMoveRequest" => (self.item_move(data), Vec::new()),
            "ItemPinRequest" => (self.item_pin(data), Vec::new()),
            _ => (
                Answer::refused(
                    ERROR_REQUEST_TYPE_UNKNOWN,
                    format!("The request type `{message_type}` is unknown."),
                ),
                Vec::new(),
            ),
        }
    }

    pub(crate) fn user_triggers_hotkey(&mut self, hotkey: &str) -> Result<Vec<Pushed>, Refusal> {
        let model = self
            .current
            .ok_or_else(|| Refusal("the fake VTube Studio has no model loaded".to_owned()))?;
        let index = self.models[model]
            .hotkeys
            .iter()
            .position(|known| known == hotkey)
            .ok_or_else(|| {
                Refusal(format!(
                    "the loaded model `{}` has no hotkey named `{hotkey}`",
                    self.models[model].name
                ))
            })?;
        Ok(vec![self.hotkey_event(model, index, false)])
    }

    pub(crate) fn user_loads_model(&mut self, name: &str) -> Result<Vec<Pushed>, Refusal> {
        let index = self
            .models
            .iter()
            .position(|model| model.name == name)
            .ok_or_else(|| Refusal(format!("the fake VTube Studio has no model named `{name}`")))?;
        if self.current == Some(index) {
            return Err(Refusal(format!("`{name}` is already the loaded model")));
        }
        Ok(self.switch_model(Some(index)))
    }

    pub(crate) fn user_unloads_model(&mut self) -> Result<Vec<Pushed>, Refusal> {
        if self.current.is_none() {
            return Err(Refusal("no model is loaded, so none can unload".to_owned()));
        }
        Ok(self.switch_model(None))
    }

    pub(crate) fn user_changes_model_config(&mut self) -> Result<Vec<Pushed>, Refusal> {
        let index = self
            .current
            .ok_or_else(|| Refusal("the fake VTube Studio has no model loaded".to_owned()))?;
        Ok(vec![Pushed {
            event_name: MODEL_CONFIG_CHANGED_EVENT,
            data: json!({
                "modelID": model_id(index),
                "modelName": self.models[index].name,
                "hotkeyConfigChanged": false,
            }),
        }])
    }

    pub(crate) fn tracking_changes(&mut self, face_found: bool) -> Result<Vec<Pushed>, Refusal> {
        if self.face_found == face_found {
            return Err(Refusal(format!(
                "the tracker already reports the face as {}",
                if face_found { "found" } else { "lost" }
            )));
        }
        self.face_found = face_found;
        Ok(vec![Pushed {
            event_name: TRACKING_STATUS_CHANGED_EVENT,
            data: json!({
                "faceFound": face_found,
                "leftHandFound": false,
                "rightHandFound": false,
            }),
        }])
    }

    pub(crate) fn user_adds_item(&mut self, file: &str) -> Result<Vec<Pushed>, Refusal> {
        if !self.item_files.iter().any(|known| known == file) {
            return Err(Refusal(format!(
                "the fake VTube Studio has no item file `{file}`"
            )));
        }
        let order = self
            .free_order(1)
            .ok_or_else(|| Refusal("the scene has no free item order left".to_owned()))?;
        Ok(vec![self.place_item(file, order, 0.0, 0.0, None)])
    }

    pub(crate) fn user_removes_item(&mut self, file: &str) -> Result<Vec<Pushed>, Refusal> {
        let position = self
            .scene
            .iter()
            .position(|item| item.file == file)
            .ok_or_else(|| Refusal(format!("no `{file}` item is in the scene")))?;
        let item = self.scene.remove(position);
        Ok(vec![item_event("Removed", &item)])
    }

    pub(crate) fn user_sets_expression(
        &mut self,
        file: &str,
        active: bool,
    ) -> Result<Vec<Pushed>, Refusal> {
        let model = self
            .current
            .ok_or_else(|| Refusal("the fake VTube Studio has no model loaded".to_owned()))?;
        match self.expression_active(file) {
            None => Err(Refusal(format!(
                "the loaded model `{}` has no expression `{file}`",
                self.models[model].name
            ))),
            Some(state) if state == active => Err(Refusal(format!(
                "`{file}` is already {}",
                if active { "active" } else { "inactive" }
            ))),
            Some(_) => Ok(self.toggle_expression(model, file, active)),
        }
    }

    fn switch_model(&mut self, next: Option<usize>) -> Vec<Pushed> {
        let mut pushed = Vec::new();
        if let Some(previous) = self.current.take() {
            for (_, active) in &mut self.models[previous].expressions {
                *active = false;
            }
            pushed.push(self.model_loaded_event(previous, false));
        }
        self.current = next;
        if let Some(index) = next {
            pushed.push(self.model_loaded_event(index, true));
        }
        pushed
    }

    fn model_loaded_event(&self, index: usize, loaded: bool) -> Pushed {
        Pushed {
            event_name: MODEL_LOADED_EVENT,
            data: json!({
                "modelLoaded": loaded,
                "modelName": self.models[index].name,
                "modelID": model_id(index),
            }),
        }
    }

    fn hotkey_event(&self, model: usize, hotkey: usize, by_api: bool) -> Pushed {
        Pushed {
            event_name: HOTKEY_TRIGGERED_EVENT,
            data: json!({
                "hotkeyID": hotkey_id(model, hotkey),
                "hotkeyName": self.models[model].hotkeys[hotkey],
                "hotkeyAction": HOTKEY_TYPE,
                "hotkeyFile": "",
                "hotkeyTriggeredByAPI": by_api,
                "modelID": model_id(model),
                "modelName": self.models[model].name,
                "isLive2DItem": false,
            }),
        }
    }

    fn toggle_expression(&mut self, model: usize, file: &str, active: bool) -> Vec<Pushed> {
        let Some(entry) = self.models[model]
            .expressions
            .iter_mut()
            .find(|(known, _)| known == file)
        else {
            return Vec::new();
        };
        if entry.1 == active {
            return Vec::new();
        }
        entry.1 = active;
        vec![Pushed {
            event_name: EXPRESSION_TOGGLED_EVENT,
            data: json!({
                "modelID": model_id(model),
                "modelName": self.models[model].name,
                "isLive2DItem": false,
                "itemInstanceID": "",
                "justLoaded": false,
                "expressionFile": file,
                "expressionName": expression_name(file),
                "active": active,
            }),
        }]
    }

    fn loaded_fields(&self) -> Map<String, Value> {
        let mut fields = Map::new();
        fields.insert("modelLoaded".to_owned(), json!(self.current.is_some()));
        fields.insert(
            "modelName".to_owned(),
            json!(self.current_model().unwrap_or_default()),
        );
        fields.insert(
            "modelID".to_owned(),
            json!(self.current.map(model_id).unwrap_or_default()),
        );
        fields
    }

    fn current_model_answer(&self) -> Answer {
        let mut fields = self.loaded_fields();
        let loaded = self.current.is_some();
        let name = self.current_model().unwrap_or_default();
        let file_name = |extension: &str| {
            if loaded {
                format!("{name}{extension}")
            } else {
                String::new()
            }
        };
        let count = |value: u64| if loaded { value } else { 0 };
        fields.insert("vtsModelName".to_owned(), json!(file_name(".vtube.json")));
        fields.insert("vtsModelIconName".to_owned(), json!(""));
        fields.insert(
            "live2DModelName".to_owned(),
            json!(file_name(".model3.json")),
        );
        fields.insert("modelLoadTime".to_owned(), json!(0));
        fields.insert("timeSinceModelLoaded".to_owned(), json!(0));
        fields.insert(
            "numberOfLive2DParameters".to_owned(),
            json!(count(LIVE2D_PARAMETERS as u64)),
        );
        fields.insert(
            "numberOfLive2DArtmeshes".to_owned(),
            json!(count(TINTED_ART_MESHES)),
        );
        fields.insert("hasPhysicsFile".to_owned(), json!(loaded));
        fields.insert("numberOfTextures".to_owned(), json!(count(1)));
        fields.insert("textureResolution".to_owned(), json!(count(2048)));
        fields.insert(
            "modelPosition".to_owned(),
            json!({ "positionX": 0.0, "positionY": 0.0, "rotation": 0.0, "size": 0.0 }),
        );
        Answer::Done(Value::Object(fields))
    }

    fn available_models(&self) -> Answer {
        let models: Vec<Value> = self
            .models
            .iter()
            .enumerate()
            .map(|(index, model)| {
                json!({
                    "modelLoaded": self.current == Some(index),
                    "modelName": model.name,
                    "modelID": model_id(index),
                    "vtsModelName": format!("{}.vtube.json", model.name),
                    "vtsModelIconName": "",
                })
            })
            .collect();
        Answer::Done(json!({ "numberOfModels": models.len(), "availableModels": models }))
    }

    fn model_load(&mut self, data: &Value) -> (Answer, Vec<Pushed>) {
        let Some(requested) = text(data, "modelID") else {
            return (
                Answer::refused(ERROR_MODEL_ID_MISSING, "The modelID field is missing."),
                Vec::new(),
            );
        };
        if requested.is_empty() {
            let pushed = if self.current.is_some() {
                self.switch_model(None)
            } else {
                Vec::new()
            };
            return (Answer::Done(json!({ "modelID": "" })), pushed);
        }
        let Some(index) = (0..self.models.len()).find(|index| model_id(*index) == requested) else {
            return (
                Answer::refused(
                    ERROR_MODEL_ID_NOT_FOUND,
                    format!("No model with the ID `{requested}` was found."),
                ),
                Vec::new(),
            );
        };
        let pushed = if self.current == Some(index) {
            Vec::new()
        } else {
            self.switch_model(Some(index))
        };
        (Answer::Done(json!({ "modelID": requested })), pushed)
    }

    fn move_model(&self, data: &Value) -> Answer {
        if self.current.is_none() {
            return Answer::refused(ERROR_MOVE_MODEL_NO_MODEL_LOADED, "No model is loaded.");
        }
        let seconds = number(data, "timeInSeconds");
        if seconds.is_none() || data.get("valuesAreRelativeToModel").is_none() {
            return Answer::refused(
                ERROR_MOVE_MODEL_MISSING_FIELDS,
                "timeInSeconds and valuesAreRelativeToModel are required.",
            );
        }
        if outside(seconds, 0.0, MAX_MOVE_MODEL_SECONDS) {
            return Answer::refused(
                ERROR_MOVE_MODEL_OUT_OF_RANGE,
                "timeInSeconds has to be between 0 and 2.",
            );
        }
        Answer::Done(json!({}))
    }

    fn hotkey_list(&self, data: &Value) -> Answer {
        let requested = text(data, "modelID").filter(|id| !id.is_empty());
        let model = match requested {
            Some(id) => match (0..self.models.len()).find(|index| model_id(*index) == id) {
                Some(index) => Some(index),
                None => {
                    return Answer::refused(
                        ERROR_MODEL_ID_NOT_FOUND,
                        format!("No model with the ID `{id}` was found."),
                    );
                }
            },
            None => self.current,
        };
        let hotkeys: Vec<Value> = model
            .map(|index| {
                self.models[index]
                    .hotkeys
                    .iter()
                    .enumerate()
                    .map(|(position, name)| {
                        json!({
                            "name": name,
                            "type": HOTKEY_TYPE,
                            "description": HOTKEY_DESCRIPTION,
                            "file": "",
                            "hotkeyID": hotkey_id(index, position),
                            "keyCombination": [],
                            "onScreenButtonID": -1,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Answer::Done(json!({
            "modelLoaded": model.is_some_and(|index| self.current == Some(index)),
            "modelName": model.map(|index| self.models[index].name.clone()).unwrap_or_default(),
            "modelID": model.map(model_id).unwrap_or_default(),
            "availableHotkeys": hotkeys,
        }))
    }

    fn hotkey_trigger(&mut self, data: &Value) -> (Answer, Vec<Pushed>) {
        if text(data, "itemInstanceID").is_some_and(|id| !id.is_empty()) {
            return (
                Answer::refused(
                    ERROR_HOTKEY_LIVE2D_ITEM_NOT_FOUND,
                    "No Live2D item with that instance ID is loaded.",
                ),
                Vec::new(),
            );
        }
        let Some(model) = self.current else {
            return (
                Answer::refused(ERROR_HOTKEY_NO_MODEL_LOADED, "No model is loaded."),
                Vec::new(),
            );
        };
        let requested = text(data, "hotkeyID").unwrap_or_default();
        let found = self.models[model]
            .hotkeys
            .iter()
            .enumerate()
            .position(|(position, name)| {
                !requested.is_empty()
                    && (hotkey_id(model, position) == requested
                        || name.eq_ignore_ascii_case(requested))
            });
        match found {
            Some(position) => (
                Answer::Done(json!({ "hotkeyID": hotkey_id(model, position) })),
                vec![self.hotkey_event(model, position, true)],
            ),
            None => (
                Answer::refused(
                    ERROR_HOTKEY_NOT_FOUND,
                    format!("No hotkey `{requested}` was found in the current model."),
                ),
                Vec::new(),
            ),
        }
    }

    fn expression_entries(&self, only: Option<&str>) -> Vec<Value> {
        let Some(model) = self.current else {
            return Vec::new();
        };
        self.models[model]
            .expressions
            .iter()
            .filter(|(file, _)| only.is_none_or(|only| only == file))
            .map(|(file, active)| {
                json!({
                    "name": expression_name(file),
                    "file": file,
                    "active": active,
                    "deactivateWhenKeyIsLetGo": false,
                    "autoDeactivateAfterSeconds": false,
                    "secondsRemaining": 0,
                    "usedInHotkeys": [],
                    "parameters": [],
                })
            })
            .collect()
    }

    fn expression_state(&self, data: &Value) -> Answer {
        let only = text(data, "expressionFile").filter(|file| !file.is_empty());
        if let Some(file) = only {
            if !file.ends_with(EXPRESSION_FILE_SUFFIX) {
                return Answer::refused(
                    ERROR_EXPRESSION_STATE_INVALID_FILE,
                    "The expression file name has to end in .exp3.json.",
                );
            }
            if self.expression_active(file).is_none() {
                return Answer::refused(
                    ERROR_EXPRESSION_STATE_FILE_NOT_FOUND,
                    format!("The expression `{file}` was not found in the current model."),
                );
            }
        }
        let mut fields = self.loaded_fields();
        fields.insert(
            "expressions".to_owned(),
            Value::Array(self.expression_entries(only)),
        );
        Answer::Done(Value::Object(fields))
    }

    fn expression_activation(&mut self, data: &Value) -> (Answer, Vec<Pushed>) {
        let file = text(data, "expressionFile").unwrap_or_default();
        if !file.ends_with(EXPRESSION_FILE_SUFFIX) {
            return (
                Answer::refused(
                    ERROR_EXPRESSION_INVALID_FILE,
                    "The expression file name has to end in .exp3.json.",
                ),
                Vec::new(),
            );
        }
        let Some(model) = self.current else {
            return (
                Answer::refused(ERROR_EXPRESSION_NO_MODEL_LOADED, "No model is loaded."),
                Vec::new(),
            );
        };
        if self.expression_active(file).is_none() {
            return (
                Answer::refused(
                    ERROR_EXPRESSION_FILE_NOT_FOUND,
                    format!("The expression `{file}` was not found in the current model."),
                ),
                Vec::new(),
            );
        }
        let pushed = self.toggle_expression(model, file, flag(data, "active"));
        (Answer::Done(json!({})), pushed)
    }

    fn color_tint(&self, data: &Value) -> Answer {
        if self.current.is_none() {
            return Answer::refused(ERROR_TINT_NO_MODEL_LOADED, "No model is loaded.");
        }
        let tint = data.get("colorTint");
        let channels: Option<Vec<i64>> = ["colorR", "colorG", "colorB", "colorA"]
            .iter()
            .map(|channel| {
                tint.and_then(|tint| tint.get(*channel))
                    .and_then(Value::as_i64)
            })
            .collect();
        let matcher = data
            .get("artMeshMatcher")
            .filter(|matcher| matcher.is_object());
        let (Some(channels), Some(matcher)) = (channels, matcher) else {
            return Answer::refused(
                ERROR_TINT_MATCH_OR_COLOR_MISSING,
                "The color or the ArtMesh matcher is missing.",
            );
        };
        if channels
            .iter()
            .any(|channel| !(0..=MAX_COLOR_CHANNEL).contains(channel))
        {
            return Answer::refused(
                ERROR_TINT_COLOR_INVALID,
                "Color values have to be between 0 and 255.",
            );
        }
        let matched = if flag(matcher, "tintAll") {
            TINTED_ART_MESHES
        } else {
            0
        };
        Answer::Done(json!({ "matchedArtMeshes": matched }))
    }

    fn parameter_list(&self) -> Answer {
        let mut fields = self.loaded_fields();
        fields.insert("customParameters".to_owned(), json!([]));
        let defaults: Vec<Value> = self
            .parameters
            .iter()
            .map(|name| {
                json!({
                    "name": name,
                    "addedBy": ADDED_BY_VTUBE_STUDIO,
                    "value": 0.0,
                    "min": -1.0,
                    "max": 1.0,
                    "defaultValue": 0.0,
                })
            })
            .collect();
        fields.insert("defaultParameters".to_owned(), Value::Array(defaults));
        Answer::Done(Value::Object(fields))
    }

    fn inject_parameters(&self, data: &Value) -> Answer {
        if text(data, "mode").is_some_and(|mode| mode != "set" && mode != "add") {
            return Answer::refused(
                ERROR_INJECT_MODE_UNKNOWN,
                "Only \"set\" and \"add\" are valid modes.",
            );
        }
        let values = data
            .get("parameterValues")
            .and_then(Value::as_array)
            .filter(|values| !values.is_empty());
        let Some(values) = values else {
            return Answer::refused(ERROR_INJECT_NO_DATA, "No parameter values were provided.");
        };
        for value in values {
            let id = text(value, "id").unwrap_or_default();
            if !self.parameters.iter().any(|known| known == id) {
                return Answer::refused(
                    ERROR_INJECT_PARAMETER_NOT_FOUND,
                    format!("The parameter `{id}` does not exist."),
                );
            }
            let given = number(value, "value");
            if given.is_none() || outside(given, -INJECT_VALUE_LIMIT, INJECT_VALUE_LIMIT) {
                return Answer::refused(
                    ERROR_INJECT_VALUE_INVALID,
                    format!("The value for `{id}` is missing or out of range."),
                );
            }
            if outside(number(value, "weight"), 0.0, 1.0) {
                return Answer::refused(
                    ERROR_INJECT_WEIGHT_INVALID,
                    format!("The weight for `{id}` has to be between 0 and 1."),
                );
            }
        }
        Answer::Done(json!({}))
    }

    fn physics(&self, data: &Value) -> Answer {
        if self.current.is_none() {
            return Answer::refused(ERROR_PHYSICS_NO_MODEL_LOADED, "No model is loaded.");
        }
        let overrides: Vec<&Value> = ["strengthOverrides", "windOverrides"]
            .iter()
            .filter_map(|field| data.get(*field).and_then(Value::as_array))
            .flatten()
            .collect();
        if overrides.is_empty() {
            return Answer::refused(ERROR_PHYSICS_NO_OVERRIDES, "No overrides were provided.");
        }
        for entry in overrides {
            if number(entry, "value").is_none() {
                return Answer::refused(
                    ERROR_PHYSICS_NO_OVERRIDE_VALUE,
                    "An override has no value.",
                );
            }
            let group = text(entry, "id").unwrap_or_default();
            if !flag(entry, "setBaseValue") && !group.is_empty() {
                return Answer::refused(
                    ERROR_PHYSICS_GROUP_NOT_FOUND,
                    format!("The physics group `{group}` was not found."),
                );
            }
        }
        Answer::Done(json!({}))
    }

    fn taken(&self, order: i64) -> bool {
        self.scene.iter().any(|item| item.order == order)
    }

    fn spot_is_free(&self, order: i64) -> bool {
        (LOWEST_ORDER..=HIGHEST_ORDER).contains(&order)
            && order != MODEL_ORDER
            && !self.taken(order)
    }

    fn free_order(&self, wanted: i64) -> Option<i64> {
        if self.scene.len() >= TOTAL_ITEMS_ALLOWED {
            return None;
        }
        let start = wanted.clamp(LOWEST_ORDER, HIGHEST_ORDER);
        (start..=HIGHEST_ORDER)
            .chain((LOWEST_ORDER..start).rev())
            .find(|order| self.spot_is_free(*order))
    }

    fn place_item(
        &mut self,
        file: &str,
        order: i64,
        x: f64,
        y: f64,
        loaded_by: Option<u64>,
    ) -> Pushed {
        let instance_id = format!("{:032x}", 0xa000_0000_u64 + self.next_instance);
        self.next_instance += 1;
        let item = Item {
            instance_id,
            file: file.to_owned(),
            order,
            x,
            y,
            pinned: None,
            loaded_by,
        };
        let pushed = item_event("Added", &item);
        self.scene.push(item);
        pushed
    }

    fn item_list(&self, data: &Value) -> Answer {
        let only_file = text(data, "onlyItemsWithFileName").filter(|file| !file.is_empty());
        let only_instance = text(data, "onlyItemsWithInstanceID").filter(|id| !id.is_empty());
        let spots: Vec<i64> = if flag(data, "includeAvailableSpots") {
            (LOWEST_ORDER..=HIGHEST_ORDER)
                .filter(|order| self.spot_is_free(*order))
                .collect()
        } else {
            Vec::new()
        };
        let instances: Vec<Value> = if flag(data, "includeItemInstancesInScene") {
            self.scene
                .iter()
                .filter(|item| only_file.is_none_or(|file| item.file == file))
                .filter(|item| only_instance.is_none_or(|id| item.instance_id == id))
                .map(|item| {
                    let (pinned_model, pinned_mesh) = item.pinned.clone().unwrap_or_default();
                    json!({
                        "fileName": item.file,
                        "instanceID": item.instance_id,
                        "order": item.order,
                        "type": item_type(&item.file),
                        "censored": false,
                        "flipped": false,
                        "locked": false,
                        "smoothing": 0.0,
                        "framerate": 0.0,
                        "frameCount": -1,
                        "currentFrame": -1,
                        "pinnedToModel": item.pinned.is_some(),
                        "pinnedModelID": pinned_model,
                        "pinnedArtMeshID": pinned_mesh,
                        "groupName": "",
                        "sceneName": "",
                        "fromWorkshop": false,
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        let files: Vec<Value> = if flag(data, "includeAvailableItemFiles") {
            self.item_files
                .iter()
                .filter(|file| only_file.is_none_or(|only| *file == only))
                .map(|file| {
                    json!({
                        "fileName": file,
                        "type": item_type(file),
                        "loadedCount": self.scene.iter().filter(|item| &item.file == file).count(),
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        Answer::Done(json!({
            "itemsInSceneCount": self.scene.len(),
            "totalItemsAllowedCount": TOTAL_ITEMS_ALLOWED,
            "canLoadItemsRightNow": true,
            "availableSpots": spots,
            "itemInstancesInScene": instances,
            "availableItemFiles": files,
        }))
    }

    fn item_load(&mut self, session: u64, data: &Value) -> (Answer, Vec<Pushed>) {
        let refuse =
            |error_id: i64, reason: String| (Answer::refused(error_id, reason), Vec::new());
        let file = text(data, "fileName").unwrap_or_default();
        if file.is_empty() {
            return refuse(
                ERROR_ITEM_FILE_NAME_MISSING,
                "The fileName field is missing.".to_owned(),
            );
        }
        if text(data, "customDataBase64").is_some_and(|custom| !custom.is_empty()) {
            return refuse(
                ERROR_REQUIRES_PERMISSION,
                "Loading custom image data needs the LoadCustomImagesAsItems permission."
                    .to_owned(),
            );
        }
        if !self.item_files.iter().any(|known| known == file) {
            return refuse(
                ERROR_ITEM_FILE_NAME_NOT_FOUND,
                format!("No item file `{file}` was found."),
            );
        }
        let x = number(data, "positionX");
        let y = number(data, "positionY");
        let invalid = outside(x, -POSITION_LIMIT, POSITION_LIMIT)
            || outside(y, -POSITION_LIMIT, POSITION_LIMIT)
            || outside(number(data, "size"), 0.0, 1.0)
            || outside(number(data, "fadeTime"), 0.0, MAX_FADE_SECONDS)
            || outside(number(data, "smoothing"), 0.0, 1.0);
        if invalid {
            return refuse(
                ERROR_ITEM_LOAD_VALUES_INVALID,
                "Size, position, fade time or smoothing is out of range.".to_owned(),
            );
        }
        let wanted = data.get("order").and_then(Value::as_i64).unwrap_or(1);
        if flag(data, "failIfOrderTaken") && !self.spot_is_free(wanted) {
            return refuse(
                ERROR_ITEM_ORDER_TAKEN,
                format!("The order {wanted} is already taken."),
            );
        }
        let Some(order) = self.free_order(wanted) else {
            return refuse(ERROR_ITEM_SCENE_FULL, "The scene is full.".to_owned());
        };
        let pushed = self.place_item(
            file,
            order,
            x.unwrap_or_default(),
            y.unwrap_or_default(),
            Some(session),
        );
        let instance_id = self
            .scene
            .last()
            .map(|item| item.instance_id.clone())
            .unwrap_or_default();
        (
            Answer::Done(json!({ "instanceID": instance_id, "fileName": file })),
            vec![pushed],
        )
    }

    fn item_unload(&mut self, session: u64, data: &Value) -> (Answer, Vec<Pushed>) {
        let listed = |field: &str| -> Vec<String> {
            data.get(field)
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        let instance_ids = listed("instanceIDs");
        let file_names = listed("fileNames");
        let all = flag(data, "unloadAllInScene");
        let own = flag(data, "unloadAllLoadedByThisPlugin");
        let others_allowed = data
            .get("allowUnloadingItemsLoadedByUserOrOtherPlugins")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let (removed, kept): (Vec<Item>, Vec<Item>) = std::mem::take(&mut self.scene)
            .into_iter()
            .partition(|item| {
                if all {
                    return true;
                }
                let mine = item.loaded_by == Some(session);
                let chosen = (own && mine)
                    || instance_ids.contains(&item.instance_id)
                    || file_names.contains(&item.file);
                chosen && (mine || others_allowed)
            });
        self.scene = kept;
        let unloaded: Vec<Value> = removed
            .iter()
            .map(|item| json!({ "instanceID": item.instance_id, "fileName": item.file }))
            .collect();
        let pushed = removed
            .iter()
            .map(|item| item_event("Removed", item))
            .collect();
        (Answer::Done(json!({ "unloadedItems": unloaded })), pushed)
    }

    fn item_move(&mut self, data: &Value) -> Answer {
        let requested = data
            .get("itemsToMove")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut moved = Vec::with_capacity(requested.len());
        for entry in &requested {
            let id = text(entry, "itemInstanceID").unwrap_or_default().to_owned();
            let error_id = self.move_one(entry, &id);
            moved.push(json!({
                "itemInstanceID": id,
                "success": error_id.is_none(),
                "errorID": error_id.unwrap_or(-1),
            }));
        }
        Answer::Done(json!({ "movedItems": moved }))
    }

    fn move_one(&mut self, entry: &Value, id: &str) -> Option<i64> {
        let Some(index) = self.scene.iter().position(|item| item.instance_id == id) else {
            return Some(ERROR_ITEM_MOVE_INSTANCE_NOT_FOUND);
        };
        if text(entry, "fadeMode").is_some_and(|mode| !FADE_MODES.contains(&mode)) {
            return Some(ERROR_ITEM_MOVE_FADE_MODE_INVALID);
        }
        let order = entry
            .get("order")
            .and_then(Value::as_f64)
            .filter(|order| *order > IGNORED_BELOW);
        if let Some(order) = order {
            let order = order as i64;
            if order != self.scene[index].order && !self.spot_is_free(order) {
                return Some(ERROR_ITEM_MOVE_ORDER_INVALID);
            }
            self.scene[index].order = order;
        }
        if let Some(x) = number(entry, "positionX").filter(|x| *x > IGNORED_BELOW) {
            self.scene[index].x = x;
        }
        if let Some(y) = number(entry, "positionY").filter(|y| *y > IGNORED_BELOW) {
            self.scene[index].y = y;
        }
        None
    }

    fn item_pin(&mut self, data: &Value) -> Answer {
        let id = text(data, "itemInstanceID").unwrap_or_default();
        let Some(index) = self.scene.iter().position(|item| item.instance_id == id) else {
            return Answer::refused(
                ERROR_ITEM_PIN_ITEM_NOT_LOADED,
                format!("No item with the instance ID `{id}` is loaded."),
            );
        };
        if !flag(data, "pin") {
            self.scene[index].pinned = None;
            return self.pin_answer(index);
        }
        let types_valid = text(data, "angleRelativeTo")
            .is_some_and(|t| ANGLE_RELATIVE_TO.contains(&t))
            && text(data, "sizeRelativeTo").is_some_and(|t| SIZE_RELATIVE_TO.contains(&t))
            && text(data, "vertexPinType").is_some_and(|t| VERTEX_PIN_TYPES.contains(&t));
        if !types_valid {
            return Answer::refused(
                ERROR_ITEM_PIN_ANGLE_OR_SIZE_TYPE_INVALID,
                "The angle, size or vertex pin type is invalid.",
            );
        }
        let info = data.get("pinInfo").cloned().unwrap_or_else(|| json!({}));
        let Some(model) = self.current else {
            return Answer::refused(ERROR_ITEM_PIN_MODEL_NOT_FOUND, "No model is loaded.");
        };
        let wanted_model = text(&info, "modelID").unwrap_or_default();
        if !wanted_model.is_empty() && wanted_model != model_id(model) {
            return Answer::refused(
                ERROR_ITEM_PIN_MODEL_NOT_FOUND,
                format!("The model `{wanted_model}` is not loaded."),
            );
        }
        let art_mesh = text(&info, "artMeshID").unwrap_or_default();
        if !art_mesh.is_empty() {
            return Answer::refused(
                ERROR_ITEM_PIN_ART_MESH_NOT_FOUND,
                format!("The model has no ArtMesh `{art_mesh}`."),
            );
        }
        self.scene[index].pinned = Some((model_id(model), String::new()));
        self.pin_answer(index)
    }

    fn pin_answer(&self, index: usize) -> Answer {
        let item = &self.scene[index];
        Answer::Done(json!({
            "isPinned": item.pinned.is_some(),
            "itemInstanceID": item.instance_id,
            "itemFileName": item.file,
        }))
    }
}

fn item_event(kind: &str, item: &Item) -> Pushed {
    Pushed {
        event_name: ITEM_EVENT,
        data: json!({
            "itemEventType": kind,
            "itemInstanceID": item.instance_id,
            "itemFileName": item.file,
            "itemPosition": { "x": item.x, "y": item.y },
        }),
    }
}
