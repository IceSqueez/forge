mod closure;
mod documents;
mod inventory;
pub(crate) mod layout;
mod manifest;
mod reader;
mod writer;

pub use closure::{ClosureRegistries, PackSelection, resolve_pack_closure};
pub use documents::{
    ActionDocument, ClipDocument, ClipSourceDocument, DocumentConfig, GlobalDocument,
    MediaDocument, OverlayDocument, OverlayFileDocument, PACK_DOCUMENTS_VERSION, PackDocuments,
    QueueDocument, ScriptDocument, StepDocument, TriggerDocument,
};
pub use inventory::{PackInventory, load_pack_inventory};
pub use layout::PACK_ROOT;
pub use manifest::{PackManifest, PackReference};
pub use reader::{
    PACK_STAGING_DIR_NAME, StagedPack, discard_staged_pack, pack_staging_root, stage_pack,
};
pub use writer::{PackExportRequest, PackMetadata, write_pack};
