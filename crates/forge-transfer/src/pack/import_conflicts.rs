use forge_storage::OverlayId;

use crate::pack::documents::{PackDocuments, TriggerDocument};
use crate::pack::import_plan::{PackConflict, PackItemKey, PackItemKind};
use crate::pack::inventory::PackInventory;

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
        let id_matches = inventory
            .scripts
            .values()
            .any(|local| local.id == script.id);
        let named = inventory
            .scripts
            .get(&script.name)
            .map(|local| local.id.to_string());
        detected.push(DetectedItem::new(
            PackItemKind::Script,
            script.id.to_string(),
            &script.name,
            conflict_of(id_matches, named),
        ));
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
