use forge_types::{MessageCatalog, MessageCode, MessageRef, UpstreamDetail, VerbatimText};

use crate::vocabulary::INTEGRATION_ID;
use crate::vocabulary::labels;

pub(crate) const NO_SCENE_TO_LIST: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "no_scene_to_list");
pub(crate) const SCENE_NOT_IN_OBS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "scene_not_in_obs");
const CONNECT_REFUSED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "connect_refused");
const UNREACHABLE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "unreachable");
const PASSWORD_REJECTED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "password_rejected");
const CONNECT_FAILED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "connect_failed");
const CONNECTION_LOST: MessageCode = MessageCode::from_static(INTEGRATION_ID, "connection_lost");
const UNRESPONSIVE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "unresponsive");
const NOT_CONNECTED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "not_connected");
const REQUEST_TIMED_OUT: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "request_timed_out");
const REQUEST_REJECTED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "request_rejected");
const PROTOCOL_ERROR: MessageCode = MessageCode::from_static(INTEGRATION_ID, "protocol_error");
const PAYLOAD_INVALID: MessageCode = MessageCode::from_static(INTEGRATION_ID, "payload_invalid");
const STUDIO_MODE_INACTIVE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "studio_mode_inactive");
const PATH_HOME_RELATIVE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "path_home_relative");
const PATH_ESCAPES_TEMPLATE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "path_escapes_template");

const HEALTH_LIVE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_live");
const HEALTH_OFF: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_off");
const HEALTH_PAUSED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_paused");
const HEALTH_ACTIVE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_active");
const HEALTH_LAGGING: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_lagging");
const HEALTH_SMOOTH: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_smooth");
const HEALTH_CPU_FPS_VALUE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_cpu_fps_value");
const HEALTH_DROPPED_COUNT: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_dropped_count");
const HEALTH_DROPPED_SHARE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_dropped_share");

const REQUEST_ARG: &str = "request";
const CPU_ARG: &str = "cpu";
const FPS_ARG: &str = "fps";
const COUNT_ARG: &str = "count";
const PERCENT_ARG: &str = "percent";

pub const HEALTH_STREAM: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_stream");
pub const HEALTH_RECORDING: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_recording");
pub const HEALTH_CPU_FPS: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_cpu_fps");
pub const HEALTH_DROPPED_FRAMES: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_dropped_frames");

