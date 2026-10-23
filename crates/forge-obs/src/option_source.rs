use std::collections::BTreeMap;

use async_trait::async_trait;
use forge_platform_core::{
    BuiltinOptionSource, CollectionFailure, CollectionOutcome, CollectionRevisions, OptionEntry,
    OptionHint, OptionListId, OptionListMetadata, QuickActionFieldValue, SectionIcon,
};

use crate::catalog::{ObsCatalog, icon_for_kind};
use crate::client::ObsClient;
use crate::error::ObsError;

pub const SCENE_LIST: &str = "obs.scenes";
pub const SOURCE_LIST: &str = "obs.sources";
pub const INPUT_LIST: &str = "obs.inputs";
pub const TRANSITION_LIST: &str = "obs.transitions";
pub const PROFILE_LIST: &str = "obs.profiles";
pub const SCENE_COLLECTION_LIST: &str = "obs.scene_collections";

pub const SCENE_FIELD: &str = "scene";

const HINT_LIVE: &str = "option_hint_live";
const HINT_ACTIVE: &str = "option_hint_active";
const HINT_VISIBLE: &str = "option_hint_visible";
const HINT_HIDDEN: &str = "option_hint_hidden";

const SCENE_ICON: &str = "layout";
const LIVE_SCENE_ICON: &str = "eye";
const INPUT_ICON: &str = "volume";
const TRANSITION_ICON: &str = "transition-right";
const PROFILE_ICON: &str = "user-cog";
const SCENE_COLLECTION_ICON: &str = "layout-2";

fn listing_failure(error: &ObsError) -> CollectionFailure {
    match error {
        ObsError::Disconnected | ObsError::Connect(_) => CollectionFailure::NotConnected,
        ObsError::Authentication => CollectionFailure::Unauthorized,
        ObsError::Timeout
        | ObsError::Protocol(_)
        | ObsError::Payload(_)
        | ObsError::StudioModeInactive
        | ObsError::Request { .. } => CollectionFailure::Transport,
    }
}

fn named_entries(names: Vec<String>, current: Option<&str>, icon: &str) -> Vec<OptionEntry> {
    names
        .into_iter()
        .map(|name| {
            let is_current = current == Some(name.as_str());
            let entry = OptionEntry::new(name.clone(), name).with_icon(SectionIcon::new(icon));
            if is_current {
                entry.with_hint(OptionHint::new(HINT_ACTIVE)).as_current()
            } else {
                entry
            }
        })
        .collect()
}

pub(crate) fn scene_entries(catalog: &ObsCatalog) -> Vec<OptionEntry> {
    let current = catalog.current_scene.as_deref();
    catalog
        .scenes
        .iter()
        .map(|scene| {
            let entry = OptionEntry::new(scene.clone(), scene.clone());
            if current == Some(scene.as_str()) {
                entry
                    .with_icon(SectionIcon::new(LIVE_SCENE_ICON))
                    .with_hint(OptionHint::new(HINT_LIVE))
                    .as_current()
            } else {
                entry.with_icon(SectionIcon::new(SCENE_ICON))
            }
        })
        .collect()
}

fn requested_scene(depends_on_values: &BTreeMap<String, QuickActionFieldValue>) -> Option<&str> {
    match depends_on_values.get(SCENE_FIELD) {
        Some(QuickActionFieldValue::Text(scene)) if !scene.is_empty() => Some(scene.as_str()),
        _ => None,
    }
}

pub(crate) fn source_entries(
    catalog: &ObsCatalog,
    requested: Option<&str>,
) -> CollectionOutcome<Vec<OptionEntry>> {
    let unknown_scene = |message: &str| CollectionFailure::InvalidInput {
        field: Some(SCENE_FIELD.to_owned()),
        message: message.to_owned(),
    };
    let scene = requested
        .or(catalog.current_scene.as_deref())
        .ok_or_else(|| unknown_scene("no scene to list sources from"))?;
    if !catalog.scenes.iter().any(|known| known == scene) {
        return Err(unknown_scene("the selected scene is not in OBS"));
    }
    Ok(catalog
        .sources
        .get(scene)
        .map(|infos| {
            infos
                .iter()
                .map(|info| {
                    OptionEntry::new(info.name.clone(), info.name.clone())
                        .with_icon(icon_for_kind(info.kind.as_deref()))
                        .with_hint(OptionHint::new(if info.visible {
                            HINT_VISIBLE
                        } else {
                            HINT_HIDDEN
                        }))
                })
                .collect()
        })
        .unwrap_or_default())
}

