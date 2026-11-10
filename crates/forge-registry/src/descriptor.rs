use forge_events::{DeliveryLane, Event};
use forge_types::{ArgStack, TriggerConfig, UserText, VariableSchema, Variant};
use time::OffsetDateTime;

use crate::category::TriggerCategory;
use crate::chat_event::ChatEventKind;
use crate::condition_messages::any_condition;
use crate::config_problem::ConfigProblem;
use crate::event_filter::EventFilter;
use crate::form::FormField;
use crate::kind_platform_contract::KindPlatformContract;
use crate::references::ReferenceField;
use crate::refinement::FormRefinement;
use crate::variables::{ActorDeclaration, TriggerVariables};
use crate::vocabulary::ACTOR_IDENTITY_VARIABLES;

pub const IGNORE_BOT_MESSAGES_KEY: &str = "ignore_bot_messages";

pub fn ignores_bot_messages(config: &TriggerConfig) -> bool {
    config
        .get(IGNORE_BOT_MESSAGES_KEY)
        .and_then(Variant::as_bool)
        .unwrap_or(true)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatTriggerFamily {
    Command,
    Message,
}

pub trait TriggerCatalogEntry {
    fn category(&self) -> TriggerCategory;
    fn label(&self) -> &str;
    fn summary(&self) -> &str;
    fn search_text(&self) -> &str;
    fn icon_name(&self) -> &str;
}

pub trait TriggerKindDescriptor: TriggerCatalogEntry + Send + Sync {
    fn id(&self) -> &str;
    fn platform_contract(&self) -> KindPlatformContract;
    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }
    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }
    fn config_refinement(&self) -> Option<FormRefinement> {
        None
    }
    fn check_config(&self, _: &TriggerConfig) -> Result<(), ConfigProblem> {
        Ok(())
    }
    fn declared_references(&self) -> Vec<ReferenceField> {
        Vec::new()
    }
    fn condition_display(&self, _: &TriggerConfig) -> UserText {
        any_condition()
    }
    fn event_filter(&self) -> EventFilter;
    fn chat_event(&self) -> Option<ChatEventKind> {
        None
    }
    fn targeted_instance_field(&self) -> Option<&'static str> {
        None
    }
    fn service_filter_field(&self) -> Option<&'static str> {
        None
    }
    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool;
    fn variables(&self) -> Option<TriggerVariables> {
        None
    }
    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Undeclared
    }
    fn follow_moment(&self, _: &Event) -> Option<OffsetDateTime> {
        None
    }
    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        self.variables()
            .map_or_else(ArgStack::new, |variables| variables.arg_stack(event))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        self.variables().map(|variables| variables.schema())
    }
    fn cooldown_identity(&self, args: &ArgStack) -> Option<String> {
        ACTOR_IDENTITY_VARIABLES.iter().find_map(|name| {
            args.get(name)
                .and_then(Variant::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        })
    }
    fn chat_trigger_family(&self) -> Option<ChatTriggerFamily> {
        None
    }
    fn delivery_lane(&self) -> DeliveryLane {
        DeliveryLane::Priority
    }
}
