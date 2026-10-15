pub const CONNECTION_PREFIX: &str = "obs.connection.";
pub const CONNECTION_CONNECTED: &str = "obs.connection.connected";
pub const CONNECTION_DISCONNECTED: &str = "obs.connection.disconnected";
pub const CONNECTION_AUTH_FAILED: &str = "obs.connection.auth_failed";

pub const SCENE_CHANGED: &str = "obs.scene.changed";
pub const SCENE_PREVIEW_CHANGED: &str = "obs.scene.preview_changed";
pub const SCENE_LIST_CHANGED: &str = "obs.scene.list_changed";
pub const SCENE_CREATED: &str = "obs.scene.created";
pub const SCENE_REMOVED: &str = "obs.scene.removed";
pub const SCENE_RENAMED: &str = "obs.scene.renamed";
pub const SOURCE_VISIBILITY_CHANGED: &str = "obs.source.visibility_changed";
pub const SOURCE_LOCK_CHANGED: &str = "obs.source.lock_changed";
pub const SOURCE_INPUT_CREATED: &str = "obs.source.input_created";
pub const SOURCE_INPUT_REMOVED: &str = "obs.source.input_removed";
pub const SOURCE_INPUT_RENAMED: &str = "obs.source.input_renamed";
pub const SOURCE_SCENE_ITEM_CREATED: &str = "obs.source.scene_item_created";
pub const SOURCE_SCENE_ITEM_REMOVED: &str = "obs.source.scene_item_removed";

pub const CATALOG_CHANGING_KINDS: [&str; 13] = [
    SCENE_CHANGED,
    SCENE_PREVIEW_CHANGED,
    SCENE_LIST_CHANGED,
    SCENE_CREATED,
    SCENE_REMOVED,
    SCENE_RENAMED,
    SOURCE_VISIBILITY_CHANGED,
    SOURCE_LOCK_CHANGED,
    SOURCE_INPUT_CREATED,
    SOURCE_INPUT_REMOVED,
    SOURCE_INPUT_RENAMED,
    SOURCE_SCENE_ITEM_CREATED,
    SOURCE_SCENE_ITEM_REMOVED,
];

pub const NAME_LIST_CHANGING_KINDS: [&str; 9] = [
    SCENE_LIST_CHANGED,
    SCENE_CREATED,
    SCENE_REMOVED,
    SCENE_RENAMED,
    SOURCE_INPUT_CREATED,
    SOURCE_INPUT_REMOVED,
    SOURCE_INPUT_RENAMED,
    SOURCE_SCENE_ITEM_CREATED,
    SOURCE_SCENE_ITEM_REMOVED,
];
