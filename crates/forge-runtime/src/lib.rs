mod action_cancel;
mod action_engine;
pub mod actions;
pub mod audio_runners;
pub mod bus;
mod catalog;
mod chain;
mod chat_history_persistence;
mod chat_send_router;
mod chat_stream;
mod condition;
mod config;
mod cooldown;
mod core_messages;
pub mod dashboard;
mod date_names;
pub mod delivery;
mod delivery_loss;
mod donations;
mod egress;
mod engine_voice_ref;
mod event_log_bridge;
mod event_log_writer;
mod event_ring;
mod first_chat_ledger;
mod global_mutation;
mod integration_gate;
mod integration_start_error;
mod integration_supervisor;
mod latest;
mod latest_overlay_feed;
mod lifecycle_messages;
mod live_viewers;
mod notification_policy;
mod overlay_definition_revision;
mod overlay_lanes;
mod overlay_media;
pub mod overlay_service;
mod overlay_shows;
mod own_chat_echoes;
mod persist_batch;
mod queue_depth;
pub mod queue_scheduler;
mod run_history;
pub mod scheduled_runs;
mod script_registry;
pub mod settings;
mod speak_dispatcher;
mod stream_live;
pub mod sub_action_runners;
mod task_stop;
#[cfg(test)]
mod test_support;
mod timer_scheduler;
mod trigger_evaluator;
pub mod triggers;
mod twitch_emote_lexicon;
mod viewer_tracker;

pub use action_cancel::ActionCancelRegistry;
pub use action_engine::{
    ActionEngineHandle, DispatchError, ExecutionRequest, PendingQuickAction, spawn_action_engine,
};
pub use audio_runners::register_speech_sub_actions;
pub use bus::{BusError, EventBus, EventHistory, NullEventLogRepo};
pub use catalog::{Catalog, CatalogBinding, CatalogSnapshot};
pub use chain::{ChainEngine, ChainRun, ChainScope};
pub use chat_history_persistence::{ChatHistoryRetentionHandle, spawn_chat_history_persistence};
pub use chat_send_router::spawn_chat_send_router;
pub use condition::{ConditionError, ConditionGate};
pub use config::Config;
pub use core_messages::CORE_MESSAGES;
pub use date_names::DateNamesHandle;
pub use delivery::CriticalSubscription;
pub use delivery_loss::{ConsumerLoss, DeliveryTier, LossCount, LossWatch};
pub use donations::{
    CatchUpWaker, DONATION_CATCH_UP, DonationAudience, DonationIngest, DonationOverlayAudience,
    TestDonationRunner, register_donation_sub_actions,
};
pub use engine_voice_ref::{EngineVoiceError, EngineVoiceRef};
pub use event_log_bridge::spawn_event_log_bridge;
pub use first_chat_ledger::FirstChatLedger;
pub use global_mutation::RuntimeScriptGlobals;
pub use integration_gate::IntegrationGate;
pub use integration_start_error::{IntegrationStartError, StartFailureCategory};
pub use integration_supervisor::{
    DEFAULT_TEARDOWN_GRACE, IntegrationFactory, IntegrationObjects, IntegrationSlot,
    IntegrationSupervisor, LifecycleState, LifecycleStates, LifecycleWatch, RunningIntegration,
    TaskGroup,
};
pub use latest::{
    LatestResetError, LatestValues, register_latest_sub_actions, spawn_latest_projector,
};
pub use latest_overlay_feed::spawn_latest_overlay_feed;
pub use lifecycle_messages::LIFECYCLE_MESSAGES;
pub use live_viewers::{LiveViewerAggregatorHandle, LiveViewerCount, spawn_live_viewer_aggregator};
pub use notification_policy::{
    ActionFailureNotice, NotificationIntent, NotificationIntents, NotificationPolicyHandle,
    NotificationPreferences, spawn_notification_policy,
};
pub use overlay_definition_revision::OverlayDefinitionChanges;
pub use overlay_media::OverlayMediaLibrary;
pub use overlay_service::{
    EnabledOverlay, MaterializePass, OVERLAY_NAME_KEY, OVERLAY_SPEECH_FAILED_KIND,
    OverlayConnectFanout, OverlayConnectListener, OverlayDelivery, OverlayDispatch,
    OverlayFrameSink, OverlayReceivers, OverlayServiceCell, OverlayServiceError,
    OverlayServiceHandle, SPEECH_FAILURE_ERROR_KEY, SPEECH_FAILURE_MESSAGE_KEY, TestFire,
};
pub use overlay_shows::{
    SHOW_CEILING, SHOW_QUEUE_CAPACITY, SHOW_SPEECH_START_WAIT, ShowDepthWatch, ShowEnd, ShowTicket,
};
pub use queue_depth::{QueueDepth, QueueDepthWatch, QueueDepths};
pub use queue_scheduler::{
    MAX_PENDING_PER_QUEUE, MembershipOutcome, QUEUE_DRAINING_REASON, QUEUE_NOT_FOUND_REASON,
    QUEUE_OVERFLOW_REASON, QUEUE_PAUSED_REASON, QueueAdmission, QueueIntake, QueueMode,
    QueueModeChanged, QueueProcessing, QueueRuntimeState, QueueScheduler, QueueSchedulerHandle,
    SchedulerCell, SchedulerError, SchedulerRequest,
};
pub use scheduled_runs::{
    CatchUpSettle, HandOff, ScheduleDue, ScheduleError, ScheduleIntent, ScheduleRequest,
    ScheduledPlacement, ScheduledRunsCell, ScheduledRunsHandle, ScheduledRunsParts,
    SchedulingContext, ScriptScheduling, SystemWallClock, WaitingForQueueWatch, WaitingRuns,
    WallClock, spawn_scheduled_runs,
};
pub use script_registry::{CompiledScript, ScriptRegistry, ScriptRegistryError};
pub use speak_dispatcher::{
    ShowSpeech, SpeakDispatchError, SpeakDispatcher, SpeakingViewer, SpeechOrigin,
    SpeechStartSignal, VoiceDescriptor,
};
pub use stream_live::{LiveSource, StreamLiveHandle, StreamLiveState, spawn_stream_live_signal};
pub use sub_action_runners::{
    CONTENT_SCHEMA_KEY, CoreSubActionDeps, OVERLAY_SEND_KIND_ID, OVERLAY_TARGET_KEY,
    OverlaySendTarget, feeds_overlay, overlay_send_targets, register_core_sub_actions,
    register_scheduled_run_sub_actions,
};
pub use task_stop::TaskStop;
pub use timer_scheduler::{TimerSchedulerHandle, spawn_timer_scheduler};
pub use trigger_evaluator::{
    COMMAND_LINE_TARGET, TRIGGER_BLOCKED_KIND, TriggerEvaluatorHandle, spawn_trigger_evaluator,
};
pub use triggers::register_core_triggers;
pub use twitch_emote_lexicon::{TwitchEmoteLexicon, spawn_twitch_emote_learning};
pub use viewer_tracker::spawn_viewer_tracker;
