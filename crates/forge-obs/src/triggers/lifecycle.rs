use forge_events::{Event, EventSource};
use forge_registry::{EventFilter, KindPlatformContract, TriggerCategory, TriggerKindDescriptor};
use forge_types::{
    ArgStack, DeclaredVariable, MessageCode, TriggerConfig, UserText, VariableSchema,
};

use super::record_starting::{build_record_arg_stack, record_variables};
use super::stream_starting::{build_stream_arg_stack, stream_variables};
use super::studio_enabled::{build_studio_arg_stack, studio_variables};
use super::transition_started::{build_transition_arg_stack, transition_variables};
use crate::vocabulary::{event_kinds, messages, trigger_ids};

pub(crate) struct OutputFamily {
    kind_prefix: &'static str,
    variables: fn() -> Vec<DeclaredVariable>,
    build_arg_stack: fn(&Event) -> ArgStack,
}

const RECORD: OutputFamily = OutputFamily {
    kind_prefix: event_kinds::RECORDING_PREFIX,
    variables: record_variables,
    build_arg_stack: build_record_arg_stack,
};

const STREAM: OutputFamily = OutputFamily {
    kind_prefix: event_kinds::STREAMING_PREFIX,
    variables: stream_variables,
    build_arg_stack: build_stream_arg_stack,
};

const STUDIO: OutputFamily = OutputFamily {
    kind_prefix: event_kinds::STUDIO_PREFIX,
    variables: studio_variables,
    build_arg_stack: build_studio_arg_stack,
};

const TRANSITION: OutputFamily = OutputFamily {
    kind_prefix: event_kinds::TRANSITION_PREFIX,
    variables: transition_variables,
    build_arg_stack: build_transition_arg_stack,
};

pub(crate) struct LifecycleTrigger {
    id: &'static str,
    label: &'static str,
    summary: &'static str,
    search_text: &'static str,
    icon_name: &'static str,
    condition: MessageCode,
    event_kind: &'static str,
    family: &'static OutputFamily,
}

pub(crate) const RECORDING_STARTED: LifecycleTrigger = LifecycleTrigger {
    id: trigger_ids::RECORDING_STARTED,
    label: "OBS recording started",
    summary: "Fires when OBS recording output becomes active and writing to disk.",
    search_text: "obs recording started capture active",
    icon_name: "record",
    condition: messages::CONDITION_RECORDING_STARTED,
    event_kind: event_kinds::RECORDING_STARTED,
    family: &RECORD,
};

pub(crate) const RECORDING_STOPPING: LifecycleTrigger = LifecycleTrigger {
    id: trigger_ids::RECORDING_STOPPING,
    label: "OBS recording stopping",
    summary: "Fires when OBS begins the recording stop sequence (before output becomes inactive).",
    search_text: "obs recording stopping end capture finalise",
    icon_name: "record",
    condition: messages::CONDITION_RECORDING_STOPPING,
    event_kind: event_kinds::RECORDING_STOPPING,
    family: &RECORD,
};

pub(crate) const RECORDING_STOPPED: LifecycleTrigger = LifecycleTrigger {
    id: trigger_ids::RECORDING_STOPPED,
    label: "OBS recording stopped",
    summary: "Fires when OBS recording output becomes inactive. The output_path arg carries the saved file path.",
    search_text: "obs recording stopped finished saved file path",
    icon_name: "record",
    condition: messages::CONDITION_RECORDING_STOPPED,
    event_kind: event_kinds::RECORDING_STOPPED,
    family: &RECORD,
};

pub(crate) const RECORDING_PAUSED: LifecycleTrigger = LifecycleTrigger {
    id: trigger_ids::RECORDING_PAUSED,
    label: "OBS recording paused",
    summary: "Fires when a running OBS recording output is paused.",
    search_text: "obs recording paused hold suspend",
    icon_name: "record",
    condition: messages::CONDITION_RECORDING_PAUSED,
    event_kind: event_kinds::RECORDING_PAUSED,
    family: &RECORD,
};

