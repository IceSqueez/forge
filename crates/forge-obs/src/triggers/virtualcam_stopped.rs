use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, VariableSchema};

use super::virtualcam_status_changed::{build_virtualcam_arg_stack, virtualcam_variables};

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

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: virtualcam_variables(),
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matches_only_stopped_kind_among_virtualcam_lifecycle() {
        let d = VirtualcamStoppedDescriptor;
        for (kind, expected) in [
            ("virtualcam.stopped", true),
            ("virtualcam.started", false),
            ("virtualcam.starting", false),
            ("virtualcam.stopping", false),
        ] {
            let event = Event::new(EventSource::Obs, kind, json!({}));
            assert_eq!(
                d.matches_trigger(&BTreeMap::new(), &event),
                expected,
                "kind {kind}"
            );
        }
    }

    #[test]
    fn does_not_match_foreign_source_kind() {
        let d = VirtualcamStoppedDescriptor;
        let event = Event::new(EventSource::Obs, "recording.stopped", json!({}));
        assert!(!d.matches_trigger(&BTreeMap::new(), &event));
    }
}
