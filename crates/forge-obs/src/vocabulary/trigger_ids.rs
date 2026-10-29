use super::event_kinds;

pub const CONNECTION_CONNECTED: &str = event_kinds::CONNECTION_CONNECTED;
pub const CONNECTION_DISCONNECTED: &str = event_kinds::CONNECTION_DISCONNECTED;
pub const CONNECTION_AUTH_FAILED: &str = event_kinds::CONNECTION_AUTH_FAILED;

pub const SCENE_CHANGED: &str = event_kinds::SCENE_CHANGED;
pub const SCENE_PREVIEW_CHANGED: &str = event_kinds::SCENE_PREVIEW_CHANGED;
pub const SCENE_LIST_CHANGED: &str = event_kinds::SCENE_LIST_CHANGED;
pub const SCENE_CREATED: &str = event_kinds::SCENE_CREATED;
pub const SCENE_REMOVED: &str = event_kinds::SCENE_REMOVED;
pub const SCENE_RENAMED: &str = event_kinds::SCENE_RENAMED;

pub const SOURCE_INPUT_CREATED: &str = event_kinds::SOURCE_INPUT_CREATED;
pub const SOURCE_INPUT_REMOVED: &str = event_kinds::SOURCE_INPUT_REMOVED;
pub const SOURCE_INPUT_RENAMED: &str = event_kinds::SOURCE_INPUT_RENAMED;
pub const SOURCE_SCENE_ITEM_CREATED: &str = event_kinds::SOURCE_SCENE_ITEM_CREATED;
pub const SOURCE_SCENE_ITEM_REMOVED: &str = event_kinds::SOURCE_SCENE_ITEM_REMOVED;
pub const SOURCE_LOCK_CHANGED: &str = event_kinds::SOURCE_LOCK_CHANGED;
pub const SOURCE_VISIBILITY_CHANGED: &str = event_kinds::SOURCE_VISIBILITY_CHANGED;

pub const FILTER_CREATED: &str = event_kinds::FILTER_CREATED;
pub const FILTER_REMOVED: &str = event_kinds::FILTER_REMOVED;
pub const FILTER_ENABLED_CHANGED: &str = event_kinds::FILTER_ENABLED_CHANGED;

pub const RECORDING_STARTING: &str = event_kinds::RECORDING_STARTING;
pub const RECORDING_STARTED: &str = event_kinds::RECORDING_STARTED;
pub const RECORDING_STOPPING: &str = event_kinds::RECORDING_STOPPING;
pub const RECORDING_STOPPED: &str = event_kinds::RECORDING_STOPPED;
pub const RECORDING_PAUSED: &str = event_kinds::RECORDING_PAUSED;
pub const RECORDING_RESUMED: &str = event_kinds::RECORDING_RESUMED;
pub const RECORDING_FILE_CHANGED: &str = event_kinds::RECORDING_FILE_CHANGED;
pub const RECORDING_STATUS_CHANGED: &str = "obs.recording.status_changed";

pub const STREAMING_STARTING: &str = event_kinds::STREAMING_STARTING;
pub const STREAMING_STARTED: &str = event_kinds::STREAMING_STARTED;
pub const STREAMING_STOPPING: &str = event_kinds::STREAMING_STOPPING;
pub const STREAMING_STOPPED: &str = event_kinds::STREAMING_STOPPED;
pub const STREAMING_STATUS_CHANGED: &str = "obs.streaming.status_changed";

pub const COLLECTION_CHANGED: &str = event_kinds::COLLECTION_CHANGED;
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
