use forge_events::Event;
use forge_types::{ArgStack, Trigger, TriggerConfig};

use crate::category::TriggerCategory;
use crate::evaluator::EventFilter;
use crate::form::FormField;

pub trait TriggerKindDescriptor: Send + Sync {
    fn id(&self) -> &str;
    fn category(&self) -> TriggerCategory;
    fn label(&self) -> &str;
    fn summary(&self) -> &str;
    fn search_text(&self) -> &str;
    fn icon_name(&self) -> &str;
    fn default_config(&self) -> TriggerConfig;
    fn config_fields(&self) -> Vec<FormField>;
    fn condition_display(&self, config: &TriggerConfig) -> String;
    fn event_filter(&self) -> EventFilter;
    fn matches_trigger(&self, trigger: &Trigger, event: &Event) -> bool;
    fn build_arg_stack(&self, event: &Event) -> ArgStack;
}
