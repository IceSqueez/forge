use forge_types::MessageCode;
use std::collections::BTreeMap;

use async_trait::async_trait;
use forge_platform_core::{
    BuiltinOptionSource, CollectionFailure, CollectionOutcome, CollectionRevisions, OptionEntry,
    OptionHint, OptionListId, OptionListMetadata, QuickActionFieldValue, SectionIcon,
};

use crate::catalog::{ObsCatalog, icon_for_kind};
use crate::client::ObsClient;
use crate::error::ObsError;
use crate::vocabulary::messages;
use crate::vocabulary::option_lists::{
    INPUT_LIST, PROFILE_LIST, SCENE_COLLECTION_LIST, SCENE_LIST, SOURCE_LIST, TRANSITION_LIST,
};

pub(crate) const SCENE_FIELD: &str = crate::vocabulary::config_keys::SCENE;

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
    let unknown_scene = |message: MessageCode| CollectionFailure::InvalidInput {
        field: Some(SCENE_FIELD.to_owned()),
        message: message.into(),
    };
    let scene = requested
        .or(catalog.current_scene.as_deref())
        .ok_or_else(|| unknown_scene(messages::NO_SCENE_TO_LIST))?;
    if !catalog.scenes.iter().any(|known| known == scene) {
        return Err(unknown_scene(messages::SCENE_NOT_IN_OBS));
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::source::SourceInfo;

    fn item(name: &str, visible: bool, kind: Option<&str>) -> SourceInfo {
        SourceInfo {
            name: name.to_owned(),
            visible,
            locked: false,
            audio_db: None,
            kind: kind.map(str::to_owned),
        }
    }

    fn fixture() -> ObsCatalog {
        ObsCatalog {
            scenes: vec![
                "Starting".to_owned(),
                "Gameplay".to_owned(),
                "BRB".to_owned(),
            ],
            current_scene: Some("Gameplay".to_owned()),
            current_preview_scene: Some("BRB".to_owned()),
            sources: HashMap::from([
                (
                    "Gameplay".to_owned(),
                    vec![
                        item("Webcam", true, Some("v4l2_input")),
                        item("Game Capture", false, Some("monitor_capture")),
                        item("Alerts", true, Some("browser_source")),
                    ],
                ),
                (
                    "BRB".to_owned(),
                    vec![item("Logo", false, Some("image_source"))],
                ),
            ]),
            audio_inputs: vec!["Mic/Aux".to_owned(), "Desktop Audio".to_owned()],
            revision: 0,
        }
    }

    fn values(entries: &[OptionEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.value.as_str()).collect()
    }

    fn hint_key(entry: &OptionEntry) -> Option<&'static str> {
        entry.hint.as_ref().map(|h| h.key)
    }

    #[test]
    fn scene_entries_keep_the_obs_order() {
        assert_eq!(
            values(&scene_entries(&fixture())),
            ["Starting", "Gameplay", "BRB"]
        );
    }

    #[test]
    fn scene_entries_mark_only_the_program_scene_as_current_and_live() {
        for entry in scene_entries(&fixture()) {
            let live = entry.value == "Gameplay";
            assert_eq!(entry.current, live, "{}: current marker", entry.value);
            assert_eq!(
                hint_key(&entry),
                live.then_some(HINT_LIVE),
                "{}: live hint",
                entry.value
            );
        }
    }

    #[test]
    fn scene_entries_give_the_program_scene_a_distinct_icon() {
        let entries = scene_entries(&fixture());
        let live = entries.iter().find(|e| e.current).unwrap();
        for other in entries.iter().filter(|e| !e.current) {
            assert_ne!(
                other.icon, live.icon,
                "{} shares the live icon",
                other.value
            );
        }
    }

    #[test]
    fn scene_entries_without_a_program_scene_mark_nothing_current() {
        let catalog = ObsCatalog {
            current_scene: None,
            ..fixture()
        };
        assert!(scene_entries(&catalog).iter().all(|e| !e.current));
    }

    #[test]
    fn source_entries_list_the_requested_scene_in_obs_order() {
        let entries = source_entries(&fixture(), Some("Gameplay")).unwrap();
        assert_eq!(values(&entries), ["Webcam", "Game Capture", "Alerts"]);
    }

    #[test]
    fn source_entries_hint_whether_each_item_is_visible() {
        let entries = source_entries(&fixture(), Some("Gameplay")).unwrap();
        let hints: Vec<(&str, Option<&str>)> = entries
            .iter()
            .map(|e| (e.value.as_str(), hint_key(e)))
            .collect();
        assert_eq!(
            hints,
            [
                ("Webcam", Some(HINT_VISIBLE)),
                ("Game Capture", Some(HINT_HIDDEN)),
                ("Alerts", Some(HINT_VISIBLE)),
            ]
        );
    }

    #[test]
    fn source_entries_use_the_same_icon_as_the_catalog_for_each_input_kind() {
        let catalog = fixture();
        let entries = source_entries(&catalog, Some("Gameplay")).unwrap();
        for (entry, info) in entries.iter().zip(&catalog.sources["Gameplay"]) {
            assert_eq!(
                entry.icon,
                Some(icon_for_kind(info.kind.as_deref())),
                "{}",
                entry.value
            );
        }
    }

    #[test]
    fn source_entries_never_mark_an_item_as_current() {
        let entries = source_entries(&fixture(), Some("Gameplay")).unwrap();
        assert!(entries.iter().all(|e| !e.current));
    }

    #[test]
    fn source_entries_without_a_requested_scene_list_the_program_scene() {
        let catalog = fixture();
        assert_eq!(
            source_entries(&catalog, None).unwrap(),
            source_entries(&catalog, Some("Gameplay")).unwrap()
        );
    }

    #[test]
    fn source_entries_for_a_known_scene_without_items_are_empty() {
        assert!(
            source_entries(&fixture(), Some("Starting"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn source_entries_for_an_unknown_scene_fail_on_the_scene_field_with_a_fixed_message() {
        let no_program_scene = ObsCatalog {
            current_scene: None,
            ..fixture()
        };
        let cases = [
            (fixture(), Some("Deleted Scene"), messages::SCENE_NOT_IN_OBS),
            (fixture(), Some("gameplay"), messages::SCENE_NOT_IN_OBS),
            (no_program_scene, None, messages::NO_SCENE_TO_LIST),
        ];
        for (catalog, requested, code) in cases {
            match source_entries(&catalog, requested) {
                Err(CollectionFailure::InvalidInput { field, message }) => {
                    assert_eq!(field.as_deref(), Some(SCENE_FIELD), "{requested:?}");
                    assert_eq!(message.code(), &code, "{requested:?}");
                    assert_eq!(
                        message.args().count(),
                        0,
                        "{requested:?}: message echoes the scene name"
                    );
                }
                other => panic!("{requested:?}: expected InvalidInput, got {other:?}"),
            }
        }
    }

    #[test]
    fn requested_scene_reads_only_a_non_empty_text_scene_value() {
        let cases = [
            (Vec::new(), None),
            (
                vec![(SCENE_FIELD, QuickActionFieldValue::Text(String::new()))],
                None,
            ),
            (
                vec![(SCENE_FIELD, QuickActionFieldValue::Text("BRB".to_owned()))],
                Some("BRB"),
            ),
            (
                vec![(SCENE_FIELD, QuickActionFieldValue::Toggle(true))],
                None,
            ),
            (
                vec![("source", QuickActionFieldValue::Text("BRB".to_owned()))],
                None,
            ),
        ];
        for (pairs, expected) in cases {
            let values: BTreeMap<String, QuickActionFieldValue> =
                pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
            assert_eq!(requested_scene(&values), expected, "{values:?}");
        }
    }

    #[test]
    fn input_entries_keep_the_obs_order_without_a_current_marker() {
        let entries = input_entries(&fixture());
        assert_eq!(values(&entries), ["Mic/Aux", "Desktop Audio"]);
        assert!(entries.iter().all(|e| !e.current && e.hint.is_none()));
    }

    #[test]
    fn named_entries_mark_only_the_reported_current_name_as_active() {
        let names = vec!["Fade".to_owned(), "Cut".to_owned(), "Stinger".to_owned()];
        let entries = named_entries(names, Some("Cut"), TRANSITION_ICON);
        let marked: Vec<(&str, bool, Option<&str>)> = entries
            .iter()
            .map(|e| (e.value.as_str(), e.current, hint_key(e)))
            .collect();
        assert_eq!(
            marked,
            [
                ("Fade", false, None),
                ("Cut", true, Some(HINT_ACTIVE)),
                ("Stinger", false, None),
            ]
        );
    }

    #[test]
    fn named_entries_with_a_current_name_obs_did_not_list_mark_nothing() {
        let names = vec!["Default".to_owned(), "Collab".to_owned()];
        let entries = named_entries(names, Some("Removed"), PROFILE_ICON);
        assert!(entries.iter().all(|e| !e.current));
    }

    #[test]
    fn obs_request_failures_map_to_collection_failures_without_protocol_text() {
        let cases = [
            (ObsError::Disconnected, CollectionFailure::NotConnected),
            (
                ObsError::Connect("connection refused".to_owned()),
                CollectionFailure::NotConnected,
            ),
            (ObsError::Authentication, CollectionFailure::Unauthorized),
            (ObsError::Timeout, CollectionFailure::Transport),
            (
                ObsError::Protocol("unexpected frame".to_owned()),
                CollectionFailure::Transport,
            ),
            (ObsError::StudioModeInactive, CollectionFailure::Transport),
            (
                ObsError::Request {
                    request_type: "GetProfileList".to_owned(),
                    message: "internal".to_owned(),
                },
                CollectionFailure::Transport,
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(listing_failure(&error), expected, "{error:?}");
        }
    }

    fn disconnected() -> ObsClient {
        ObsClient::new_for_test("localhost:4455".to_owned())
    }

    #[tokio::test]
    async fn resolve_on_a_disconnected_client_reports_not_connected_for_every_declared_list() {
        let client = disconnected();
        let picked = BTreeMap::from([(
            SCENE_FIELD.to_owned(),
            QuickActionFieldValue::Text("Gameplay".to_owned()),
        )]);
        for list in client.option_lists() {
            for values in [BTreeMap::new(), picked.clone()] {
                assert_eq!(
                    client.resolve(&list.id, &values).await,
                    Err(CollectionFailure::NotConnected),
                    "{}",
                    list.id.as_str()
                );
            }
        }
    }

    #[tokio::test]
    async fn resolve_on_a_disconnected_client_ignores_a_stale_catalog_mirror() {
        let client = disconnected();
        *client.catalog_state.write().unwrap() = fixture();
        for list in [SCENE_LIST, SOURCE_LIST, INPUT_LIST] {
            assert_eq!(
                client
                    .resolve(&OptionListId::new(list), &BTreeMap::new())
                    .await,
                Err(CollectionFailure::NotConnected),
                "{list}"
            );
        }
    }

    #[tokio::test]
    async fn resolve_rejects_a_list_id_obs_does_not_declare_as_not_eligible() {
        let client = disconnected();
        for list in [
            "obs.filters",
            "OBS.SCENES",
            "obs.scenes ",
            "twitch.rewards",
            "",
        ] {
            assert_eq!(
                client
                    .resolve(&OptionListId::new(list), &BTreeMap::new())
                    .await,
                Err(CollectionFailure::NotEligible),
                "{list:?}"
            );
        }
    }
}
