use forge_registry::IdSuccession;

use super::{sub_action_ids, trigger_ids};

pub const RETIRED_TRIGGER_IDS: &[IdSuccession] = &[
    IdSuccession {
        current: trigger_ids::FILTER_CREATED,
        retired: &["obs.filters.created"],
    },
    IdSuccession {
        current: trigger_ids::FILTER_REMOVED,
        retired: &["obs.filters.removed"],
    },
    IdSuccession {
        current: trigger_ids::FILTER_ENABLED_CHANGED,
        retired: &["obs.filters.enabled_changed"],
    },
    IdSuccession {
        current: trigger_ids::RECORDING_STARTED,
        retired: &["obs.record.started"],
    },
    IdSuccession {
        current: trigger_ids::RECORDING_STARTING,
        retired: &["obs.record.starting"],
    },
    IdSuccession {
        current: trigger_ids::RECORDING_STOPPED,
        retired: &["obs.record.stopped"],
    },
    IdSuccession {
        current: trigger_ids::RECORDING_STOPPING,
        retired: &["obs.record.stopping"],
    },
    IdSuccession {
        current: trigger_ids::RECORDING_PAUSED,
        retired: &["obs.record.paused"],
    },
    IdSuccession {
        current: trigger_ids::RECORDING_RESUMED,
        retired: &["obs.record.resumed"],
    },
    IdSuccession {
        current: trigger_ids::RECORDING_FILE_CHANGED,
        retired: &["obs.record.file_changed"],
    },
    IdSuccession {
        current: trigger_ids::RECORDING_STATUS_CHANGED,
        retired: &["obs.record.status_changed"],
    },
    IdSuccession {
        current: trigger_ids::STREAMING_STARTED,
        retired: &["obs.stream.started"],
    },
    IdSuccession {
        current: trigger_ids::STREAMING_STARTING,
        retired: &["obs.stream.starting"],
    },
    IdSuccession {
        current: trigger_ids::STREAMING_STOPPED,
        retired: &["obs.stream.stopped"],
    },
    IdSuccession {
        current: trigger_ids::STREAMING_STOPPING,
        retired: &["obs.stream.stopping"],
    },
    IdSuccession {
        current: trigger_ids::STREAMING_STATUS_CHANGED,
        retired: &["obs.stream.status_changed"],
    },
    IdSuccession {
        current: trigger_ids::SCENE_CREATED,
        retired: &["obs.scenes.created"],
    },
    IdSuccession {
        current: trigger_ids::SCENE_REMOVED,
        retired: &["obs.scenes.removed"],
    },
    IdSuccession {
        current: trigger_ids::SCENE_RENAMED,
        retired: &["obs.scenes.renamed"],
    },
    IdSuccession {
        current: trigger_ids::SCENE_LIST_CHANGED,
        retired: &["obs.scenes.list_changed"],
    },
    IdSuccession {
        current: trigger_ids::SCENE_PREVIEW_CHANGED,
        retired: &["obs.scenes.preview_changed"],
    },
    IdSuccession {
        current: trigger_ids::SCENE_CHANGED,
        retired: &["obs.scenes.current_changed"],
    },
    IdSuccession {
        current: trigger_ids::SOURCE_INPUT_CREATED,
        retired: &["obs.sources.input_created"],
    },
    IdSuccession {
        current: trigger_ids::SOURCE_INPUT_REMOVED,
        retired: &["obs.sources.input_removed"],
    },
    IdSuccession {
        current: trigger_ids::SOURCE_INPUT_RENAMED,
        retired: &["obs.sources.input_renamed"],
    },
    IdSuccession {
        current: trigger_ids::SOURCE_SCENE_ITEM_CREATED,
        retired: &["obs.sources.scene_item_created"],
    },
    IdSuccession {
        current: trigger_ids::SOURCE_SCENE_ITEM_REMOVED,
        retired: &["obs.sources.scene_item_removed"],
    },
    IdSuccession {
        current: trigger_ids::SOURCE_LOCK_CHANGED,
        retired: &["obs.sources.scene_item_lock_changed"],
    },
    IdSuccession {
        current: trigger_ids::SOURCE_VISIBILITY_CHANGED,
        retired: &["obs.sources.scene_item_visibility_changed"],
    },
    IdSuccession {
        current: trigger_ids::COLLECTION_CHANGED,
        retired: &["obs.collection.current_changed"],
    },
];

