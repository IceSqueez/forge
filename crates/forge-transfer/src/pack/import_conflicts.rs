use forge_storage::OverlayId;

use crate::documents::inventory::PackInventory;
use crate::documents::model::{PackDocuments, TriggerDocument};
use crate::pack::import_plan::{PackConflict, PackItemKey, PackItemKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DetectedItem {
    pub(crate) key: PackItemKey,
    pub(crate) name: String,
    pub(crate) conflict: PackConflict,
    pub(crate) archived: bool,
}

impl DetectedItem {
    fn new(kind: PackItemKind, id: String, name: &str, conflict: PackConflict) -> Self {
        Self {
            key: PackItemKey { kind, id },
            name: name.to_owned(),
            conflict,
            archived: false,
        }
    }

    fn archived_local(mut self, archived: bool) -> Self {
        self.archived = archived;
        self
    }
}

fn conflict_of(id_matches: bool, name_match: Option<String>) -> PackConflict {
    match (id_matches, name_match) {
        (true, _) => PackConflict::IdMatch,
        (false, Some(local_id)) => PackConflict::NameMatch { local_id },
        (false, None) => PackConflict::None,
    }
}

pub(crate) fn detect_conflicts(
    documents: &PackDocuments,
    inventory: &PackInventory,
) -> Vec<DetectedItem> {
    let mut detected = Vec::new();
    for media in &documents.media {
        detected.push(DetectedItem::new(
            PackItemKind::Media,
            media.id.clone(),
            &media.label,
            PackConflict::None,
        ));
    }
    for queue in &documents.queues {
        let named = inventory
            .queues
            .values()
            .find(|local| local.name == queue.name)
            .map(|local| local.id.to_string());
        detected.push(DetectedItem::new(
            PackItemKind::Queue,
            queue.id.to_string(),
            &queue.name,
            conflict_of(inventory.queues.contains_key(&queue.id), named),
        ));
    }
    for script in &documents.scripts {
        let by_id = inventory.stored_script(script.id);
        let by_name = inventory.stored_script_named(&script.name);
        let archived = by_id
            .as_ref()
            .or(by_name.as_ref())
            .is_some_and(|stored| stored.archived);
        detected.push(
            DetectedItem::new(
                PackItemKind::Script,
                script.id.to_string(),
                &script.name,
                conflict_of(
                    by_id.is_some(),
                    by_name.map(|stored| stored.row.id.to_string()),
                ),
            )
            .archived_local(archived),
        );
    }
    for global in &documents.globals {
        let archived = inventory.archived_globals.contains_key(&global.name);
        detected.push(
            DetectedItem::new(
                PackItemKind::Global,
                global.name.clone(),
                &global.name,
                conflict_of(
                    archived || inventory.globals.contains_key(&global.name),
                    None,
                ),
            )
            .archived_local(archived),
        );
    }
    for clip in &documents.clips {
        let named = inventory
            .clips
            .values()
            .find(|local| local.name == clip.name)
            .map(|local| local.id.to_string());
        detected.push(DetectedItem::new(
            PackItemKind::Clip,
            clip.id.to_string(),
            &clip.name,
            conflict_of(inventory.clips.contains_key(&clip.id), named),
        ));
    }
    for overlay in &documents.overlays {
        let named = inventory
            .overlays
            .values()
            .find(|local| local.display_name == overlay.display_name)
            .map(|local| local.id.as_str().to_owned());
        let id_matches = inventory
            .overlays
            .contains_key(&OverlayId::new(overlay.id.as_str()));
        detected.push(DetectedItem::new(
            PackItemKind::Overlay,
            overlay.id.clone(),
            &overlay.display_name,
            conflict_of(id_matches, named),
        ));
    }
    for trigger in &documents.triggers {
        let archived = inventory.archived_triggers.contains_key(&trigger.id);
        detected.push(
            DetectedItem::new(
                PackItemKind::Trigger,
                trigger.id.to_string(),
                &trigger.name,
                conflict_of(
                    archived || inventory.triggers.contains_key(&trigger.id),
                    trigger_name_match(trigger, inventory),
                ),
            )
            .archived_local(archived),
        );
    }
    for action in &documents.actions {
        let named = inventory
            .actions
            .values()
            .find(|local| local.name == action.name)
            .map(|local| local.id.to_string());
        let archived = inventory.archived_actions.contains_key(&action.id);
        detected.push(
            DetectedItem::new(
                PackItemKind::Action,
                action.id.to_string(),
                &action.name,
                conflict_of(
                    archived || inventory.actions.contains_key(&action.id),
                    named,
                ),
            )
            .archived_local(archived),
        );
    }
    detected
}

fn trigger_name_match(trigger: &TriggerDocument, inventory: &PackInventory) -> Option<String> {
    inventory
        .triggers
        .values()
        .find(|local| {
            local.kind_id == trigger.kind_id
                && local.user_defined == trigger.user_defined
                && (!trigger.user_defined || local.name == trigger.name)
        })
        .map(|local| local.id.to_string())
}

#[cfg(test)]
mod tests {
    use forge_storage::OverlayId;

    use super::*;
    use crate::documents::model::{
        ActionDocument, ClipDocument, ClipSourceDocument, GlobalDocument, MediaDocument,
        OverlayDocument, QueueDocument, ScriptDocument,
    };
    use crate::test_support::{
        OWNED_TRIGGER, PLAIN_TRIGGER, action, clip, global, no_documents, overlay, queue, script,
        trigger,
    };
    use forge_storage::MediaFormat;
    use forge_types::{QueueId, TriggerInstanceId};

    fn only_item(documents: &PackDocuments, inventory: &PackInventory) -> DetectedItem {
        let mut detected = detect_conflicts(documents, inventory);
        assert_eq!(detected.len(), 1);
        detected.remove(0)
    }

    fn trigger_conflict(
        pack: &forge_types::TriggerInstance,
        inventory: &PackInventory,
    ) -> DetectedItem {
        let mut documents = no_documents();
        documents.triggers.push(TriggerDocument::of(pack));
        only_item(&documents, inventory)
    }

    #[test]
    fn a_queue_conflict_prefers_the_id_over_the_name() {
        let local = queue("Alerts");
        let mut inventory = PackInventory::default();
        inventory.queues.insert(local.id, local.clone());
        let renamed = {
            let mut same_id = local.clone();
            same_id.name = "Renamed".to_owned();
            same_id
        };
        let cases = [
            (local.clone(), PackConflict::IdMatch),
            (renamed, PackConflict::IdMatch),
            (
                queue("Alerts"),
                PackConflict::NameMatch {
                    local_id: local.id.to_string(),
                },
            ),
            (queue("Elsewhere"), PackConflict::None),
        ];
        for (pack, expected) in cases {
            let mut documents = no_documents();
            documents.queues.push(QueueDocument::of(&pack, None));
            assert_eq!(
                only_item(&documents, &inventory).conflict,
                expected,
                "{}",
                pack.name
            );
        }
    }

    #[test]
    fn a_script_conflict_reports_an_archived_local_row() {
        let live = script("live");
        let archived = script("archived");
        let mut inventory = PackInventory::default();
        inventory.scripts.insert(live.name.clone(), live.clone());
        inventory
            .archived_scripts
            .insert(archived.name.clone(), archived.clone());
        let cases = [
            (live.clone(), PackConflict::IdMatch, false),
            (archived.clone(), PackConflict::IdMatch, true),
            (
                script("archived"),
                PackConflict::NameMatch {
                    local_id: archived.id.to_string(),
                },
                true,
            ),
            (script("fresh"), PackConflict::None, false),
        ];
        for (pack, conflict, is_archived) in cases {
            let mut documents = no_documents();
            documents.scripts.push(ScriptDocument::of(&pack));
            let item = only_item(&documents, &inventory);
            assert_eq!((item.conflict, item.archived), (conflict, is_archived));
        }
    }

    #[test]
    fn a_global_conflicts_by_name_alone_and_an_archived_one_says_so() {
        let mut inventory = PackInventory::default();
        inventory.globals.insert("live".to_owned(), global("live"));
        inventory
            .archived_globals
            .insert("old".to_owned(), global("old"));
        for (name, conflict, archived) in [
            ("live", PackConflict::IdMatch, false),
            ("old", PackConflict::IdMatch, true),
            ("fresh", PackConflict::None, false),
        ] {
            let mut documents = no_documents();
            documents.globals.push(GlobalDocument::of(&global(name)));
            let item = only_item(&documents, &inventory);
            assert_eq!(
                (item.conflict, item.archived),
                (conflict, archived),
                "{name}"
            );
        }
    }

    #[test]
    fn a_stock_trigger_matches_by_kind_whatever_its_name_is() {
        let local = trigger(PLAIN_TRIGGER, "Local label", false);
        let mut inventory = PackInventory::default();
        inventory.triggers.insert(local.id, local.clone());
        assert_eq!(
            trigger_conflict(&trigger(PLAIN_TRIGGER, "Pack label", false), &inventory).conflict,
            PackConflict::NameMatch {
                local_id: local.id.to_string()
            }
        );
    }

    #[test]
    fn a_user_defined_trigger_also_needs_the_same_name_to_match() {
        let local = trigger(PLAIN_TRIGGER, "Mine", true);
        let mut inventory = PackInventory::default();
        inventory.triggers.insert(local.id, local.clone());
        assert_eq!(
            trigger_conflict(&trigger(PLAIN_TRIGGER, "Mine", true), &inventory).conflict,
            PackConflict::NameMatch {
                local_id: local.id.to_string()
            }
        );
        assert_eq!(
            trigger_conflict(&trigger(PLAIN_TRIGGER, "Other", true), &inventory).conflict,
            PackConflict::None
        );
    }

    #[test]
    fn triggers_of_a_different_kind_or_authorship_never_match_by_name() {
        let local = trigger(PLAIN_TRIGGER, "Same", false);
        let mut inventory = PackInventory::default();
        inventory.triggers.insert(local.id, local);
        for pack in [
            trigger(OWNED_TRIGGER, "Same", false),
            trigger(PLAIN_TRIGGER, "Same", true),
        ] {
            assert_eq!(
                trigger_conflict(&pack, &inventory).conflict,
                PackConflict::None
            );
        }
    }

    #[test]
    fn an_archived_trigger_with_the_same_id_is_an_id_match() {
        let local = trigger(PLAIN_TRIGGER, "Gone", false);
        let mut inventory = PackInventory::default();
        inventory.archived_triggers.insert(local.id, local.clone());
        let item = trigger_conflict(&local, &inventory);
        assert_eq!(
            (item.conflict, item.archived),
            (PackConflict::IdMatch, true)
        );
    }

    #[test]
    fn an_action_conflict_covers_live_archived_and_named_rows() {
        let queue_id = QueueId::new();
        let live = action("live", queue_id, Vec::new());
        let archived = action("archived", queue_id, Vec::new());
        let mut inventory = PackInventory::default();
        inventory.actions.insert(live.id, live.clone());
        inventory
            .archived_actions
            .insert(archived.id, archived.clone());
        let cases = [
            (live.clone(), PackConflict::IdMatch, false),
            (archived, PackConflict::IdMatch, true),
            (
                action("live", queue_id, Vec::new()),
                PackConflict::NameMatch {
                    local_id: live.id.to_string(),
                },
                false,
            ),
            (
                action("fresh", queue_id, Vec::new()),
                PackConflict::None,
                false,
            ),
        ];
        for (pack, conflict, is_archived) in cases {
            let mut documents = no_documents();
            documents
                .actions
                .push(ActionDocument::of(&pack, Vec::<TriggerInstanceId>::new()));
            let item = only_item(&documents, &inventory);
            assert_eq!((item.conflict, item.archived), (conflict, is_archived));
        }
    }

    #[test]
    fn a_clip_and_an_overlay_conflict_by_id_then_by_name() {
        let local_clip = clip("Ding");
        let local_overlay = overlay("local-overlay", "Alert box");
        let mut inventory = PackInventory::default();
        inventory.clips.insert(local_clip.id, local_clip.clone());
        inventory
            .overlays
            .insert(OverlayId::new("local-overlay"), local_overlay.clone());
        let mut documents = no_documents();
        for pack in [local_clip.clone(), clip("Ding")] {
            documents
                .clips
                .push(ClipDocument::of(&pack, ClipSourceDocument::Missing));
        }
        for pack in [local_overlay, overlay("pack-overlay", "Alert box")] {
            documents
                .overlays
                .push(OverlayDocument::of(&pack, Vec::new()));
        }
        let conflicts: Vec<PackConflict> = detect_conflicts(&documents, &inventory)
            .into_iter()
            .map(|item| item.conflict)
            .collect();
        assert_eq!(
            conflicts,
            [
                PackConflict::IdMatch,
                PackConflict::NameMatch {
                    local_id: local_clip.id.to_string()
                },
                PackConflict::IdMatch,
                PackConflict::NameMatch {
                    local_id: "local-overlay".to_owned()
                },
            ]
        );
    }

    #[test]
    fn media_never_conflicts_because_it_is_content_addressed() {
        let mut documents = no_documents();
        documents.media.push(MediaDocument {
            id: "blob".to_owned(),
            label: "Ding".to_owned(),
            format: MediaFormat::Ogg,
            file: "media/blob".to_owned(),
        });
        assert_eq!(
            only_item(&documents, &PackInventory::default()).conflict,
            PackConflict::None
        );
    }
}
