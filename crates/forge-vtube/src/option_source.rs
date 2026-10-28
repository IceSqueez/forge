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
use crate::vts::message_type::ITEM_LIST_REQUEST;
use crate::vts::protocol::check_response;

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
            .send_json_request(ITEM_LIST_REQUEST, data)
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use forge_platform_core::{ConnectionState, RevisionWait};
    use futures_util::FutureExt;
    use tokio::sync::mpsc;

    use super::*;
    use crate::client::tests::{FakeVts, MockPublisher, stored_token_creds};
    use crate::content::{ExpressionItem, HotkeyItem, ModelItem};
    use crate::request::PendingRequest;

    const HANG_GUARD: Duration = Duration::from_secs(30);

    fn snapshot() -> ContentSnapshot {
        ContentSnapshot {
            models: vec![
                ModelItem {
                    id: "m-akari".to_owned(),
                    name: "Akari".to_owned(),
                },
                ModelItem {
                    id: "m-hiyori".to_owned(),
                    name: "Hiyori".to_owned(),
                },
            ],
            current_model_id: Some("m-hiyori".to_owned()),
            hotkeys: vec![
                HotkeyItem {
                    id: "h-wave".to_owned(),
                    name: "Wave".to_owned(),
                },
                HotkeyItem {
                    id: "h-spin".to_owned(),
                    name: "Spin".to_owned(),
                },
            ],
            expressions: vec![
                ExpressionItem {
                    name: "Blush".to_owned(),
                    file: "blush.exp3.json".to_owned(),
                    active: true,
                },
                ExpressionItem {
                    name: "Wink".to_owned(),
                    file: "wink.exp3.json".to_owned(),
                    active: false,
                },
                ExpressionItem {
                    name: "Tears".to_owned(),
                    file: "tears.exp3.json".to_owned(),
                    active: true,
                },
            ],
            ..ContentSnapshot::default()
        }
    }

    fn item_list_response() -> Value {
        json!({
            "itemsInSceneCount": 2,
            "availableItemFiles": [
                { "fileName": "hat.png", "type": "PNG", "loadedCount": 1 },
                { "fileName": "confetti.gif", "type": "GIF", "loadedCount": 0 },
                { "fileName": "cake.png", "type": "PNG" },
                { "fileName": "sign.png", "type": "PNG", "loadedCount": 2 }
            ],
            "itemInstancesInScene": [
                { "instanceID": "i-01", "fileName": "hat.png", "order": 1 },
                { "instanceID": "i-02", "fileName": "sign.png", "order": 2 },
                { "instanceID": "i-03", "fileName": "", "order": 3 }
            ]
        })
    }

    type Row = (String, String, bool, Option<&'static str>);

    fn rows_of(entries: &[OptionEntry]) -> Vec<Row> {
        entries
            .iter()
            .map(|e| {
                (
                    e.value.clone(),
                    e.label.clone(),
                    e.current,
                    e.hint.as_ref().map(|h| h.key),
                )
            })
            .collect()
    }

    fn row(value: &str, label: &str, current: bool, hint: Option<&'static str>) -> Row {
        (value.to_owned(), label.to_owned(), current, hint)
    }

    #[test]
    fn model_entries_mark_only_the_loaded_model_as_current_and_loaded() {
        assert_eq!(
            rows_of(&model_entries(&snapshot())),
            [
                row("m-akari", "Akari", false, None),
                row("m-hiyori", "Hiyori", true, Some(HINT_LOADED)),
            ]
        );
    }

    #[test]
    fn model_entries_mark_nothing_while_no_listed_model_is_loaded() {
        for loaded in [None, Some("m-deleted"), Some("")] {
            let snap = ContentSnapshot {
                current_model_id: loaded.map(str::to_owned),
                ..snapshot()
            };
            assert!(
                model_entries(&snap)
                    .iter()
                    .all(|e| !e.current && e.hint.is_none()),
                "{loaded:?}"
            );
        }
    }

    #[test]
    fn hotkey_entries_offer_the_hotkey_id_under_its_name_without_a_marker() {
        assert_eq!(
            rows_of(&hotkey_entries(&snapshot())),
            [
                row("h-wave", "Wave", false, None),
                row("h-spin", "Spin", false, None),
            ]
        );
    }

    #[test]
    fn expression_entries_mark_every_active_expression_as_current_and_active() {
        assert_eq!(
            rows_of(&expression_entries(&snapshot())),
            [
                row("blush.exp3.json", "Blush", true, Some(HINT_ACTIVE)),
                row("wink.exp3.json", "Wink", false, None),
                row("tears.exp3.json", "Tears", true, Some(HINT_ACTIVE)),
            ]
        );
    }

    #[test]
    fn item_files_hint_files_already_in_the_scene_and_never_mark_one_current() {
        assert_eq!(
            rows_of(&item_lists(&item_list_response()).files),
            [
                row("hat.png", "hat.png", false, Some(HINT_LOADED)),
                row("confetti.gif", "confetti.gif", false, None),
                row("cake.png", "cake.png", false, None),
                row("sign.png", "sign.png", false, Some(HINT_LOADED)),
            ]
        );
    }

    #[test]
    fn item_instances_offer_the_instance_id_under_the_file_name() {
        assert_eq!(
            rows_of(&item_lists(&item_list_response()).instances),
            [
                row("i-01", "hat.png", false, None),
                row("i-02", "sign.png", false, None),
                row("i-03", "i-03", false, None),
            ]
        );
    }

    #[test]
    fn item_lists_from_a_response_without_the_arrays_are_empty() {
        for data in [
            json!({}),
            json!({ "itemsInSceneCount": 0 }),
            json!({ "availableItemFiles": null, "itemInstancesInScene": "none" }),
        ] {
            assert_eq!(item_lists(&data), ItemLists::default(), "{data}");
        }
    }

    #[test]
    fn vtube_errors_map_to_collection_failures() {
        let cases = [
            (VTubeError::NotConnected, CollectionFailure::NotConnected),
            (
                VTubeError::Connect("connection refused".to_owned()),
                CollectionFailure::NotConnected,
            ),
            (VTubeError::ApiDisabled, CollectionFailure::NotConnected),
            (VTubeError::TokenDenied, CollectionFailure::Unauthorized),
            (VTubeError::TokenTimeout, CollectionFailure::Unauthorized),
            (VTubeError::TokenRejected, CollectionFailure::Unauthorized),
            (VTubeError::Timeout, CollectionFailure::Transport),
            (
                VTubeError::Request {
                    message: "socket reset".to_owned(),
                },
                CollectionFailure::Transport,
            ),
            (
                VTubeError::Rejected {
                    error_id: 8,
                    message: "no model loaded".to_owned(),
                },
                CollectionFailure::Transport,
            ),
            (
                VTubeError::Json(serde_json::from_str::<Value>("{").unwrap_err()),
                CollectionFailure::Transport,
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(listing_failure(&error), expected, "{error:?}");
        }
    }

    fn client() -> VTubeClient {
        VTubeClient::new_for_test("ws://127.0.0.1:8001/")
    }

    fn connected_client() -> (VTubeClient, mpsc::UnboundedReceiver<PendingRequest>) {
        let client = client();
        let (tx, rx) = mpsc::unbounded_channel();
        *client.req_tx.try_lock().unwrap() = tx;
        client.state.store(ConnectionState::Connected);
        (client, rx)
    }

    async fn resolve(client: &VTubeClient, list: &str) -> CollectionOutcome<Vec<OptionEntry>> {
        client
            .resolve(&OptionListId::new(list), &BTreeMap::new())
            .await
    }

    #[tokio::test]
    async fn resolve_on_a_disconnected_client_reports_not_connected_for_every_declared_list() {
        let client = client();
        for list in client.option_lists() {
            assert_eq!(
                client.resolve(&list.id, &BTreeMap::new()).await,
                Err(CollectionFailure::NotConnected),
                "{}",
                list.id.as_str()
            );
        }
    }

    #[tokio::test]
    async fn resolve_on_a_disconnected_client_ignores_a_stale_mirror() {
        let client = client();
        *client.content_state.write().unwrap() = snapshot();
        for list in [MODEL_LIST, HOTKEY_LIST, EXPRESSION_LIST] {
            assert_eq!(
                resolve(&client, list).await,
                Err(CollectionFailure::NotConnected),
                "{list}"
            );
        }
    }

    #[tokio::test]
    async fn resolve_rejects_a_list_id_vtube_does_not_declare_as_not_eligible() {
        let (client, _requests) = connected_client();
        for list in [
            "vtube.items",
            "VTUBE.MODELS",
            "vtube.models ",
            "obs.scenes",
            "",
        ] {
            assert_eq!(
                resolve(&client, list).await,
                Err(CollectionFailure::NotEligible),
                "{list:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_connected_client_resolves_mirror_lists_from_the_content_snapshot() {
        let (client, _requests) = connected_client();
        *client.content_state.write().unwrap() = snapshot();
        let snap = snapshot();
        for (list, expected) in [
            (MODEL_LIST, model_entries(&snap)),
            (HOTKEY_LIST, hotkey_entries(&snap)),
            (EXPRESSION_LIST, expression_entries(&snap)),
        ] {
            assert_eq!(resolve(&client, list).await, Ok(expected), "{list}");
        }
    }

    fn answer_item_lists(
        mut requests: mpsc::UnboundedReceiver<PendingRequest>,
        reply: Value,
    ) -> mpsc::UnboundedReceiver<Value> {
        let (seen_tx, seen) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(request) = requests.recv().await {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the receiver may already be gone"
                )]
                let _ = seen_tx.send(serde_json::from_str(&request.payload).unwrap());
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the receiver may already be gone"
                )]
                let _ = request.respond_to.send(reply.clone());
            }
        });
        seen
    }

    #[tokio::test]
    async fn an_item_list_resolve_sends_one_request_asking_for_files_and_instances() {
        for list in [ITEM_FILE_LIST, ITEM_INSTANCE_LIST] {
            let (client, requests) = connected_client();
            let mut seen = answer_item_lists(requests, item_list_response());

            resolve(&client, list).await.unwrap();
            drop(client);

            let request = seen.recv().await.unwrap();
            assert_eq!(
                (&request["messageType"], &request["data"]),
                (
                    &json!("ItemListRequest"),
                    &json!({
                        "includeAvailableSpots": false,
                        "includeItemInstancesInScene": true,
                        "includeAvailableItemFiles": true,
                    })
                ),
                "{list}"
            );
            assert!(seen.recv().await.is_none(), "{list}: a second request");
        }
    }

    #[tokio::test]
    async fn one_item_list_response_feeds_both_the_file_and_the_instance_list() {
        let (client, requests) = connected_client();
        let _seen = answer_item_lists(requests, item_list_response());
        let values = |entries: Vec<OptionEntry>| -> Vec<String> {
            entries.into_iter().map(|e| e.value).collect()
        };

        let files = values(resolve(&client, ITEM_FILE_LIST).await.unwrap());
        let instances = values(resolve(&client, ITEM_INSTANCE_LIST).await.unwrap());

        assert_eq!(
            (files, instances),
            (
                vec![
                    "hat.png".to_owned(),
                    "confetti.gif".to_owned(),
                    "cake.png".to_owned(),
                    "sign.png".to_owned(),
                ],
                vec!["i-01".to_owned(), "i-02".to_owned(), "i-03".to_owned()],
            )
        );
    }

    #[tokio::test]
    async fn an_item_list_vtube_studio_rejects_is_a_transport_failure() {
        for list in [ITEM_FILE_LIST, ITEM_INSTANCE_LIST] {
            let (client, requests) = connected_client();
            let _seen = answer_item_lists(
                requests,
                json!({ "errorID": 8, "message": "item list unavailable" }),
            );
            assert_eq!(
                resolve(&client, list).await,
                Err(CollectionFailure::Transport),
                "{list}"
            );
        }
    }

    #[tokio::test]
    async fn an_item_list_whose_request_channel_is_gone_reports_not_connected() {
        let (client, requests) = connected_client();
        drop(requests);
        for list in [ITEM_FILE_LIST, ITEM_INSTANCE_LIST] {
            assert_eq!(
                resolve(&client, list).await,
                Err(CollectionFailure::NotConnected),
                "{list}"
            );
        }
    }

    async fn bumped(revisions: &mut CollectionRevisions) -> bool {
        tokio::time::timeout(HANG_GUARD, revisions.changed())
            .await
            .is_ok_and(|wait| wait == RevisionWait::Changed)
    }

    #[tokio::test]
    async fn a_session_bumps_the_option_revision_on_connect_item_spawn_and_loss() {
        let mut fake = FakeVts::bind().await;
        let publisher = MockPublisher::new();
        let client = fake.connect(&publisher, &stored_token_creds());
        let mut revisions = client.revisions();
        let (peer, _) = fake.logged_in_conn().await.answer_all_except(|_| false);

        assert!(bumped(&mut revisions).await, "connect");

        peer.event(
            "ItemEvent",
            json!({
                "itemEventType": "Added",
                "itemInstanceID": "i-09",
                "itemFileName": "hat.png",
                "itemPosition": { "x": 0.0, "y": 0.0 }
            }),
        );
        assert!(bumped(&mut revisions).await, "item added");

        peer.close();
        assert!(bumped(&mut revisions).await, "session lost");
    }

    #[test]
    fn bump_options_moves_the_option_revision_but_not_the_catalog_signal() {
        let client = client();
        let mut catalog = client.catalog_changes();
        let mut revisions = client.revisions();

        client.catalog_signal().bump_options();

        assert_eq!(
            (
                catalog.changed().now_or_never(),
                revisions.changed().now_or_never()
            ),
            (None, Some(RevisionWait::Changed))
        );
    }

    #[test]
    fn bump_moves_both_the_catalog_signal_and_the_option_revision() {
        let client = client();
        let mut catalog = client.catalog_changes();
        let mut revisions = client.revisions();

        client.catalog_signal().bump();

        assert_eq!(
            (
                catalog.changed().now_or_never(),
                revisions.changed().now_or_never()
            ),
            (Some(true), Some(RevisionWait::Changed))
        );
    }
}
