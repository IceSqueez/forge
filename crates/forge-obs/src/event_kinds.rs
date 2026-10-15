pub const CONNECTION_PREFIX: &str = "obs.connection.";
pub const AUDIO_PREFIX: &str = "obs.audio.";
pub const COLLECTION_PREFIX: &str = "obs.collection.";
pub const FILTER_PREFIX: &str = "obs.filter.";
pub const PROFILE_PREFIX: &str = "obs.profile.";
pub const RECORDING_PREFIX: &str = "obs.recording.";
pub const SCENE_PREFIX: &str = "obs.scene.";
pub const SOURCE_PREFIX: &str = "obs.source.";
pub const STREAMING_PREFIX: &str = "obs.streaming.";
pub const STUDIO_PREFIX: &str = "obs.studio.";
pub const TRANSITION_PREFIX: &str = "obs.transition.";
pub const VIRTUALCAM_PREFIX: &str = "obs.virtualcam.";
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
pub const RECORDING_STARTED: &str = "obs.recording.started";
pub const RECORDING_STOPPED: &str = "obs.recording.stopped";
pub const RECORDING_STARTING: &str = "obs.recording.starting";
pub const RECORDING_STOPPING: &str = "obs.recording.stopping";
pub const RECORDING_PAUSED: &str = "obs.recording.paused";
pub const RECORDING_RESUMED: &str = "obs.recording.resumed";
pub const VIRTUALCAM_STARTING: &str = "obs.virtualcam.starting";
pub const VIRTUALCAM_STARTED: &str = "obs.virtualcam.started";
pub const VIRTUALCAM_STOPPING: &str = "obs.virtualcam.stopping";
pub const VIRTUALCAM_STOPPED: &str = "obs.virtualcam.stopped";
pub const STREAMING_STARTING: &str = "obs.streaming.starting";
pub const STREAMING_STARTED: &str = "obs.streaming.started";
pub const STREAMING_STOPPING: &str = "obs.streaming.stopping";
pub const STREAMING_STOPPED: &str = "obs.streaming.stopped";
pub const STREAMING_RECONNECTING: &str = "obs.streaming.reconnecting";
pub const STREAMING_RECONNECTED: &str = "obs.streaming.reconnected";
pub const PROFILE_CURRENT_CHANGED: &str = "obs.profile.current_changed";
pub const PROFILE_LIST_CHANGED: &str = "obs.profile.list_changed";
pub const COLLECTION_LIST_CHANGED: &str = "obs.collection.list_changed";
pub const COLLECTION_CHANGING: &str = "obs.collection.changing";
pub const COLLECTION_CHANGED: &str = "obs.collection.changed";
pub const RECORDING_FILE_CHANGED: &str = "obs.recording.file_changed";
pub const STUDIO_ENABLED: &str = "obs.studio.enabled";
pub const STUDIO_DISABLED: &str = "obs.studio.disabled";
pub const TRANSITION_STARTED: &str = "obs.transition.started";
pub const TRANSITION_ENDED: &str = "obs.transition.ended";
pub const TRANSITION_VIDEO_ENDED: &str = "obs.transition.video_ended";
pub const AUDIO_SOURCE_MUTE_CHANGED: &str = "obs.audio.source_mute_changed";
pub const AUDIO_SOURCE_VOLUME_CHANGED: &str = "obs.audio.source_volume_changed";
pub const AUDIO_SOURCE_BALANCE_CHANGED: &str = "obs.audio.source_balance_changed";
pub const AUDIO_SOURCE_SYNC_OFFSET_CHANGED: &str = "obs.audio.source_sync_offset_changed";
pub const FILTER_CREATED: &str = "obs.filter.created";
pub const FILTER_REMOVED: &str = "obs.filter.removed";
pub const FILTER_ENABLED_CHANGED: &str = "obs.filter.enabled_changed";

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
