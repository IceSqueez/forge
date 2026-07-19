use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, VariableSchema};

use super::studio_enabled::{build_studio_arg_stack, studio_variables};

pub struct StudioDisabledDescriptor;

impl TriggerKindDescriptor for StudioDisabledDescriptor {
    fn id(&self) -> &str {
        "obs.studio.disabled"
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

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "studio mode disabled".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("studio.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "studio.disabled"
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
