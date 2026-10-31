use std::collections::{BTreeSet, VecDeque};

use forge_overlay::{
    MediaSlot, MediaValue, OverlayKindRegistry, clip_references, image_references, media_slot,
    read_media_value,
};
use forge_registry::references::LOCAL_FILE_NAMESPACE;
use forge_registry::{
    FormSchemaSource, InternalNamespace, ReferenceCatalog, ReferenceListing, ReferenceNamespace,
    StepNode, StepReach, SubActionRegistry, TriggerRegistry, walk_steps,
};
use forge_storage::{MediaBlobId, OverlayId};
use forge_types::{
    ActionId, ClipId, IntegrationId, QueueId, SubActionStep, TriggerInstanceId, Variant,
};

use crate::documents::inventory::PackInventory;
use crate::documents::reference::PackReference;
use crate::error::TransferError;

#[derive(Clone, Copy)]
pub struct ClosureRegistries<'a> {
    pub sub_actions: &'a SubActionRegistry,
    pub triggers: &'a TriggerRegistry,
    pub overlays: &'a OverlayKindRegistry,
    pub schema: Option<&'a (dyn FormSchemaSource + Sync)>,
}

impl<'a> ClosureRegistries<'a> {
    pub(crate) fn catalog(&self) -> ReferenceCatalog<'a> {
        let catalog = ReferenceCatalog::new(self.sub_actions, self.triggers);
        match self.schema {
            Some(schema) => catalog.with_schema(schema),
            None => catalog,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackSelection {
    pub roots: Vec<ActionId>,
    pub actions: BTreeSet<ActionId>,
    pub triggers: BTreeSet<TriggerInstanceId>,
    pub queues: BTreeSet<QueueId>,
    pub scripts: BTreeSet<String>,
    pub globals: BTreeSet<String>,
    pub clips: BTreeSet<ClipId>,
    pub overlays: BTreeSet<OverlayId>,
    pub media: BTreeSet<MediaBlobId>,
    pub required_integrations: BTreeSet<IntegrationId>,
    pub external_references: BTreeSet<PackReference>,
    pub unresolved_references: BTreeSet<PackReference>,
    pub unknown_kinds: BTreeSet<String>,
}

pub fn resolve_pack_closure(
    inventory: &PackInventory,
    registries: ClosureRegistries<'_>,
    roots: &[ActionId],
) -> Result<PackSelection, TransferError> {
    if roots.is_empty() {
        return Err(TransferError::EmptyPack);
    }
    let mut walk = ClosureWalk {
        inventory,
        registries,
        catalog: registries.catalog(),
        selection: PackSelection {
            roots: roots.to_vec(),
            ..PackSelection::default()
        },
        pending: VecDeque::new(),
    };
    for root in roots {
        walk.push(InternalNamespace::Action, root.to_string());
        let linked = inventory.action_triggers.get(root).into_iter().flatten();
        for trigger in linked {
            walk.push(InternalNamespace::TriggerInstance, trigger.to_string());
        }
    }
    walk.run();
    if walk.selection.actions.is_empty() {
        return Err(TransferError::EmptyPack);
    }
    Ok(walk.selection)
}

struct ClosureWalk<'i, 'r> {
    inventory: &'i PackInventory,
    registries: ClosureRegistries<'r>,
    catalog: ReferenceCatalog<'r>,
    selection: PackSelection,
    pending: VecDeque<(InternalNamespace, String)>,
}

impl ClosureWalk<'_, '_> {
    fn push(&mut self, namespace: InternalNamespace, value: String) {
        self.pending.push_back((namespace, value));
    }

    fn run(&mut self) {
        while let Some((namespace, value)) = self.pending.pop_front() {
            if !self.visit(namespace, &value) {
                self.selection.unresolved_references.insert(PackReference {
                    namespace: namespace.as_str().to_owned(),
                    value,
                });
            }
        }
    }

    fn visit(&mut self, namespace: InternalNamespace, value: &str) -> bool {
        match namespace {
            InternalNamespace::Action => self.visit_action(value),
            InternalNamespace::TriggerInstance => self.visit_trigger(value),
            InternalNamespace::Queue => {
                let id = value
                    .parse::<QueueId>()
                    .ok()
                    .filter(|id| self.inventory.queues.contains_key(id));
                id.map(|id| self.selection.queues.insert(id)).is_some()
            }
            InternalNamespace::Script => {
                let known = self.inventory.scripts.contains_key(value);
                if known {
                    self.selection.scripts.insert(value.to_owned());
                }
                known
            }
            InternalNamespace::Global => {
                let known = self.inventory.globals.contains_key(value);
                if known {
                    self.selection.globals.insert(value.to_owned());
                }
                known
            }
            InternalNamespace::SoundboardClip => self.visit_clip(value),
            InternalNamespace::Overlay => self.visit_overlay(value),
            InternalNamespace::ManagedMedia => {
                let id = MediaBlobId::from_stored(value)
                    .ok()
                    .filter(|id| self.inventory.media.contains_key(id));
                id.map(|id| self.selection.media.insert(id)).is_some()
            }
        }
    }

    fn visit_action(&mut self, value: &str) -> bool {
        let inventory = self.inventory;
        let Some(action) = value
            .parse::<ActionId>()
            .ok()
            .and_then(|id| inventory.actions.get(&id))
        else {
            return false;
        };
        if self.selection.actions.insert(action.id) {
            self.collect_owners(&action.sub_actions);
            let listing = self.catalog.action_references(action);
            self.absorb(listing);
        }
        true
    }

    fn visit_trigger(&mut self, value: &str) -> bool {
        let inventory = self.inventory;
        let Some(trigger) = value
            .parse::<TriggerInstanceId>()
            .ok()
            .and_then(|id| inventory.triggers.get(&id))
        else {
            return false;
        };
        if self.selection.triggers.insert(trigger.id) {
            if let Some(owner) = self
                .registries
                .triggers
                .owning_integration(&trigger.kind_id)
            {
                self.selection.required_integrations.insert(owner.clone());
            }
            let listing = self.catalog.trigger_instance_references(trigger);
            self.absorb(listing);
        }
        true
    }

    fn visit_clip(&mut self, value: &str) -> bool {
        let Some(id) = value
            .parse::<ClipId>()
            .ok()
            .filter(|id| self.inventory.clips.contains_key(id))
        else {
            return false;
        };
        if self.selection.clips.insert(id)
            && let Some(blob) = self.inventory.clip_media.get(&id)
        {
            self.push(InternalNamespace::ManagedMedia, blob.as_str().to_owned());
        }
        true
    }

    fn visit_overlay(&mut self, value: &str) -> bool {
        let inventory = self.inventory;
        let Some(overlay) = inventory.overlays.get(&OverlayId::new(value)) else {
            return false;
        };
        if !self.selection.overlays.insert(overlay.id.clone()) {
            return true;
        }
        for reference in clip_references(&overlay.config) {
            self.push(InternalNamespace::SoundboardClip, reference.clip);
        }
        for reference in image_references(&overlay.config) {
            self.push(InternalNamespace::ManagedMedia, reference.image);
        }
        for (key, stored) in &overlay.config {
            let Variant::String(stored) = stored else {
                continue;
            };
            if media_slot(key) == Some(MediaSlot::Sound)
                && let MediaValue::File(path) = read_media_value(stored)
            {
                self.selection.external_references.insert(PackReference {
                    namespace: LOCAL_FILE_NAMESPACE.to_owned(),
                    value: path.to_owned(),
                });
            }
        }
        true
    }

    fn absorb(&mut self, listing: ReferenceListing) {
        for reference in listing.references {
            if reference.templated {
                continue;
            }
            match reference.namespace {
                ReferenceNamespace::Internal(namespace) => self.push(namespace, reference.value),
                ReferenceNamespace::External(namespace) => {
                    self.selection.external_references.insert(PackReference {
                        namespace: namespace.as_str().to_owned(),
                        value: reference.value,
                    });
                }
            }
        }
        for unknown in listing.unknown_kinds {
            self.selection.unknown_kinds.insert(unknown.kind_id);
        }
    }

    fn collect_owners(&mut self, steps: &[SubActionStep]) {
        let registry = self.registries.sub_actions;
        let required = &mut self.selection.required_integrations;
        walk_steps(steps, registry, StepReach::All, &mut |node| {
            if let StepNode::Step(step) = node
                && let Some(owner) = registry.owning_integration(&step.kind_id)
            {
                required.insert(owner.clone());
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use forge_types::{Action, Queue};

    use super::*;
    use crate::test_support::{
        ACTION_KEY, CLIP_KEY, FixtureRegistries, GLOBAL_KEY, MEDIA_KEY, OWNED_STEP, OWNED_TRIGGER,
        PLAIN_STEP, PLAIN_TRIGGER, SCENE_KEY, SCENE_NAMESPACE, SCRIPT_KEY, STEP_OWNER,
        TRIGGER_OWNER, action, clip, global, queue, script, step, trigger,
    };

    const BLOB_ID: &str = "sha256-0000000000000000000000000000000000000000000000000000000000000000";

    struct World {
        inventory: PackInventory,
        queue: Queue,
    }

    impl World {
        fn new() -> Self {
            let queue = queue("Default");
            let mut inventory = PackInventory::default();
            inventory.queues.insert(queue.id, queue.clone());
            Self { inventory, queue }
        }

        fn action(&mut self, name: &str, steps: Vec<SubActionStep>) -> Action {
            let action = action(name, self.queue.id, steps);
            self.inventory.actions.insert(action.id, action.clone());
            action
        }

        fn close_over(&self, roots: &[ActionId]) -> Result<PackSelection, TransferError> {
            resolve_pack_closure(&self.inventory, FixtureRegistries::new().closure(), roots)
        }
    }

    fn unresolved(namespace: InternalNamespace, value: &str) -> PackReference {
        PackReference {
            namespace: namespace.as_str().to_owned(),
            value: value.to_owned(),
        }
    }

    #[test]
    fn no_roots_is_an_empty_pack() {
        assert!(matches!(
            World::new().close_over(&[]),
            Err(TransferError::EmptyPack)
        ));
    }

    #[test]
    fn roots_that_no_longer_exist_are_an_empty_pack() {
        assert!(matches!(
            World::new().close_over(&[ActionId::new()]),
            Err(TransferError::EmptyPack)
        ));
    }

    #[test]
    fn a_root_brings_its_queue_and_only_its_own_triggers() {
        let mut world = World::new();
        let root = world.action("Root", Vec::new());
        let other = world.action("Other", Vec::new());
        let linked = trigger(PLAIN_TRIGGER, "Linked", false);
        let foreign = trigger(PLAIN_TRIGGER, "Foreign", false);
        for (owner, instance) in [(root.id, &linked), (other.id, &foreign)] {
            world
                .inventory
                .triggers
                .insert(instance.id, instance.clone());
            world
                .inventory
                .action_triggers
                .insert(owner, vec![instance.id]);
        }
        let selection = world.close_over(&[root.id]).unwrap();
        assert_eq!(
            (selection.actions, selection.triggers, selection.queues),
            (
                BTreeSet::from([root.id]),
                BTreeSet::from([linked.id]),
                BTreeSet::from([world.queue.id]),
            )
        );
    }

    #[test]
    fn a_called_action_is_followed_but_its_triggers_stay_behind() {
        let mut world = World::new();
        let called = world.action("Called", Vec::new());
        let linked = trigger(PLAIN_TRIGGER, "Linked", false);
        world.inventory.triggers.insert(linked.id, linked.clone());
        world
            .inventory
            .action_triggers
            .insert(called.id, vec![linked.id]);
        let called_id = called.id.to_string();
        let root = world.action(
            "Root",
            vec![step(PLAIN_STEP, &[(ACTION_KEY, called_id.as_str())])],
        );
        let selection = world.close_over(&[root.id]).unwrap();
        assert_eq!(selection.actions, BTreeSet::from([root.id, called.id]));
        assert!(selection.triggers.is_empty());
    }

    #[test]
    fn actions_that_call_each_other_terminate() {
        let mut world = World::new();
        let first = world.action("First", Vec::new());
        let first_id = first.id.to_string();
        let second = world.action(
            "Second",
            vec![step(PLAIN_STEP, &[(ACTION_KEY, first_id.as_str())])],
        );
        let second_id = second.id.to_string();
        world
            .inventory
            .actions
            .get_mut(&first.id)
            .unwrap()
            .sub_actions = vec![step(PLAIN_STEP, &[(ACTION_KEY, second_id.as_str())])];
        assert_eq!(
            world.close_over(&[first.id]).unwrap().actions,
            BTreeSet::from([first.id, second.id])
        );
    }

    #[test]
    fn step_references_land_in_their_selection_sets() {
        let mut world = World::new();
        world
            .inventory
            .scripts
            .insert("greet".to_owned(), script("greet"));
        world
            .inventory
            .globals
            .insert("score".to_owned(), global("score"));
        let root = world.action(
            "Root",
            vec![step(
                PLAIN_STEP,
                &[
                    (SCRIPT_KEY, "greet"),
                    (GLOBAL_KEY, "score"),
                    (SCENE_KEY, "Live"),
                ],
            )],
        );
        let selection = world.close_over(&[root.id]).unwrap();
        assert_eq!(
            (
                selection.scripts,
                selection.globals,
                selection.external_references,
                selection.unresolved_references,
            ),
            (
                BTreeSet::from(["greet".to_owned()]),
                BTreeSet::from(["score".to_owned()]),
                BTreeSet::from([PackReference {
                    namespace: SCENE_NAMESPACE.to_owned(),
                    value: "Live".to_owned(),
                }]),
                BTreeSet::new(),
            )
        );
    }

    #[test]
    fn a_dangling_or_archived_reference_is_recorded_not_fatal() {
        let mut world = World::new();
        world
            .inventory
            .archived_scripts
            .insert("old".to_owned(), script("old"));
        let dead_clip = ClipId::new().to_string();
        let root = world.action(
            "Root",
            vec![step(
                PLAIN_STEP,
                &[
                    (SCRIPT_KEY, "old"),
                    (GLOBAL_KEY, "absent"),
                    (ACTION_KEY, "not-an-id"),
                    (CLIP_KEY, dead_clip.as_str()),
                    (MEDIA_KEY, "not-a-blob"),
                ],
            )],
        );
        assert_eq!(
            world.close_over(&[root.id]).unwrap().unresolved_references,
            BTreeSet::from([
                unresolved(InternalNamespace::Script, "old"),
                unresolved(InternalNamespace::Global, "absent"),
                unresolved(InternalNamespace::Action, "not-an-id"),
                unresolved(InternalNamespace::SoundboardClip, &dead_clip),
                unresolved(InternalNamespace::ManagedMedia, "not-a-blob"),
            ])
        );
    }

    #[test]
    fn a_templated_reference_is_left_for_run_time() {
        let mut world = World::new();
        let root = world.action(
            "Root",
            vec![step(PLAIN_STEP, &[(SCRIPT_KEY, "%chosen_script%")])],
        );
        let selection = world.close_over(&[root.id]).unwrap();
        assert_eq!(
            (selection.scripts, selection.unresolved_references),
            (BTreeSet::new(), BTreeSet::new())
        );
    }

    #[test]
    fn a_clip_brings_its_managed_sound() {
        let mut world = World::new();
        let local = clip("Ding");
        let blob = MediaBlobId::from_stored(BLOB_ID).unwrap();
        world.inventory.clips.insert(local.id, local.clone());
        world.inventory.clip_media.insert(local.id, blob.clone());
        world.inventory.media.insert(
            blob.clone(),
            forge_storage::MediaBlob {
                id: blob.clone(),
                format: forge_storage::MediaFormat::Ogg,
                byte_size: 0,
                label: String::new(),
                imported_at: time::OffsetDateTime::UNIX_EPOCH,
            },
        );
        let clip_id = local.id.to_string();
        let root = world.action(
            "Root",
            vec![step(PLAIN_STEP, &[(CLIP_KEY, clip_id.as_str())])],
        );
        let selection = world.close_over(&[root.id]).unwrap();
        assert_eq!(
            (selection.clips, selection.media),
            (BTreeSet::from([local.id]), BTreeSet::from([blob]))
        );
    }

    #[test]
    fn integrations_come_from_the_owners_of_used_steps_and_triggers() {
        let mut world = World::new();
        let root = world.action("Root", vec![step(OWNED_STEP, &[]), step(PLAIN_STEP, &[])]);
        let linked = trigger(OWNED_TRIGGER, "Linked", false);
        world.inventory.triggers.insert(linked.id, linked.clone());
        world
            .inventory
            .action_triggers
            .insert(root.id, vec![linked.id]);
        assert_eq!(
            world.close_over(&[root.id]).unwrap().required_integrations,
            BTreeSet::from([
                IntegrationId::new(STEP_OWNER),
                IntegrationId::new(TRIGGER_OWNER),
            ])
        );
    }

    #[test]
    fn an_unregistered_step_kind_is_carried_as_unknown() {
        let mut world = World::new();
        let root = world.action("Root", vec![step("fixture.unknown", &[])]);
        assert_eq!(
            world.close_over(&[root.id]).unwrap().unknown_kinds,
            BTreeSet::from(["fixture.unknown".to_owned()])
        );
    }
}
