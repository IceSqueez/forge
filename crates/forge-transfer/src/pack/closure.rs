use std::collections::{BTreeSet, VecDeque};

use forge_overlay::{
    MediaSlot, MediaValue, clip_references, image_references, media_slot, read_media_value,
};
use forge_registry::references::LOCAL_FILE_NAMESPACE;
use forge_registry::{
    FormSchemaSource, InternalNamespace, ReferenceCatalog, ReferenceListing, ReferenceNamespace,
    SubActionRegistry, TriggerRegistry, nested_chains,
};
use forge_storage::{MediaBlobId, OverlayId};
use forge_types::{
    ActionId, ClipId, IntegrationId, QueueId, SubActionStep, TriggerInstanceId, Variant,
};

use crate::error::TransferError;
use crate::pack::inventory::PackInventory;
use crate::pack::manifest::PackReference;

#[derive(Clone, Copy)]
pub struct ClosureRegistries<'a> {
    pub sub_actions: &'a SubActionRegistry,
    pub triggers: &'a TriggerRegistry,
    pub schema: Option<&'a dyn FormSchemaSource>,
}

impl<'a> ClosureRegistries<'a> {
    fn catalog(&self) -> ReferenceCatalog<'a> {
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
        for step in steps {
            if let Some(owner) = registry.owning_integration(&step.kind_id) {
                self.selection.required_integrations.insert(owner.clone());
            }
            for chain in nested_chains(step, registry) {
                self.collect_owners(&chain);
            }
        }
    }
}
