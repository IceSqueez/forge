use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, UserText, VariableSchema};

use super::studio_enabled::{build_studio_arg_stack, studio_variables};

use crate::vocabulary::{event_kinds, messages, trigger_ids};

pub(crate) struct StudioDisabledDescriptor;

impl TriggerKindDescriptor for StudioDisabledDescriptor {
    fn id(&self) -> &str {
        trigger_ids::STUDIO_DISABLED
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS Studio Mode disabled"
    }

    fn summary(&self) -> &str {
        "Fires when OBS Studio Mode is turned off."
    }

    fn search_text(&self) -> &str {
        "obs studio mode disabled off preview program"
    }

    fn icon_name(&self) -> &str {
        "layout-columns"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn default_config(&self) -> TriggerConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn condition_display(&self, _: &TriggerConfig) -> UserText {
        messages::CONDITION_STUDIO_MODE_DISABLED.into()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some(event_kinds::STUDIO_PREFIX.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == event_kinds::STUDIO_DISABLED
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_studio_arg_stack(event)
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: studio_variables(),
        })
    }
}
