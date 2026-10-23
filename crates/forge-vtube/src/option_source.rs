use std::collections::BTreeMap;

use async_trait::async_trait;
use forge_platform_core::{
    BuiltinOptionSource, CollectionFailure, CollectionOutcome, CollectionRevisions, OptionEntry,
    OptionHint, OptionListId, OptionListMetadata, QuickActionFieldValue, SectionIcon,
};
use serde_json::{Value, json};

use crate::catalog::{VTubeChoice, choice};
use crate::client::VTubeClient;
use crate::content::ContentSnapshot;
use crate::error::VTubeError;
use crate::protocol::check_response;

pub const MODEL_LIST: &str = "vtube.models";
pub const HOTKEY_LIST: &str = "vtube.hotkeys";
pub const EXPRESSION_LIST: &str = "vtube.expressions";
pub const ITEM_FILE_LIST: &str = "vtube.item_files";
pub const ITEM_INSTANCE_LIST: &str = "vtube.item_instances";

const HINT_LOADED: &str = "option_hint_loaded";
const HINT_ACTIVE: &str = "option_hint_active";

const MODEL_ICON: &str = "user-square";
const HOTKEY_ICON: &str = "keyboard";
const EXPRESSION_ICON: &str = "mood-smile";
const ITEM_FILE_ICON: &str = "photo";
const ITEM_INSTANCE_ICON: &str = "confetti";

fn listing_failure(error: &VTubeError) -> CollectionFailure {
    match error {
        VTubeError::NotConnected | VTubeError::Connect(_) | VTubeError::ApiDisabled => {
            CollectionFailure::NotConnected
        }
        VTubeError::TokenDenied | VTubeError::TokenTimeout | VTubeError::TokenRejected => {
            CollectionFailure::Unauthorized
        }
        VTubeError::Timeout
        | VTubeError::Request { .. }
        | VTubeError::Rejected { .. }
        | VTubeError::Json(_) => CollectionFailure::Transport,
    }
}

fn marked_entry(choice: VTubeChoice, icon: &str, marker: Option<&'static str>) -> OptionEntry {
    let entry = OptionEntry::new(choice.value, choice.label).with_icon(SectionIcon::new(icon));
    match marker {
        Some(hint) => entry.with_hint(OptionHint::new(hint)).as_current(),
        None => entry,
    }
}

pub(crate) fn model_entries(snapshot: &ContentSnapshot) -> Vec<OptionEntry> {
    let loaded = snapshot.current_model_id.as_deref();
    snapshot
        .models
        .iter()
        .filter_map(|model| {
            let marker = (loaded == Some(model.id.as_str())).then_some(HINT_LOADED);
            choice(&model.id, &model.name).map(|c| marked_entry(c, MODEL_ICON, marker))
        })
        .collect()
}

pub(crate) fn hotkey_entries(snapshot: &ContentSnapshot) -> Vec<OptionEntry> {
    snapshot
        .hotkeys
        .iter()
        .filter_map(|hotkey| {
            choice(&hotkey.id, &hotkey.name).map(|c| marked_entry(c, HOTKEY_ICON, None))
        })
        .collect()
}

pub(crate) fn expression_entries(snapshot: &ContentSnapshot) -> Vec<OptionEntry> {
    snapshot
        .expressions
        .iter()
        .filter_map(|expression| {
            let marker = expression.active.then_some(HINT_ACTIVE);
            choice(&expression.file, &expression.name)
                .map(|c| marked_entry(c, EXPRESSION_ICON, marker))
        })
        .collect()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ItemLists {
    pub files: Vec<OptionEntry>,
    pub instances: Vec<OptionEntry>,
}

fn rows<'a>(data: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    data[key]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
}

fn text<'a>(row: &'a Value, key: &str) -> &'a str {
    row[key].as_str().unwrap_or_default()
}

pub(crate) fn item_lists(data: &Value) -> ItemLists {
    let files = rows(data, "availableItemFiles")
        .filter_map(|row| {
            let in_scene = row["loadedCount"].as_u64().unwrap_or_default() > 0;
            let name = text(row, "fileName");
            choice(name, name).map(|c| {
                let entry =
                    OptionEntry::new(c.value, c.label).with_icon(SectionIcon::new(ITEM_FILE_ICON));
                if in_scene {
                    entry.with_hint(OptionHint::new(HINT_LOADED))
                } else {
                    entry
                }
            })
        })
        .collect();
    let instances = rows(data, "itemInstancesInScene")
        .filter_map(|row| {
            choice(text(row, "instanceID"), text(row, "fileName"))
                .map(|c| marked_entry(c, ITEM_INSTANCE_ICON, None))
        })
        .collect();
    ItemLists { files, instances }
}

impl VTubeClient {
    fn read_content<T>(&self, read: impl FnOnce(&ContentSnapshot) -> T) -> CollectionOutcome<T> {
        if !self.connection_state().is_connected() {
            return Err(CollectionFailure::NotConnected);
        }
        self.content_state
            .read()
            .map(|snapshot| read(&snapshot))
            .map_err(|_| CollectionFailure::Transport)
    }

    async fn fetch_item_lists(&self) -> CollectionOutcome<ItemLists> {
        let data = json!({
            "includeAvailableSpots": false,
            "includeItemInstancesInScene": true,
            "includeAvailableItemFiles": true,
        });
        let response = self
            .send_json_request("ItemListRequest", data)
            .await
            .map_err(|e| listing_failure(&e))?;
        check_response(&response).map_err(|e| listing_failure(&e))?;
        Ok(item_lists(&response))
    }
}

#[async_trait]
impl BuiltinOptionSource for VTubeClient {
    fn option_lists(&self) -> Vec<OptionListMetadata> {
        [
            MODEL_LIST,
            HOTKEY_LIST,
            EXPRESSION_LIST,
            ITEM_FILE_LIST,
            ITEM_INSTANCE_LIST,
        ]
        .into_iter()
        .map(|id| OptionListMetadata::new(OptionListId::new(id)))
        .collect()
    }

    fn revisions(&self) -> CollectionRevisions {
        self.catalog_signal().option_revisions()
    }

    async fn resolve(
        &self,
        list: &OptionListId,
        _: &BTreeMap<String, QuickActionFieldValue>,
    ) -> CollectionOutcome<Vec<OptionEntry>> {
        match list.as_str() {
            MODEL_LIST => self.read_content(model_entries),
            HOTKEY_LIST => self.read_content(hotkey_entries),
            EXPRESSION_LIST => self.read_content(expression_entries),
            ITEM_FILE_LIST => self.fetch_item_lists().await.map(|lists| lists.files),
            ITEM_INSTANCE_LIST => self.fetch_item_lists().await.map(|lists| lists.instances),
            _ => Err(CollectionFailure::NotEligible),
        }
    }
}
