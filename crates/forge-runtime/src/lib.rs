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
mod donations;
mod egress;
pub mod event_log_bridge;
mod event_log_writer;
mod event_ring;
mod first_chat_ledger;
pub mod integration_gate;
pub mod latest;
mod latest_overlay_feed;
pub mod live_viewers;
mod overlay_definition_revision;
mod overlay_lanes;
pub mod overlay_media;
pub mod overlay_service;
mod overlay_shows;
mod own_chat_echoes;
mod persist_batch;
mod queue_depth;
pub mod queue_scheduler;
mod run_history;
pub mod scheduled_runs;
pub mod script_registry;
pub mod sound_player;
pub mod speak_dispatcher;
pub mod stream_live;
pub mod sub_action_runners;
mod task_stop;
#[cfg(test)]
mod test_support;
pub mod timer_scheduler;
pub mod trigger_evaluator;
pub mod triggers;
pub mod twitch_emote_lexicon;
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
pub use chat_history_persistence::{ChatHistoryRetentionHandle, spawn_chat_history_persistence};
pub use condition::{ConditionError, ConditionGate};
pub use config::Config;
pub use delivery::CriticalSubscription;
pub use delivery_loss::{ConsumerLoss, DeliveryTier, LossCount, LossWatch};
pub use donations::{
    CatchUpWaker, DONATION_CATCH_UP, DonationAudience, DonationIngest, DonationOverlayAudience,
    TestDonationRunner, register_donation_sub_actions,
};
pub use event_log_bridge::spawn_event_log_bridge;
pub use first_chat_ledger::FirstChatLedger;
pub use integration_gate::IntegrationGate;
pub use latest::{
    LatestResetError, LatestValues, register_latest_sub_actions, spawn_latest_projector,
};
pub use latest_overlay_feed::spawn_latest_overlay_feed;
pub use live_viewers::{LiveViewerAggregatorHandle, LiveViewerCount, spawn_live_viewer_aggregator};
pub use overlay_definition_revision::OverlayDefinitionChanges;
pub use overlay_media::OverlayMediaLibrary;
pub use overlay_service::{
    EnabledOverlay, MaterializePass, OverlayConnectFanout, OverlayConnectListener, OverlayDelivery,
    OverlayDispatch, OverlayFrameSink, OverlayReceivers, OverlayServiceCell, OverlayServiceError,
    OverlayServiceHandle, TestFire,
};
pub use overlay_shows::{
    SHOW_CEILING, SHOW_QUEUE_CAPACITY, SHOW_SPEECH_START_WAIT, ShowDepthWatch, ShowEnd, ShowTicket,
};
pub use queue_depth::{QueueDepth, QueueDepthWatch, QueueDepths};
pub use queue_scheduler::{
    MAX_PENDING_PER_QUEUE, MembershipOutcome, QUEUE_DRAINING_REASON, QUEUE_NOT_FOUND_REASON,
    QUEUE_OVERFLOW_REASON, QUEUE_PAUSED_REASON, QueueIntake, QueueMode, QueueProcessing,
    QueueRuntimeState, QueueScheduler, QueueSchedulerHandle, SchedulerCell, SchedulerError,
    SchedulerRequest,
};
pub use scheduled_runs::{
    CatchUpSettle, HandOff, ScheduleDue, ScheduleError, ScheduleIntent, ScheduleRequest,
    ScheduledPlacement, ScheduledRunsCell, ScheduledRunsHandle, ScheduledRunsParts,
    SchedulingContext, ScriptScheduling, SystemWallClock, WaitingForQueueWatch, WaitingRuns,
    WallClock, spawn_scheduled_runs,
};
pub use script_registry::{CompiledScript, ScriptRegistry, ScriptRegistryError};
pub use sound_player::{SoundPlayer, SoundPlayerError};
pub use speak_dispatcher::{
    ShowSpeech, SpeakDispatchError, SpeakDispatcher, SpeakingViewer, SpeechOrigin,
    SpeechStartSignal, VoiceDescriptor,
};
pub use stream_live::{LiveSource, StreamLiveHandle, StreamLiveState, spawn_stream_live_signal};
pub use sub_action_runners::{
    CASE_CHAIN_KEY, CONTENT_SCHEMA_KEY, CoreSubActionDeps, OVERLAY_SEND_KIND_ID,
    OVERLAY_TARGET_KEY, OverlaySendTarget, decode_steps, feeds_overlay, overlay_send_targets,
    register_core_sub_actions, register_scheduled_run_sub_actions,
};
pub use task_stop::TaskStop;
pub use timer_scheduler::{TimerSchedulerHandle, spawn_timer_scheduler};
pub use trigger_evaluator::{COMMAND_LINE_TARGET, TriggerEvaluatorHandle, spawn_trigger_evaluator};
pub use triggers::register_core_triggers;
pub use twitch_emote_lexicon::{TwitchEmoteLexicon, spawn_twitch_emote_learning};
pub use viewer_tracker::spawn_viewer_tracker;
