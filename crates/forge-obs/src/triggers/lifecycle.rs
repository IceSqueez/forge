use super::families;
use crate::vocabulary::{event_kinds, messages, trigger_ids};
use forge_events::EventSource;
use forge_registry::{EventTriggerSpec, TriggerCondition, TriggerMatcher, no_config_fields};

pub(crate) const RECORDING_STARTED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::RECORDING_STARTED,
    source: EventSource::Obs,
    label: "OBS recording started",
    summary: "Fires when OBS recording output becomes active and writing to disk.",
    search_text: "obs recording started capture active",
    icon: "record",
    prefix: event_kinds::RECORDING_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::RECORDING_STARTED),
    condition: TriggerCondition::Fixed(messages::CONDITION_RECORDING_STARTED),
    config_fields: no_config_fields,
    variables: families::record_variables,
};

pub(crate) const RECORDING_STOPPING: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::RECORDING_STOPPING,
    source: EventSource::Obs,
    label: "OBS recording stopping",
    summary: "Fires when OBS begins the recording stop sequence (before output becomes inactive).",
    search_text: "obs recording stopping end capture finalise",
    icon: "record",
    prefix: event_kinds::RECORDING_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::RECORDING_STOPPING),
    condition: TriggerCondition::Fixed(messages::CONDITION_RECORDING_STOPPING),
    config_fields: no_config_fields,
    variables: families::record_variables,
};

pub(crate) const RECORDING_STOPPED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::RECORDING_STOPPED,
    source: EventSource::Obs,
    label: "OBS recording stopped",
    summary: "Fires when OBS recording output becomes inactive. The output_path arg carries the saved file path.",
    search_text: "obs recording stopped finished saved file path",
    icon: "record",
    prefix: event_kinds::RECORDING_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::RECORDING_STOPPED),
    condition: TriggerCondition::Fixed(messages::CONDITION_RECORDING_STOPPED),
    config_fields: no_config_fields,
    variables: families::record_variables,
};

pub(crate) const RECORDING_PAUSED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::RECORDING_PAUSED,
    source: EventSource::Obs,
    label: "OBS recording paused",
    summary: "Fires when a running OBS recording output is paused.",
    search_text: "obs recording paused hold suspend",
    icon: "record",
    prefix: event_kinds::RECORDING_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::RECORDING_PAUSED),
    condition: TriggerCondition::Fixed(messages::CONDITION_RECORDING_PAUSED),
    config_fields: no_config_fields,
    variables: families::record_variables,
};

pub(crate) const RECORDING_RESUMED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::RECORDING_RESUMED,
    source: EventSource::Obs,
    label: "OBS recording resumed",
    summary: "Fires when a paused OBS recording output resumes writing.",
    search_text: "obs recording resumed unpause continue",
    icon: "record",
    prefix: event_kinds::RECORDING_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::RECORDING_RESUMED),
    condition: TriggerCondition::Fixed(messages::CONDITION_RECORDING_RESUMED),
    config_fields: no_config_fields,
    variables: families::record_variables,
};

pub(crate) const STREAMING_STARTED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::STREAMING_STARTED,
    source: EventSource::Obs,
    label: "OBS stream started",
    summary: "Fires when OBS stream output becomes active and data is flowing.",
    search_text: "obs stream started live on air",
    icon: "broadcast",
    prefix: event_kinds::STREAMING_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::STREAMING_STARTED),
    condition: TriggerCondition::Fixed(messages::CONDITION_STREAM_STARTED),
    config_fields: no_config_fields,
    variables: families::stream_variables,
};

pub(crate) const STREAMING_STOPPING: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::STREAMING_STOPPING,
    source: EventSource::Obs,
    label: "OBS stream stopping",
    summary: "Fires when OBS begins the stream stop sequence (before output goes idle).",
    search_text: "obs stream stopping end going offline",
    icon: "broadcast-off",
    prefix: event_kinds::STREAMING_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::STREAMING_STOPPING),
    condition: TriggerCondition::Fixed(messages::CONDITION_STREAM_STOPPING),
    config_fields: no_config_fields,
    variables: families::stream_variables,
};

pub(crate) const STREAMING_STOPPED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::STREAMING_STOPPED,
    source: EventSource::Obs,
    label: "OBS stream stopped",
    summary: "Fires when OBS stream output has fully stopped.",
    search_text: "obs stream stopped ended offline",
    icon: "broadcast-off",
    prefix: event_kinds::STREAMING_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::STREAMING_STOPPED),
    condition: TriggerCondition::Fixed(messages::CONDITION_STREAM_STOPPED),
    config_fields: no_config_fields,
    variables: families::stream_variables,
};

pub(crate) const STUDIO_DISABLED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::STUDIO_DISABLED,
    source: EventSource::Obs,
    label: "OBS Studio Mode disabled",
    summary: "Fires when OBS Studio Mode is turned off.",
    search_text: "obs studio mode disabled off preview program",
    icon: "layout-columns",
    prefix: event_kinds::STUDIO_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::STUDIO_DISABLED),
    condition: TriggerCondition::Fixed(messages::CONDITION_STUDIO_MODE_DISABLED),
    config_fields: no_config_fields,
    variables: families::studio_variables,
};

pub(crate) const TRANSITION_ENDED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::TRANSITION_ENDED,
    source: EventSource::Obs,
    label: "OBS scene transition ended",
    summary: "Fires when a scene transition completes in OBS (cut point; video may still play).",
    search_text: "obs scene transition ended complete finished fade cut stinger",
    icon: "arrow-right",
    prefix: event_kinds::TRANSITION_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::TRANSITION_ENDED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_TRANSITION_ENDED),
    config_fields: no_config_fields,
    variables: families::transition_variables,
};

pub(crate) const TRANSITION_VIDEO_ENDED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::TRANSITION_VIDEO_ENDED,
    source: EventSource::Obs,
    label: "OBS transition video ended",
    summary: "Fires when a transition's video playback fully completes (stinger video end, not just the cut point).",
    search_text: "obs scene transition video ended stinger playback complete",
    icon: "player-stop",
    prefix: event_kinds::TRANSITION_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::TRANSITION_VIDEO_ENDED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_TRANSITION_VIDEO_ENDED),
    config_fields: no_config_fields,
    variables: families::transition_variables,
};
