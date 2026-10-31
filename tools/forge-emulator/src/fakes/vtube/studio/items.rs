use serde_json::{Value, json};

use super::{
    ANGLE_RELATIVE_TO, Answer, FADE_MODES, HIGHEST_ORDER, IGNORED_BELOW, Item, LOWEST_ORDER,
    MAX_FADE_SECONDS, MODEL_ORDER, POSITION_LIMIT, Pushed, SIZE_RELATIVE_TO, Studio,
    TOTAL_ITEMS_ALLOWED, VERTEX_PIN_TYPES, flag, item_type, model_id, number, outside, text,
};
use crate::fakes::vtube::protocol::{
    ERROR_ITEM_FILE_NAME_MISSING, ERROR_ITEM_FILE_NAME_NOT_FOUND, ERROR_ITEM_LOAD_VALUES_INVALID,
    ERROR_ITEM_MOVE_FADE_MODE_INVALID, ERROR_ITEM_MOVE_INSTANCE_NOT_FOUND,
    ERROR_ITEM_MOVE_ORDER_INVALID, ERROR_ITEM_ORDER_TAKEN,
    ERROR_ITEM_PIN_ANGLE_OR_SIZE_TYPE_INVALID, ERROR_ITEM_PIN_ART_MESH_NOT_FOUND,
    ERROR_ITEM_PIN_ITEM_NOT_LOADED, ERROR_ITEM_PIN_MODEL_NOT_FOUND, ERROR_ITEM_SCENE_FULL,
    ERROR_REQUIRES_PERMISSION, ITEM_EVENT,
};

impl Studio {
    fn taken(&self, order: i64) -> bool {
        self.scene.iter().any(|item| item.order == order)
    }

    fn spot_is_free(&self, order: i64) -> bool {
        (LOWEST_ORDER..=HIGHEST_ORDER).contains(&order)
            && order != MODEL_ORDER
            && !self.taken(order)
    }

    pub(super) fn free_order(&self, wanted: i64) -> Option<i64> {
        if self.scene.len() >= TOTAL_ITEMS_ALLOWED {
            return None;
        }
        let start = wanted.clamp(LOWEST_ORDER, HIGHEST_ORDER);
        (start..=HIGHEST_ORDER)
            .chain((LOWEST_ORDER..start).rev())
            .find(|order| self.spot_is_free(*order))
    }

    pub(super) fn place_item(
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

    pub(super) fn item_list(&self, data: &Value) -> Answer {
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

    pub(super) fn item_load(&mut self, session: u64, data: &Value) -> (Answer, Vec<Pushed>) {
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

    pub(super) fn item_unload(&mut self, session: u64, data: &Value) -> (Answer, Vec<Pushed>) {
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

    pub(super) fn item_move(&mut self, data: &Value) -> Answer {
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

    pub(super) fn item_pin(&mut self, data: &Value) -> Answer {
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

pub(super) fn item_event(kind: &str, item: &Item) -> Pushed {
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