pub(crate) fn input_entries(catalog: &ObsCatalog) -> Vec<OptionEntry> {
    named_entries(catalog.audio_inputs.clone(), None, INPUT_ICON)
}

impl ObsClient {
    fn read_catalog<T>(&self, read: impl FnOnce(&ObsCatalog) -> T) -> CollectionOutcome<T> {
        if !self.connection_state().is_connected() {
            return Err(CollectionFailure::NotConnected);
        }
        self.catalog_state
            .read()
            .map(|catalog| read(&catalog))
            .map_err(|_| CollectionFailure::Transport)
    }

    async fn transition_entries(&self) -> Result<Vec<OptionEntry>, ObsError> {
        let session = self.active_session().await?;
        let list = session
            .request("GetSceneTransitionList", session.obs().transitions().list())
            .await?;
        let current = list.current_scene_transition.map(|id| id.name);
        let names = list.transitions.into_iter().map(|t| t.id.name).collect();
        Ok(named_entries(names, current.as_deref(), TRANSITION_ICON))
    }

    async fn profile_entries(&self) -> Result<Vec<OptionEntry>, ObsError> {
        let session = self.active_session().await?;
        let list = session
            .request("GetProfileList", session.obs().profiles().list())
            .await?;
        Ok(named_entries(
            list.profiles,
            Some(list.current.as_str()),
            PROFILE_ICON,
        ))
    }

    async fn scene_collection_entries(&self) -> Result<Vec<OptionEntry>, ObsError> {
        let session = self.active_session().await?;
        let list = session
            .request(
                "GetSceneCollectionList",
                session.obs().scene_collections().list(),
            )
            .await?;
        Ok(named_entries(
            list.collections,
            Some(list.current.as_str()),
            SCENE_COLLECTION_ICON,
        ))
    }
}

#[async_trait]
impl BuiltinOptionSource for ObsClient {
    fn option_lists(&self) -> Vec<OptionListMetadata> {
        vec![
            OptionListMetadata::new(OptionListId::new(SCENE_LIST)),
            OptionListMetadata::new(OptionListId::new(SOURCE_LIST)).depending_on(SCENE_FIELD),
            OptionListMetadata::new(OptionListId::new(INPUT_LIST)),
            OptionListMetadata::new(OptionListId::new(TRANSITION_LIST)),
            OptionListMetadata::new(OptionListId::new(PROFILE_LIST)),
            OptionListMetadata::new(OptionListId::new(SCENE_COLLECTION_LIST)),
        ]
    }

    fn revisions(&self) -> CollectionRevisions {
        self.option_revisions.subscribe()
    }

    async fn resolve(
        &self,
        list: &OptionListId,
        depends_on_values: &BTreeMap<String, QuickActionFieldValue>,
    ) -> CollectionOutcome<Vec<OptionEntry>> {
        match list.as_str() {
            SCENE_LIST => self.read_catalog(scene_entries),
            SOURCE_LIST => self.read_catalog(|catalog| {
                source_entries(catalog, requested_scene(depends_on_values))
            })?,
            INPUT_LIST => self.read_catalog(input_entries),
            TRANSITION_LIST => self
                .transition_entries()
                .await
                .map_err(|e| listing_failure(&e)),
            PROFILE_LIST => self
                .profile_entries()
                .await
                .map_err(|e| listing_failure(&e)),
            SCENE_COLLECTION_LIST => self
                .scene_collection_entries()
                .await
                .map_err(|e| listing_failure(&e)),
            _ => Err(CollectionFailure::NotEligible),
        }
    }
}