pub(crate) const CONDITION_ANY_COLLECTION_LIST_CHANGE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_collection_list_change");
pub(crate) const CONDITION_AUTHENTICATION_FAILED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_authentication_failed");
pub(crate) const CONDITION_CONNECTED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_connected");
pub(crate) const CONDITION_DISCONNECTED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_disconnected");
pub(crate) const CONDITION_ANY_FILTER_CREATED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_filter_created");
pub(crate) const CONDITION_ANY_FILTER_ENABLE_STATE_CHANGED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_filter_enable_state_changed");
pub(crate) const CONDITION_ANY_FILTER_REMOVED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_filter_removed");
pub(crate) const CONDITION_ANY_PROFILE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_profile");
pub(crate) const CONDITION_ANY_PROFILE_LIST_CHANGE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_profile_list_change");
pub(crate) const CONDITION_RECORDING_FILE_CHANGED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_recording_file_changed");
pub(crate) const CONDITION_RECORDING_PAUSED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_recording_paused");
pub(crate) const CONDITION_RECORDING_RESUMED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_recording_resumed");
pub(crate) const CONDITION_RECORDING_STARTED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_recording_started");
pub(crate) const CONDITION_RECORDING_STARTING: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_recording_starting");
pub(crate) const CONDITION_ANY_RECORDING_STATUS_CHANGE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_recording_status_change");
pub(crate) const CONDITION_RECORDING_STOPPED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_recording_stopped");
pub(crate) const CONDITION_RECORDING_STOPPING: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_recording_stopping");
pub(crate) const CONDITION_ANY_SCENE_CREATED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_scene_created");
pub(crate) const CONDITION_ANY_SCENE_LIST_CHANGE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_scene_list_change");
pub(crate) const CONDITION_ANY_SCENE_REMOVED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_scene_removed");
pub(crate) const CONDITION_ANY_SCENE_RENAMED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_scene_renamed");
pub(crate) const CONDITION_ANY_INPUT_CREATED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_input_created");
pub(crate) const CONDITION_ANY_INPUT_REMOVED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_input_removed");
pub(crate) const CONDITION_ANY_INPUT_RENAMED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_input_renamed");
pub(crate) const CONDITION_ANY_SCENE_ITEM_ADDED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_scene_item_added");
pub(crate) const CONDITION_ANY_SCENE_ITEM_LOCK_CHANGE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_scene_item_lock_change");
pub(crate) const CONDITION_ANY_SCENE_ITEM_REMOVED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_scene_item_removed");
pub(crate) const CONDITION_ANY_SCENE_ITEM_VISIBILITY_CHANGE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_scene_item_visibility_change");
pub(crate) const CONDITION_STREAM_STARTED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_stream_started");
pub(crate) const CONDITION_STREAM_STARTING: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_stream_starting");
pub(crate) const CONDITION_ANY_STREAM_STATUS_CHANGE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_stream_status_change");
pub(crate) const CONDITION_STREAM_STOPPED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_stream_stopped");
pub(crate) const CONDITION_STREAM_STOPPING: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_stream_stopping");
pub(crate) const CONDITION_STUDIO_MODE_DISABLED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_studio_mode_disabled");
pub(crate) const CONDITION_STUDIO_MODE_ENABLED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_studio_mode_enabled");
pub(crate) const CONDITION_ANY_TRANSITION_ENDED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_transition_ended");
pub(crate) const CONDITION_ANY_TRANSITION_STARTED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_transition_started");
pub(crate) const CONDITION_ANY_TRANSITION_VIDEO_ENDED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_transition_video_ended");
pub(crate) const CONDITION_VIRTUAL_CAMERA_STARTED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_virtual_camera_started");
pub(crate) const CONDITION_ANY_VIRTUAL_CAMERA_STATE_CHANGE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_virtual_camera_state_change");
pub(crate) const CONDITION_VIRTUAL_CAMERA_STOPPED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_virtual_camera_stopped");
pub(crate) const CONDITION_ANY_SCENE_COLLECTION: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_scene_collection");
pub(crate) const CONDITION_ANY_SOURCE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_source");
pub(crate) const CONDITION_ANY_SCENE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_scene");
pub(crate) const CONDITION_ANY_PREVIEW_SCENE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_preview_scene");
const CONDITION_SOURCE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "condition_source");
const CONDITION_SCENE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "condition_scene");
const CONDITION_PREVIEW: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_preview");
const SOURCE_CONDITION_ARG: &str = "source";
const SCENE_CONDITION_ARG: &str = "scene";

