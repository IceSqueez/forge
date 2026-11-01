use forge_events::Event;
use forge_registry::TriggerVariables;
use forge_registry::{EventVariable, event_variables};
use forge_types::{Variant, VariantKind};

use crate::vocabulary::fields::{audio, filter, recording, streaming, transition, virtualcam};
use crate::vocabulary::{event_kinds, variables};

fn output_state(event: &Event) -> Option<Variant> {
    Some(Variant::String(
        event.kind.rsplit('.').next().unwrap_or_default().to_owned(),
    ))
}

fn studio_enabled(event: &Event) -> Option<Variant> {
    Some(Variant::Bool(event.kind == event_kinds::STUDIO_ENABLED))
}

const RECORD: &[EventVariable] = &[
    EventVariable::custom(
        variables::recording::OUTPUT_STATE,
        VariantKind::String,
        "Recording output state",
        output_state,
    ),
    EventVariable::flag(
        variables::recording::IS_ACTIVE,
        "Recording active",
        recording::IS_ACTIVE,
    ),
    EventVariable::text(
        variables::recording::OUTPUT_PATH,
        "Recording file path",
        recording::OUTPUT_PATH,
    ),
];

const STREAM: &[EventVariable] = &[
    EventVariable::custom(
        variables::streaming::OUTPUT_STATE,
        VariantKind::String,
        "Streaming output state",
        output_state,
    ),
    EventVariable::flag(
        variables::streaming::IS_ACTIVE,
        "Streaming active",
        streaming::IS_ACTIVE,
    ),
];

const VIRTUALCAM: &[EventVariable] = &[
    EventVariable::custom(
        variables::virtualcam::OUTPUT_STATE,
        VariantKind::String,
        "Virtual camera output state",
        output_state,
    ),
    EventVariable::flag(
        variables::virtualcam::IS_ACTIVE,
        "Virtual camera active",
        virtualcam::IS_ACTIVE,
    ),
];

const STUDIO: &[EventVariable] = &[EventVariable::custom(
    variables::studio::ENABLED,
    VariantKind::Bool,
    "Studio mode enabled",
    studio_enabled,
)];

const TRANSITION: &[EventVariable] = &[EventVariable::text(
    variables::transition::NAME,
    "Transition name",
    transition::TRANSITION_NAME,
)];

const FILTER_SOURCE: &[EventVariable] = &[
    EventVariable::text(variables::source::NAME, "Source name", filter::SOURCE_NAME),
    EventVariable::text(variables::filter::NAME, "Filter name", filter::FILTER_NAME),
];

const SOURCE_MUTE: &[EventVariable] = &[
    EventVariable::text(variables::source::NAME, "Source name", audio::SOURCE_NAME),
    EventVariable::flag(variables::source::IS_MUTED, "Source muted", audio::IS_MUTED),
];

pub(super) fn record_variables() -> Option<TriggerVariables> {
    event_variables(RECORD)
}

pub(super) fn stream_variables() -> Option<TriggerVariables> {
    event_variables(STREAM)
}

pub(super) fn virtualcam_variables() -> Option<TriggerVariables> {
    event_variables(VIRTUALCAM)
}

pub(super) fn studio_variables() -> Option<TriggerVariables> {
    event_variables(STUDIO)
}

pub(super) fn transition_variables() -> Option<TriggerVariables> {
    event_variables(TRANSITION)
}

pub(super) fn filter_source_variables() -> Option<TriggerVariables> {
    event_variables(FILTER_SOURCE)
}

pub(super) fn source_mute_variables() -> Option<TriggerVariables> {
    event_variables(SOURCE_MUTE)
}

const FILTER_CREATED: &[EventVariable] = &[
    EventVariable::text(variables::source::NAME, "Source name", filter::SOURCE_NAME),
    EventVariable::text(variables::filter::NAME, "Filter name", filter::FILTER_NAME),
    EventVariable::text(variables::filter::KIND, "Filter kind", filter::FILTER_KIND),
];

const FILTER_ENABLED_CHANGED: &[EventVariable] = &[
    EventVariable::text(variables::source::NAME, "Source name", filter::SOURCE_NAME),
    EventVariable::text(variables::filter::NAME, "Filter name", filter::FILTER_NAME),
    EventVariable::flag(
        variables::filter::IS_ENABLED,
        "Filter enabled",
        filter::IS_ENABLED,
    ),
];

pub(super) fn filter_created_variables() -> Option<TriggerVariables> {
    event_variables(FILTER_CREATED)
}

pub(super) fn filter_enabled_changed_variables() -> Option<TriggerVariables> {
    event_variables(FILTER_ENABLED_CHANGED)
}
