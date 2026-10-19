mod catalog;
mod codec;
mod listing;
mod location;
mod namespace;
mod remap;
mod rewrite;

pub use catalog::{ACTION_QUEUE_KEY, ACTION_STEPS_KEY, ReferenceCatalog, TRIGGER_OVERRIDES_KEY};
pub use codec::{
    CLIP_REFERENCE_PREFIX, IMAGE_REFERENCE_PREFIX, OVERLAY_ICON_OPTIONS_KEY,
    OVERLAY_SOUND_OPTIONS_KEY,
};
pub use listing::{ConfigReference, ReferenceField, ReferenceListing, UnknownKind};
pub use location::{LocationSegment, ReferenceLocation};
pub use namespace::{
    ACTION_OPTIONS_KEY, ExternalNamespace, GLOBAL_OPTIONS_KEY, InternalNamespace,
    LOCAL_FILE_NAMESPACE, MANAGED_MEDIA_NAMESPACE, OVERLAY_OPTIONS_KEY, QUEUE_OPTIONS_KEY,
    ReferenceKey, ReferenceNamespace, ReferenceScope, SCRIPT_OPTIONS_KEY,
    SOUNDBOARD_CLIP_OPTIONS_KEY, TRIGGER_INSTANCE_OPTIONS_KEY,
};
pub use remap::ReferenceRemap;