pub(crate) const RECORDING_RESUMED: LifecycleTrigger = LifecycleTrigger {
    id: trigger_ids::RECORDING_RESUMED,
    label: "OBS recording resumed",
    summary: "Fires when a paused OBS recording output resumes writing.",
    search_text: "obs recording resumed unpause continue",
    icon_name: "record",
    condition: messages::CONDITION_RECORDING_RESUMED,
    event_kind: event_kinds::RECORDING_RESUMED,
    family: &RECORD,
};

pub(crate) const STREAMING_STARTED: LifecycleTrigger = LifecycleTrigger {
    id: trigger_ids::STREAMING_STARTED,
    label: "OBS stream started",
    summary: "Fires when OBS stream output becomes active and data is flowing.",
    search_text: "obs stream started live on air",
    icon_name: "broadcast",
    condition: messages::CONDITION_STREAM_STARTED,
    event_kind: event_kinds::STREAMING_STARTED,
    family: &STREAM,
};

pub(crate) const STREAMING_STOPPING: LifecycleTrigger = LifecycleTrigger {
    id: trigger_ids::STREAMING_STOPPING,
    label: "OBS stream stopping",
    summary: "Fires when OBS begins the stream stop sequence (before output goes idle).",
    search_text: "obs stream stopping end going offline",
    icon_name: "broadcast-off",
    condition: messages::CONDITION_STREAM_STOPPING,
    event_kind: event_kinds::STREAMING_STOPPING,
    family: &STREAM,
};

pub(crate) const STREAMING_STOPPED: LifecycleTrigger = LifecycleTrigger {
    id: trigger_ids::STREAMING_STOPPED,
    label: "OBS stream stopped",
    summary: "Fires when OBS stream output has fully stopped.",
    search_text: "obs stream stopped ended offline",
    icon_name: "broadcast-off",
    condition: messages::CONDITION_STREAM_STOPPED,
    event_kind: event_kinds::STREAMING_STOPPED,
    family: &STREAM,
};

pub(crate) const STUDIO_DISABLED: LifecycleTrigger = LifecycleTrigger {
    id: trigger_ids::STUDIO_DISABLED,
    label: "OBS Studio Mode disabled",
    summary: "Fires when OBS Studio Mode is turned off.",
    search_text: "obs studio mode disabled off preview program",
    icon_name: "layout-columns",
    condition: messages::CONDITION_STUDIO_MODE_DISABLED,
    event_kind: event_kinds::STUDIO_DISABLED,
    family: &STUDIO,
};

pub(crate) const TRANSITION_ENDED: LifecycleTrigger = LifecycleTrigger {
    id: trigger_ids::TRANSITION_ENDED,
    label: "OBS scene transition ended",
    summary: "Fires when a scene transition completes in OBS (cut point; video may still play).",
    search_text: "obs scene transition ended complete finished fade cut stinger",
    icon_name: "arrow-right",
    condition: messages::CONDITION_ANY_TRANSITION_ENDED,
    event_kind: event_kinds::TRANSITION_ENDED,
    family: &TRANSITION,
};

pub(crate) const TRANSITION_VIDEO_ENDED: LifecycleTrigger = LifecycleTrigger {
    id: trigger_ids::TRANSITION_VIDEO_ENDED,
    label: "OBS transition video ended",
    summary: "Fires when a transition's video playback fully completes (stinger video end, not just the cut point).",
    search_text: "obs scene transition video ended stinger playback complete",
    icon_name: "player-stop",
    condition: messages::CONDITION_ANY_TRANSITION_VIDEO_ENDED,
    event_kind: event_kinds::TRANSITION_VIDEO_ENDED,
    family: &TRANSITION,
};

impl TriggerKindDescriptor for LifecycleTrigger {
    fn id(&self) -> &str {
        self.id
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Integration
    }

    fn label(&self) -> &str {
        self.label
    }

    fn summary(&self) -> &str {
        self.summary
    }

    fn search_text(&self) -> &str {
        self.search_text
    }

    fn icon_name(&self) -> &str {
        self.icon_name
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn condition_display(&self, _: &TriggerConfig) -> UserText {
        self.condition.clone().into()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some(self.family.kind_prefix.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == self.event_kind
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        (self.family.build_arg_stack)(event)
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: (self.family.variables)(),
        })
    }
}
