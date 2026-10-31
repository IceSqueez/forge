use std::fmt;

use forge_types::IntegrationId;

use crate::documents::closure::ClosureRegistries;
use crate::documents::inventory::PackInventory;
use crate::documents::overlay_configs::check_overlay_configs;
use crate::documents::reference::PackReference;
use crate::error::TransferError;
use crate::pack::import_conflicts::detect_conflicts;
use crate::pack::import_review::review_pack;
use crate::pack::reader::StagedPack;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PackItemKind {
    Media,
    Queue,
    Script,
    Global,
    Clip,
    Overlay,
    Trigger,
    Action,
}

impl PackItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Media => "media",
            Self::Queue => "queue",
            Self::Script => "script",
            Self::Global => "global",
            Self::Clip => "sound clip",
            Self::Overlay => "overlay",
            Self::Trigger => "trigger",
            Self::Action => "action",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackItemKey {
    pub kind: PackItemKind,
    pub id: String,
}

impl fmt::Display for PackItemKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} '{}'", self.kind.as_str(), self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackConflict {
    None,
    IdMatch,
    NameMatch { local_id: String },
}

impl PackConflict {
    fn default_resolution(&self) -> Option<PackResolution> {
        match self {
            Self::None => None,
            Self::IdMatch => Some(PackResolution::Skip),
            Self::NameMatch { .. } => Some(PackResolution::KeepBoth),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackResolution {
    Skip,
    Replace,
    KeepBoth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackPlanItem {
    pub key: PackItemKey,
    pub name: String,
    pub conflict: PackConflict,
    pub archived: bool,
    pub resolution: Option<PackResolution>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum PackWarning {
    MissingIntegration { integration: IntegrationId },
    UnknownKind { kind_id: String },
    UnresolvedReference { reference: PackReference },
    ExternalReference { reference: PackReference },
    ClipWithoutSound { clip: String },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExecutableContent {
    Script { name: String },
    HttpStep { action: String, kind_id: String },
    ScriptStep { action: String, kind_id: String },
    OverlaySource { overlay: String, file: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackImportLimit {
    NamesInScriptsAndTextNotFollowed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackImportPlan {
    items: Vec<PackPlanItem>,
    warnings: Vec<PackWarning>,
    executable: Vec<ExecutableContent>,
    limits: Vec<PackImportLimit>,
}

impl PackImportPlan {
    pub fn items(&self) -> &[PackPlanItem] {
        &self.items
    }

    pub fn warnings(&self) -> &[PackWarning] {
        &self.warnings
    }

    pub fn executable_content(&self) -> &[ExecutableContent] {
        &self.executable
    }

    pub fn limits(&self) -> &[PackImportLimit] {
        &self.limits
    }

    pub fn replaces_anything(&self) -> bool {
        self.items
            .iter()
            .any(|item| item.resolution == Some(PackResolution::Replace))
    }

    pub fn set_resolution(
        &mut self,
        key: &PackItemKey,
        resolution: PackResolution,
    ) -> Result<(), TransferError> {
        let item = self
            .items
            .iter_mut()
            .find(|item| &item.key == key)
            .ok_or_else(|| TransferError::UnknownPackItem {
                item: key.to_string(),
            })?;
        if item.conflict == PackConflict::None {
            return Err(TransferError::PackResolutionNotAllowed {
                item: key.to_string(),
            });
        }
        item.resolution = Some(resolution);
        Ok(())
    }
}

pub fn plan_pack_import(
    staged: &StagedPack,
    inventory: &PackInventory,
    registries: ClosureRegistries<'_>,
) -> Result<PackImportPlan, TransferError> {
    check_overlay_configs(&staged.documents().overlays, registries.overlays)?;
    let items = detect_conflicts(staged.documents(), inventory)
        .into_iter()
        .map(|detected| PackPlanItem {
            resolution: detected.conflict.default_resolution(),
            key: detected.key,
            name: detected.name,
            conflict: detected.conflict,
            archived: detected.archived,
        })
        .collect();
    let review = review_pack(staged, registries);
    Ok(PackImportPlan {
        items,
        warnings: review.warnings,
        executable: review.executable,
        limits: vec![PackImportLimit::NamesInScriptsAndTextNotFollowed],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::model::{GlobalDocument, QueueDocument};
    use crate::test_support::{
        FixtureRegistries, blank_manifest, global, no_documents, queue, staged,
    };

    fn plan_with_one_free_and_one_named_and_one_same_id_item() -> (PackImportPlan, Vec<PackItemKey>)
    {
        let named = queue("Named");
        let mut inventory = PackInventory::default();
        inventory.queues.insert(named.id, named.clone());
        inventory
            .globals
            .insert("score".to_owned(), global("score"));
        let mut documents = no_documents();
        documents
            .queues
            .push(QueueDocument::of(&queue("Fresh"), None));
        documents
            .queues
            .push(QueueDocument::of(&queue("Named"), None));
        documents.globals.push(GlobalDocument::of(&global("score")));
        let registries = FixtureRegistries::new();
        let plan = plan_pack_import(
            &staged(blank_manifest(), documents),
            &inventory,
            registries.closure(),
        )
        .unwrap();
        let keys = plan.items().iter().map(|item| item.key.clone()).collect();
        (plan, keys)
    }

    #[test]
    fn a_plan_defaults_to_nothing_destructive() {
        let (plan, _) = plan_with_one_free_and_one_named_and_one_same_id_item();
        let resolutions: Vec<_> = plan.items().iter().map(|item| item.resolution).collect();
        assert_eq!(
            resolutions,
            [
                None,
                Some(PackResolution::KeepBoth),
                Some(PackResolution::Skip),
            ]
        );
        assert!(!plan.replaces_anything());
    }

    #[test]
    fn resolving_a_conflict_to_replace_marks_the_plan_as_replacing() {
        let (mut plan, keys) = plan_with_one_free_and_one_named_and_one_same_id_item();
        plan.set_resolution(&keys[2], PackResolution::Replace)
            .unwrap();
        assert!(plan.replaces_anything());
    }

    #[test]
    fn an_item_without_a_conflict_cannot_take_a_resolution() {
        let (mut plan, keys) = plan_with_one_free_and_one_named_and_one_same_id_item();
        let refused = plan.set_resolution(&keys[0], PackResolution::Replace);
        assert!(matches!(
            refused,
            Err(TransferError::PackResolutionNotAllowed { .. })
        ));
        assert_eq!(plan.items()[0].resolution, None);
    }

    #[test]
    fn a_key_outside_the_plan_is_refused() {
        let (mut plan, _) = plan_with_one_free_and_one_named_and_one_same_id_item();
        let stranger = PackItemKey {
            kind: PackItemKind::Action,
            id: "stranger".to_owned(),
        };
        assert!(matches!(
            plan.set_resolution(&stranger, PackResolution::Skip),
            Err(TransferError::UnknownPackItem { .. })
        ));
    }
}
