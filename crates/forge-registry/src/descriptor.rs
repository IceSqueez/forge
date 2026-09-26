use forge_events::{DeliveryLane, Event};
use forge_types::{ArgStack, TriggerConfig, VariableSchema};

use crate::category::TriggerCategory;
use crate::evaluator::EventFilter;
use crate::form::FormField;
use crate::kind_platform_contract::KindPlatformContract;
use crate::refinement::FormRefinement;
use crate::variables::{ActorDeclaration, TriggerVariables};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatTriggerFamily {
    Command,
    Message,
}

pub trait TriggerKindDescriptor: Send + Sync {
    fn id(&self) -> &str;
    fn category(&self) -> TriggerCategory;
    fn label(&self) -> &str;
    fn summary(&self) -> &str;
    fn search_text(&self) -> &str;
    fn icon_name(&self) -> &str;
    fn platform_contract(&self) -> KindPlatformContract;
    fn default_config(&self) -> TriggerConfig;
    fn config_fields(&self) -> Vec<FormField>;
    fn config_refinement(&self) -> Option<FormRefinement> {
        None
    }
    fn condition_display(&self, config: &TriggerConfig) -> String;
    fn event_filter(&self) -> EventFilter;
    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool;
    fn variables(&self) -> Option<TriggerVariables> {
        None
    }
    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Undeclared
    }
    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        self.variables()
            .map_or_else(ArgStack::new, |variables| variables.arg_stack(event))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        self.variables().map(|variables| variables.schema())
    }
    /// `Some` only where the event carries a genuine chatter role signal - the chat envelopes
    /// attached to cheer, sub, gift and raid events do not qualify.
    fn chat_trigger_family(&self) -> Option<ChatTriggerFamily> {
        None
    }
    /// `Bulk` only for event kinds that arrive in floods; every kind this descriptor's filter
    /// covers then rides the sheddable lane for every lossless consumer.
    fn delivery_lane(&self) -> DeliveryLane {
        DeliveryLane::Priority
    }
}