pub const OBS_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: INTEGRATION_ID,
    codes: &[
        NO_SCENE_TO_LIST,
        SCENE_NOT_IN_OBS,
        HEALTH_STREAM,
        HEALTH_RECORDING,
        HEALTH_CPU_FPS,
        HEALTH_DROPPED_FRAMES,
        HEALTH_LIVE,
        HEALTH_OFF,
        HEALTH_PAUSED,
        HEALTH_ACTIVE,
        HEALTH_LAGGING,
        HEALTH_SMOOTH,
        HEALTH_CPU_FPS_VALUE,
        HEALTH_DROPPED_COUNT,
        HEALTH_DROPPED_SHARE,
        CONNECT_REFUSED,
        UNREACHABLE,
        PASSWORD_REJECTED,
        CONNECT_FAILED,
        CONNECTION_LOST,
        UNRESPONSIVE,
        NOT_CONNECTED,
        REQUEST_TIMED_OUT,
        REQUEST_REJECTED,
        PROTOCOL_ERROR,
        PAYLOAD_INVALID,
        STUDIO_MODE_INACTIVE,
        PATH_HOME_RELATIVE,
        PATH_ESCAPES_TEMPLATE,
        CONDITION_ANY_COLLECTION_LIST_CHANGE,
        CONDITION_AUTHENTICATION_FAILED,
        CONDITION_CONNECTED,
        CONDITION_DISCONNECTED,
        CONDITION_ANY_FILTER_CREATED,
        CONDITION_ANY_FILTER_ENABLE_STATE_CHANGED,
        CONDITION_ANY_FILTER_REMOVED,
        CONDITION_ANY_PROFILE,
        CONDITION_ANY_PROFILE_LIST_CHANGE,
        CONDITION_RECORDING_FILE_CHANGED,
        CONDITION_RECORDING_PAUSED,
        CONDITION_RECORDING_RESUMED,
        CONDITION_RECORDING_STARTED,
        CONDITION_RECORDING_STARTING,
        CONDITION_ANY_RECORDING_STATUS_CHANGE,
        CONDITION_RECORDING_STOPPED,
        CONDITION_RECORDING_STOPPING,
        CONDITION_ANY_SCENE_CREATED,
        CONDITION_ANY_SCENE_LIST_CHANGE,
        CONDITION_ANY_SCENE_REMOVED,
        CONDITION_ANY_SCENE_RENAMED,
        CONDITION_ANY_INPUT_CREATED,
        CONDITION_ANY_INPUT_REMOVED,
        CONDITION_ANY_INPUT_RENAMED,
        CONDITION_ANY_SCENE_ITEM_ADDED,
        CONDITION_ANY_SCENE_ITEM_LOCK_CHANGE,
        CONDITION_ANY_SCENE_ITEM_REMOVED,
        CONDITION_ANY_SCENE_ITEM_VISIBILITY_CHANGE,
        CONDITION_STREAM_STARTED,
        CONDITION_STREAM_STARTING,
        CONDITION_ANY_STREAM_STATUS_CHANGE,
        CONDITION_STREAM_STOPPED,
        CONDITION_STREAM_STOPPING,
        CONDITION_STUDIO_MODE_DISABLED,
        CONDITION_STUDIO_MODE_ENABLED,
        CONDITION_ANY_TRANSITION_ENDED,
        CONDITION_ANY_TRANSITION_STARTED,
        CONDITION_ANY_TRANSITION_VIDEO_ENDED,
        CONDITION_VIRTUAL_CAMERA_STARTED,
        CONDITION_ANY_VIRTUAL_CAMERA_STATE_CHANGE,
        CONDITION_VIRTUAL_CAMERA_STOPPED,
        CONDITION_ANY_SCENE_COLLECTION,
        CONDITION_ANY_SOURCE,
        CONDITION_ANY_SCENE,
        CONDITION_ANY_PREVIEW_SCENE,
        CONDITION_SOURCE,
        CONDITION_SCENE,
        CONDITION_PREVIEW,
        labels::APPLY_FILTER,
        labels::AUDIO_SOURCE,
        labels::COLLECTION,
        labels::ENABLED,
        labels::FILTER,
        labels::FOLDER,
        labels::GAIN_DB,
        labels::LIVE,
        labels::MUTED,
        labels::PROFILE,
        labels::PROFILES,
        labels::RECORDING,
        labels::REFRESH_BROWSER_SOURCE,
        labels::REPLAY_CAPTURE,
        labels::RESTART_MEDIA,
        labels::SAVE_REPLAY_BUFFER,
        labels::SAVE_TO,
        labels::SCENE,
        labels::SCENES,
        labels::SCREENSHOT_SOURCE,
        labels::SET_MUTE,
        labels::SET_PREVIEW_SCENE,
        labels::SET_RECORDING_FOLDER,
        labels::SET_VOLUME,
        labels::SOURCE,
        labels::SOURCES,
        labels::SOURCES_AUDIO,
        labels::STREAM_RECORD,
        labels::STREAMING,
        labels::SWITCH_PROFILE,
        labels::SWITCH_SCENE,
        labels::SWITCH_SCENE_COLLECTION,
        labels::TOGGLE_RECORDING,
        labels::TOGGLE_RECORDING_PAUSE,
        labels::TOGGLE_REPLAY_BUFFER,
        labels::TOGGLE_SOURCE,
        labels::TOGGLE_STREAM,
        labels::TOGGLE_STUDIO_MODE,
        labels::TOGGLE_VIRTUAL_CAMERA,
        labels::TRANSITION,
        labels::TRIGGER_TRANSITION,
        labels::VISIBLE,
        labels::VOLUME_EXAMPLE,
        labels::SCENE_SOURCE_COUNT,
        labels::AUDIO_DB,
        labels::SOURCES_IN_SCENE,
        labels::SOURCE_TOTAL,
        labels::WS_BADGE,
        labels::WS_BADGE_UPTIME,
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};

