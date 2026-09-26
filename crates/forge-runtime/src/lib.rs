pub mod action_cancel;
pub mod action_engine;
pub mod actions;
pub mod audio_runners;
mod bridge;
pub mod bus;
pub mod catalog;
pub mod chain;
pub mod chat_history_persistence;
mod chat_stream;
pub mod condition;
pub mod config;
mod cooldown;
pub mod dashboard;
pub mod delivery;
mod delivery_loss;
mod egress;
pub mod event_log_bridge;
mod event_log_writer;
mod event_ring;
pub mod live_viewers;
mod overlay_lanes;
pub mod overlay_media;
pub mod overlay_service;
mod overlay_shows;
mod persist_batch;
pub mod queue_scheduler;
mod run_history;
pub mod script_registry;
pub mod sound_player;
pub mod speak_dispatcher;
pub mod sub_action_runners;
#[cfg(test)]
mod test_support;
pub mod trigger_evaluator;
pub mod triggers;
pub mod viewer_tracker;

pub use action_cancel::ActionCancelRegistry;
pub use action_engine::{
    ActionEngineHandle, DispatchError, ExecutionRequest, PendingQuickAction, spawn_action_engine,
};
pub use audio_runners::register_audio_sub_actions;
pub use bridge::bus_subscription;
pub use bus::{BusError, Delivery, EventBus, EventSubscription, NullEventLogRepo};
pub use catalog::{Catalog, CatalogBinding, CatalogSnapshot};
pub use chain::{ChainEngine, ChainRun, ChainScope};
pub use chat_history_persistence::spawn_chat_history_persistence;
pub use condition::{ConditionError, ConditionGate};
pub use config::Config;
pub use delivery::CriticalSubscription;
pub use delivery_loss::{ConsumerLoss, DeliveryTier, LossCount, LossWatch};
pub use event_log_bridge::spawn_event_log_bridge;
pub use live_viewers::{LiveViewerAggregatorHandle, LiveViewerCount, spawn_live_viewer_aggregator};
pub use overlay_media::OverlayMediaLibrary;
pub use overlay_service::{
    MaterializePass, OverlayConnectListener, OverlayDelivery, OverlayDispatch, OverlayFrameSink,
    OverlayReceivers, OverlayServiceCell, OverlayServiceError, OverlayServiceHandle, TestFire,
};
pub use overlay_shows::{
    SHOW_CEILING, SHOW_QUEUE_CAPACITY, SHOW_SPEECH_START_WAIT, ShowDepthWatch, ShowEnd, ShowTicket,
};
pub use queue_scheduler::{
    MAX_PENDING_PER_QUEUE, MembershipOutcome, QueueIntake, QueueMode, QueueProcessing,
    QueueRuntimeState, QueueScheduler, QueueSchedulerHandle, SchedulerCell, SchedulerError,
    SchedulerRequest,
};
pub use script_registry::{CompiledScript, ScriptRegistry, ScriptRegistryError};
pub use sound_player::{SoundPlayer, SoundPlayerError};
pub use speak_dispatcher::{
    ShowSpeech, SpeakDispatchError, SpeakDispatcher, SpeechStartSignal, VoiceDescriptor,
};
pub use sub_action_runners::{
    CONTENT_SCHEMA_KEY, OVERLAY_SEND_KIND_ID, OVERLAY_TARGET_KEY, OverlaySendTarget, feeds_overlay,
    overlay_send_targets, register_core_sub_actions,
};
pub use trigger_evaluator::{COMMAND_LINE_TARGET, TriggerEvaluatorHandle, spawn_trigger_evaluator};
pub use triggers::register_core_triggers;
pub use viewer_tracker::spawn_viewer_tracker;
