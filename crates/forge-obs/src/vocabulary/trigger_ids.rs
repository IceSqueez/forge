use super::event_kinds;

pub const CONNECTION_CONNECTED: &str = event_kinds::CONNECTION_CONNECTED;
pub const CONNECTION_DISCONNECTED: &str = event_kinds::CONNECTION_DISCONNECTED;
pub const CONNECTION_AUTH_FAILED: &str = event_kinds::CONNECTION_AUTH_FAILED;

pub const SCENE_CHANGED: &str = "obs.scenes.current_changed";
pub const SCENE_PREVIEW_CHANGED: &str = "obs.scenes.preview_changed";
pub const SCENE_LIST_CHANGED: &str = "obs.scenes.list_changed";
pub const SCENE_CREATED: &str = "obs.scenes.created";
pub const SCENE_REMOVED: &str = "obs.scenes.removed";
pub const SCENE_RENAMED: &str = "obs.scenes.renamed";

pub const SOURCE_INPUT_CREATED: &str = "obs.sources.input_created";
pub const SOURCE_INPUT_REMOVED: &str = "obs.sources.input_removed";
pub const SOURCE_INPUT_RENAMED: &str = "obs.sources.input_renamed";
pub const SOURCE_SCENE_ITEM_CREATED: &str = "obs.sources.scene_item_created";
pub const SOURCE_SCENE_ITEM_REMOVED: &str = "obs.sources.scene_item_removed";
pub const SOURCE_LOCK_CHANGED: &str = "obs.sources.scene_item_lock_changed";
pub const SOURCE_VISIBILITY_CHANGED: &str = "obs.sources.scene_item_visibility_changed";

pub const FILTER_CREATED: &str = "obs.filters.created";
pub const FILTER_REMOVED: &str = "obs.filters.removed";
pub const FILTER_ENABLED_CHANGED: &str = "obs.filters.enabled_changed";

pub const RECORDING_STARTING: &str = "obs.record.starting";
pub const RECORDING_STARTED: &str = "obs.record.started";
pub const RECORDING_STOPPING: &str = "obs.record.stopping";
pub const RECORDING_STOPPED: &str = "obs.record.stopped";
pub const RECORDING_PAUSED: &str = "obs.record.paused";
pub const RECORDING_RESUMED: &str = "obs.record.resumed";
pub const RECORDING_FILE_CHANGED: &str = "obs.record.file_changed";
pub const RECORDING_STATUS_CHANGED: &str = "obs.record.status_changed";

pub const STREAMING_STARTING: &str = "obs.stream.starting";
pub const STREAMING_STARTED: &str = "obs.stream.started";
pub const STREAMING_STOPPING: &str = "obs.stream.stopping";
pub const STREAMING_STOPPED: &str = "obs.stream.stopped";
pub const STREAMING_STATUS_CHANGED: &str = "obs.stream.status_changed";

pub const COLLECTION_CHANGED: &str = "obs.collection.current_changed";
pub const COLLECTION_CHANGING: &str = event_kinds::COLLECTION_CHANGING;
pub const COLLECTION_LIST_CHANGED: &str = event_kinds::COLLECTION_LIST_CHANGED;

pub const PROFILE_CURRENT_CHANGED: &str = event_kinds::PROFILE_CURRENT_CHANGED;
pub const PROFILE_LIST_CHANGED: &str = event_kinds::PROFILE_LIST_CHANGED;

pub const AUDIO_SOURCE_MUTE_CHANGED: &str = event_kinds::AUDIO_SOURCE_MUTE_CHANGED;
pub const AUDIO_SOURCE_VOLUME_CHANGED: &str = event_kinds::AUDIO_SOURCE_VOLUME_CHANGED;
pub const AUDIO_SOURCE_BALANCE_CHANGED: &str = event_kinds::AUDIO_SOURCE_BALANCE_CHANGED;
pub const AUDIO_SOURCE_SYNC_OFFSET_CHANGED: &str = event_kinds::AUDIO_SOURCE_SYNC_OFFSET_CHANGED;

pub const STUDIO_ENABLED: &str = event_kinds::STUDIO_ENABLED;
pub const STUDIO_DISABLED: &str = event_kinds::STUDIO_DISABLED;

pub const TRANSITION_STARTED: &str = event_kinds::TRANSITION_STARTED;
pub const TRANSITION_ENDED: &str = event_kinds::TRANSITION_ENDED;
pub const TRANSITION_VIDEO_ENDED: &str = event_kinds::TRANSITION_VIDEO_ENDED;

pub const VIRTUALCAM_STARTED: &str = event_kinds::VIRTUALCAM_STARTED;
pub const VIRTUALCAM_STOPPED: &str = event_kinds::VIRTUALCAM_STOPPED;
pub const VIRTUALCAM_STATUS_CHANGED: &str = "obs.virtualcam.state_changed";