pub(crate) fn health_live() -> MessageRef {
    MessageRef::new(HEALTH_LIVE)
}

pub(crate) fn health_off() -> MessageRef {
    MessageRef::new(HEALTH_OFF)
}

pub(crate) fn health_paused() -> MessageRef {
    MessageRef::new(HEALTH_PAUSED)
}

pub(crate) fn health_active() -> MessageRef {
    MessageRef::new(HEALTH_ACTIVE)
}

pub(crate) fn health_lagging() -> MessageRef {
    MessageRef::new(HEALTH_LAGGING)
}

pub(crate) fn health_smooth() -> MessageRef {
    MessageRef::new(HEALTH_SMOOTH)
}

pub(crate) fn health_cpu_fps_value(cpu_percent: f64, fps: f64) -> MessageRef {
    MessageRef::new(HEALTH_CPU_FPS_VALUE)
        .with_arg(CPU_ARG, cpu_percent)
        .with_arg(FPS_ARG, fps)
}

pub(crate) fn health_dropped_count(dropped: u64) -> MessageRef {
    MessageRef::new(HEALTH_DROPPED_COUNT).with_arg(COUNT_ARG, dropped)
}

pub(crate) fn health_dropped_share(percent: f64) -> MessageRef {
    MessageRef::new(HEALTH_DROPPED_SHARE).with_arg(PERCENT_ARG, percent)
}

pub(crate) fn connect_refused() -> MessageRef {
    MessageRef::new(CONNECT_REFUSED)
}

pub(crate) fn unreachable() -> MessageRef {
    MessageRef::new(UNREACHABLE)
}

pub(crate) fn password_rejected() -> MessageRef {
    MessageRef::new(PASSWORD_REJECTED)
}

pub(crate) fn connect_failed() -> MessageRef {
    MessageRef::new(CONNECT_FAILED)
}

