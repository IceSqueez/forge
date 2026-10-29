mod analyzer;
mod category;
mod chain;
mod config_ext;
mod config_problem;
mod descriptor;
mod duration_bounds;
mod error;
mod evaluator;
mod first_chatter;
mod flow_kinds;
mod form;
mod integration_references;
mod io;
mod kind_platform_contract;
mod merge;
mod message_emotes;
pub mod references;
mod refinement;
mod registry;
mod run_context;
pub mod runner;
mod scope;
mod step_tree;
mod synthesis;
mod telemetry;
mod variables;

pub use analyzer::{Finding, HealthSeverity, StepHealth, analyze, flag_failed, flag_switched_off};
pub use category::{SubActionCategory, TriggerCategory};
pub use chain::{
    CancelSignal, ChainExecutor, ChainSignal, ChildChainOutcome, ControlCell, ControlSignal,
    RunningAction, StopMark, TelemetrySink,
};
pub use config_ext::SubActionConfigExt;
pub use config_problem::{ConfigProblem, ConfigReason, ValueBounds};
pub use descriptor::{
    ChatTriggerFamily, IGNORE_BOT_MESSAGES_KEY, TriggerKindDescriptor, ignores_bot_messages,
};
pub use duration_bounds::{
    DURATION_UNIT_DAYS, DURATION_UNIT_HOURS, DURATION_UNIT_MILLISECONDS, DURATION_UNIT_MINUTES,
    DURATION_UNIT_SECONDS, DurationBase, DurationBounds,
};
pub use error::RegistryError;
pub use evaluator::{EventFilter, kind_matches_prefix};
pub use first_chatter::{
    FIRST_CHATTERS_ONLY, FIRST_MESSAGE_VARIABLE, admits_chatter, chat_message_condition,
    first_chatters_only_field, is_first_chat_message,
};
pub use flow_kinds::{BREAK_LOOP_KIND_ID, CONTINUE_LOOP_KIND_ID, LOOP_KIND_ID, STOP_KIND_ID};
pub use form::{
    AmountUnit, CodeKind, CodeLanguage, ENGINE_VOICE_OPTIONS_KEY, FormField, UnitAmountBounds,
};
pub use integration_references::{IntegrationReferences, ReferencingAction, tally_references};
pub use io::{ProducedVariable, SubActionIo};
pub use kind_platform_contract::KindPlatformContract;
pub use merge::effective_config;
pub use message_emotes::{MESSAGE_EMOTES_VARIABLE, chat_emote_codes, message_emote_codes};
pub use references::{
    ConfigReference, ExternalNamespace, InternalNamespace, LocationSegment, ReferenceCatalog,
    ReferenceField, ReferenceKey, ReferenceListing, ReferenceLocation, ReferenceNamespace,
    ReferenceRemap, ReferenceScope, UnknownKind,
};
pub use refinement::{FormRefinement, FormSchemaSource, refined_fields};
pub use registry::{
    OwnedSubActionRegistration, OwnedTriggerRegistration, SubActionRegistry, TriggerRegistry,
};
pub use run_context::RunContext;
pub use runner::{SubActionConfig, SubActionRunner};
pub use scope::{ScopeLevel, ScopeOrigin, ScopedVariable, variables_in_scope};
pub use step_tree::{
    CASE_CHAIN_KEY, ChainEntry, DecodedChain, STEP_CONDITION_KEY, STEP_CONFIG_KEY,
    STEP_CONTINUE_ON_ERROR_KEY, STEP_ENABLED_KEY, STEP_KIND_KEY, STEP_LABEL_KEY, StepNode,
    StepReach, UndecodableReason, UndecodableStep, decode_chain, decode_step, decode_steps,
    encode_chain, encode_step, nested_chains, rewrite_steps, walk_steps,
};
pub use synthesis::{SynthesisSample, synthesize_args};
pub use telemetry::StepTimer;
pub use variables::{
    ActorBlock, ActorDeclaration, ActorIdentity, LoginSlot, SourcedActor, TriggerVariable,
    TriggerVariables, declared_variables, money_value,
};
