use std::fmt;

use forge_types::IntegrationId;

use crate::error::TransferError;
use crate::pack::closure::ClosureRegistries;
use crate::pack::import_conflicts::detect_conflicts;
use crate::pack::import_review::review_pack;
use crate::pack::inventory::PackInventory;
use crate::pack::manifest::PackReference;
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
) -> PackImportPlan {
    let items = detect_conflicts(staged.documents(), inventory)
        .into_iter()
        .map(|detected| PackPlanItem {
            resolution: detected.conflict.default_resolution(),
            key: detected.key,
            name: detected.name,
            conflict: detected.conflict,
        })
        .collect();
    let review = review_pack(staged, registries);
    PackImportPlan {
        items,
        warnings: review.warnings,
        executable: review.executable,
        limits: vec![PackImportLimit::NamesInScriptsAndTextNotFollowed],
    }
}