pub(crate) fn connect_failed_with(detail: &str) -> MessageRef {
    connect_failed().with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn not_connected() -> MessageRef {
    MessageRef::new(NOT_CONNECTED)
}

pub(crate) fn request_timed_out() -> MessageRef {
    MessageRef::new(REQUEST_TIMED_OUT)
}

pub(crate) fn request_rejected(request: &str, detail: &str) -> MessageRef {
    MessageRef::new(REQUEST_REJECTED)
        .with_arg(REQUEST_ARG, VerbatimText::new(request))
        .with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn protocol_error(detail: &str) -> MessageRef {
    MessageRef::new(PROTOCOL_ERROR).with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn payload_invalid(detail: &str) -> MessageRef {
    MessageRef::new(PAYLOAD_INVALID).with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn path_home_relative() -> MessageRef {
    MessageRef::new(PATH_HOME_RELATIVE)
}

pub(crate) fn path_escapes_template() -> MessageRef {
    MessageRef::new(PATH_ESCAPES_TEMPLATE)
}

pub(crate) fn studio_mode_inactive() -> MessageRef {
    MessageRef::new(STUDIO_MODE_INACTIVE)
}

pub(crate) fn connection_lost() -> MessageRef {
    MessageRef::new(CONNECTION_LOST)
}

pub(crate) fn unresponsive() -> MessageRef {
    MessageRef::new(UNRESPONSIVE)
}

pub(crate) fn condition_source(source: &str) -> MessageRef {
    MessageRef::new(CONDITION_SOURCE).with_arg(SOURCE_CONDITION_ARG, VerbatimText::new(source))
}

pub(crate) fn condition_scene(scene: &str) -> MessageRef {
    MessageRef::new(CONDITION_SCENE).with_arg(SCENE_CONDITION_ARG, VerbatimText::new(scene))
}

pub(crate) fn condition_preview(scene: &str) -> MessageRef {
    MessageRef::new(CONDITION_PREVIEW).with_arg(SCENE_CONDITION_ARG, VerbatimText::new(scene))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    fn ftl_variables(source: &str) -> BTreeMap<String, BTreeSet<String>> {
        let mut found: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut current = None;
        for line in source.lines() {
            if let Some((id, _)) = line.split_once(" = ").filter(|_| line.starts_with("msg-")) {
                current = Some(id.trim().to_owned());
                found.entry(id.trim().to_owned()).or_default();
            }
            let Some(id) = &current else { continue };
            let mut rest = line;
            while let Some(at) = rest.find('$') {
                rest = &rest[at + 1..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                    .collect();
                found.get_mut(id).unwrap().insert(name);
            }
        }
        found
    }

    fn every_constructor() -> Vec<MessageRef> {
        vec![
            MessageRef::new(NO_SCENE_TO_LIST),
            MessageRef::new(SCENE_NOT_IN_OBS),
            connect_refused(),
            unreachable(),
            password_rejected(),
            connect_failed(),
            connect_failed_with("c"),
            connection_lost(),
            unresponsive(),
            not_connected(),
            request_timed_out(),
            request_rejected("GetVersion", "c"),
            protocol_error("c"),
            payload_invalid("c"),
            studio_mode_inactive(),
            path_home_relative(),
            path_escapes_template(),
            health_live(),
            health_off(),
            health_paused(),
            health_active(),
            health_lagging(),
            health_smooth(),
            health_cpu_fps_value(1.0, 2.0),
            health_dropped_count(1),
            health_dropped_share(0.5),
            MessageRef::new(HEALTH_STREAM),
            MessageRef::new(HEALTH_RECORDING),
            MessageRef::new(HEALTH_CPU_FPS),
            MessageRef::new(HEALTH_DROPPED_FRAMES),
            MessageRef::new(CONDITION_ANY_COLLECTION_LIST_CHANGE),
            MessageRef::new(CONDITION_AUTHENTICATION_FAILED),
            MessageRef::new(CONDITION_CONNECTED),
            MessageRef::new(CONDITION_DISCONNECTED),
            MessageRef::new(CONDITION_ANY_FILTER_CREATED),
            MessageRef::new(CONDITION_ANY_FILTER_ENABLE_STATE_CHANGED),
            MessageRef::new(CONDITION_ANY_FILTER_REMOVED),
            MessageRef::new(CONDITION_ANY_PROFILE),
            MessageRef::new(CONDITION_ANY_PROFILE_LIST_CHANGE),
            MessageRef::new(CONDITION_RECORDING_FILE_CHANGED),
            MessageRef::new(CONDITION_RECORDING_PAUSED),
            MessageRef::new(CONDITION_RECORDING_RESUMED),
            MessageRef::new(CONDITION_RECORDING_STARTED),
            MessageRef::new(CONDITION_RECORDING_STARTING),
            MessageRef::new(CONDITION_ANY_RECORDING_STATUS_CHANGE),
            MessageRef::new(CONDITION_RECORDING_STOPPED),
            MessageRef::new(CONDITION_RECORDING_STOPPING),
            MessageRef::new(CONDITION_ANY_SCENE_CREATED),
            MessageRef::new(CONDITION_ANY_SCENE_LIST_CHANGE),
            MessageRef::new(CONDITION_ANY_SCENE_REMOVED),
            MessageRef::new(CONDITION_ANY_SCENE_RENAMED),
            MessageRef::new(CONDITION_ANY_INPUT_CREATED),
            MessageRef::new(CONDITION_ANY_INPUT_REMOVED),
            MessageRef::new(CONDITION_ANY_INPUT_RENAMED),
            MessageRef::new(CONDITION_ANY_SCENE_ITEM_ADDED),
            MessageRef::new(CONDITION_ANY_SCENE_ITEM_LOCK_CHANGE),
            MessageRef::new(CONDITION_ANY_SCENE_ITEM_REMOVED),
            MessageRef::new(CONDITION_ANY_SCENE_ITEM_VISIBILITY_CHANGE),
            MessageRef::new(CONDITION_STREAM_STARTED),
            MessageRef::new(CONDITION_STREAM_STARTING),
            MessageRef::new(CONDITION_ANY_STREAM_STATUS_CHANGE),
            MessageRef::new(CONDITION_STREAM_STOPPED),
            MessageRef::new(CONDITION_STREAM_STOPPING),
            MessageRef::new(CONDITION_STUDIO_MODE_DISABLED),
            MessageRef::new(CONDITION_STUDIO_MODE_ENABLED),
            MessageRef::new(CONDITION_ANY_TRANSITION_ENDED),
            MessageRef::new(CONDITION_ANY_TRANSITION_STARTED),
            MessageRef::new(CONDITION_ANY_TRANSITION_VIDEO_ENDED),
            MessageRef::new(CONDITION_VIRTUAL_CAMERA_STARTED),
            MessageRef::new(CONDITION_ANY_VIRTUAL_CAMERA_STATE_CHANGE),
            MessageRef::new(CONDITION_VIRTUAL_CAMERA_STOPPED),
            MessageRef::new(CONDITION_ANY_SCENE_COLLECTION),
            MessageRef::new(CONDITION_ANY_SOURCE),
            MessageRef::new(CONDITION_ANY_SCENE),
            MessageRef::new(CONDITION_ANY_PREVIEW_SCENE),
            condition_source("s"),
            condition_scene("s"),
            condition_preview("s"),
        ]
    }

    #[test]
    fn every_constructor_code_is_catalogued_and_localized_in_both_languages_with_matching_variables()
     {
        let en = ftl_variables(OBS_MESSAGES.en_ftl);
        let uk = ftl_variables(OBS_MESSAGES.uk_ftl);
        for message in every_constructor() {
            let code = message.code();
            assert!(OBS_MESSAGES.codes.contains(code), "{code} not catalogued");
            let passed: BTreeSet<String> = message.args().map(|(n, _)| n.to_owned()).collect();
            for (language, ftl) in [("en", &en), ("uk", &uk)] {
                let used = ftl
                    .get(&code.ftl_key())
                    .unwrap_or_else(|| panic!("{code} missing in {language}"));
                assert_eq!(used, &passed, "{code} variables in {language}");
            }
        }
    }

    #[test]
    fn every_catalogued_code_has_a_constructor_in_the_table() {
        let produced: BTreeSet<MessageCode> = every_constructor()
            .iter()
            .map(|m| m.code().clone())
            .collect();
        let catalogued: BTreeSet<MessageCode> = OBS_MESSAGES
            .codes
            .iter()
            .filter(|code| !code.name().starts_with("label_"))
            .cloned()
            .collect();
        assert_eq!(produced, catalogued);
    }

    #[test]
    fn english_and_ukrainian_declare_the_same_messages_with_the_same_variables() {
        assert_eq!(
            ftl_variables(OBS_MESSAGES.en_ftl),
            ftl_variables(OBS_MESSAGES.uk_ftl)
        );
    }

    #[test]
    fn upstream_cause_text_is_carried_as_detail_and_never_as_an_argument() {
        let failure = request_rejected("SetInputVolume", "ResourceNotFound: no such input");

        assert_eq!(
            failure.detail().map(|d| d.as_str()),
            Some("ResourceNotFound: no such input")
        );
        assert!(
            failure
                .args()
                .all(|(_, v)| !format!("{v:?}").contains("no such input"))
        );
    }
}