pub const RETIRED_SUB_ACTION_IDS: &[IdSuccession] = &[
    IdSuccession {
        current: sub_action_ids::SCENE_GET_LIST,
        retired: &["obs.scenes.get_list"],
    },
    IdSuccession {
        current: sub_action_ids::SCENE_SWITCH_CURRENT,
        retired: &["obs.scenes.switch_current"],
    },
    IdSuccession {
        current: sub_action_ids::SCENE_SET_PREVIEW,
        retired: &["obs.scenes.set_preview"],
    },
    IdSuccession {
        current: sub_action_ids::SCENE_SET_TRANSITION,
        retired: &["obs.scenes.set_transition"],
    },
    IdSuccession {
        current: sub_action_ids::SOURCE_GET_LIST,
        retired: &["obs.sources.get_list"],
    },
    IdSuccession {
        current: sub_action_ids::SOURCE_GET_INPUT_SETTINGS,
        retired: &["obs.sources.get_input_settings"],
    },
    IdSuccession {
        current: sub_action_ids::SOURCE_SET_INPUT_SETTINGS,
        retired: &["obs.sources.set_input_settings"],
    },
    IdSuccession {
        current: sub_action_ids::SOURCE_SET_VISIBLE,
        retired: &["obs.sources.set_visible"],
    },
    IdSuccession {
        current: sub_action_ids::SOURCE_SET_LOCKED,
        retired: &["obs.sources.set_locked"],
    },
    IdSuccession {
        current: sub_action_ids::RECORDING_GET_STATUS,
        retired: &["obs.record.get_status"],
    },
    IdSuccession {
        current: sub_action_ids::RECORDING_START,
        retired: &["obs.record.start"],
    },
    IdSuccession {
        current: sub_action_ids::RECORDING_STOP,
        retired: &["obs.record.stop"],
    },
    IdSuccession {
        current: sub_action_ids::RECORDING_PAUSE,
        retired: &["obs.record.pause"],
    },
    IdSuccession {
        current: sub_action_ids::RECORDING_RESUME,
        retired: &["obs.record.resume"],
    },
    IdSuccession {
        current: sub_action_ids::RECORDING_TOGGLE_PAUSE,
        retired: &["obs.record.toggle_pause"],
    },
    IdSuccession {
        current: sub_action_ids::RECORDING_SET_ACTIVE,
        retired: &["obs.record.set_active"],
    },
    IdSuccession {
        current: sub_action_ids::RECORDING_SET_DIRECTORY,
        retired: &["obs.record.set_directory"],
    },
    IdSuccession {
        current: sub_action_ids::STREAMING_GET_STATUS,
        retired: &["obs.stream.get_status"],
    },
    IdSuccession {
        current: sub_action_ids::STREAMING_START,
        retired: &["obs.stream.start"],
    },
    IdSuccession {
        current: sub_action_ids::STREAMING_STOP,
        retired: &["obs.stream.stop"],
    },
    IdSuccession {
        current: sub_action_ids::STREAMING_SEND_CAPTION,
        retired: &["obs.stream.send_caption"],
    },
    IdSuccession {
        current: sub_action_ids::STREAMING_SET_ACTIVE,
        retired: &["obs.stream.set_active"],
    },
    IdSuccession {
        current: sub_action_ids::COLLECTION_SWITCH,
        retired: &["obs.scene_collection.switch"],
    },
];
