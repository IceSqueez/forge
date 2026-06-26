pub mod action_engine;
pub mod actions;
pub mod audio_runners;
mod bridge;
mod buf;
pub mod bus;
pub mod chain;
pub mod chat_stream;
pub mod condition;
pub mod config;
pub mod dashboard;
pub mod queue_scheduler;
pub mod script_registry;
pub mod sound_player;
pub mod speak_dispatcher;
pub mod sub_action_runners;
pub mod trigger_evaluator;
pub mod triggers;

pub use action_engine::{ActionEngineHandle, DispatchError, ExecutionRequest, spawn_action_engine};
pub use audio_runners::register_audio_sub_actions;
pub use bridge::bus_subscription;
pub use bus::{BusError, BusStats, EventBus, EventSubscription, NullEventLogRepo};
pub use chain::{ChainEngine, ChainRun, ChainScope};
pub use chat_stream::chat_stream;
pub use condition::{ConditionError, ConditionGate};
pub use config::Config;
pub use queue_scheduler::{
    MembershipOutcome, QueueScheduler, QueueSchedulerHandle, SchedulerError, SchedulerRequest,
};
pub use script_registry::{CompiledScript, ScriptRegistry, ScriptRegistryError};
pub use sound_player::{SoundPlayer, SoundPlayerError};
pub use speak_dispatcher::{SpeakDispatchError, SpeakDispatcher, VoiceDescriptor};
pub use sub_action_runners::register_core_sub_actions;
pub use trigger_evaluator::{TriggerEvaluatorHandle, spawn_trigger_evaluator};
pub use triggers::register_core_triggers;
