use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig};

use super::virtualcam_status_changed::build_virtualcam_arg_stack;

pub struct VirtualcamStoppedDescriptor;

impl TriggerKindDescriptor for VirtualcamStoppedDescriptor {
    fn id(&self) -> &str {
        "obs.virtualcam.stopped"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS virtual camera stopped"
    }

    fn summary(&self) -> &str {
        "Fires when the OBS virtual camera output has fully stopped."
    }

    fn search_text(&self) -> &str {
        "obs virtual camera stopped off disabled"
    }

    fn icon_name(&self) -> &str {
        "camera-off"
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
        "virtual camera stopped".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("virtualcam.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "virtualcam.stopped"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_virtualcam_arg_stack(event)
    }
}
