mod action_cancel;
mod action_engine;
mod actions;
mod audio_runners;
mod bus;
mod catalog;
mod chain;
mod chat_history_persistence;
mod chat_send_router;
mod chat_stream;
mod condition;
mod config;
mod cooldown;
mod core_messages;
mod dashboard;
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
#[cfg(any(test, feature = "test-support"))]
mod null_event_log;
mod overlay_definition_revision;
mod overlay_lanes;
mod overlay_media;
mod overlay_samples;
mod overlay_service;
mod overlay_shows;
mod own_chat_echoes;
mod persist_batch;
mod queue_depth;
mod queue_scheduler;
mod run_history;
mod runtime_builder;
mod scheduled_runs;
mod script_registry;
pub mod settings;
mod speak_dispatcher;
mod stream_live;
mod sub_action_runners;
mod task_stop;
#[cfg(test)]
mod test_support;
mod timer_scheduler;
mod trigger_evaluator;
mod triggers;
mod twitch_emote_lexicon;
mod viewer_tracker;

pub use action_cancel::ActionCancelRegistry;
pub use action_engine::{
    ActionEngineHandle, DispatchError, ExecutionRequest, PendingQuickAction, spawn_action_engine,
};
pub use audio_runners::register_speech_sub_actions;
pub use bus::{BusError, EventBus, EventHistory};
pub use catalog::Catalog;
pub use chain::{ChainEngine, ChainRun};
pub use chat_history_persistence::{ChatHistoryRetentionHandle, spawn_chat_history_persistence};
pub use chat_send_router::spawn_chat_send_router;
pub use condition::ConditionGate;
pub use config::Config;
pub use core_messages::CORE_MESSAGES;
pub use date_names::DateNamesHandle;
pub use delivery_loss::{ConsumerLoss, DeliveryTier, LossCount, LossWatch};
pub use donations::{
    DonationAudience, DonationIngest, DonationOverlayAudience, register_donation_sub_actions,
};
pub use engine_voice_ref::{EngineVoiceError, EngineVoiceRef};
pub use event_log_bridge::spawn_event_log_bridge;
pub use first_chat_ledger::FirstChatLedger;
pub use global_mutation::RuntimeScriptGlobals;
pub use integration_gate::IntegrationGate;
pub use integration_start_error::{IntegrationStartError, StartFailureCategory};
pub use integration_supervisor::{
    DEFAULT_TEARDOWN_GRACE, IntegrationFactory, IntegrationObjects, IntegrationSlot,
    IntegrationSupervisor, LifecycleState, LifecycleStates, RunningIntegration, TaskGroup,
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
#[cfg(any(test, feature = "test-support"))]
pub use null_event_log::NullEventLogRepo;
pub use overlay_definition_revision::OverlayDefinitionChanges;
pub use overlay_media::OverlayMediaLibrary;
pub use overlay_samples::{OverlaySampleSource, OverlaySamples};
pub use overlay_service::{
    MaterializePass, OVERLAY_NAME_KEY, OVERLAY_SPEECH_FAILED_KIND, OverlayConnectFanout,
    OverlayConnectListener, OverlayDelivery, OverlayDispatch, OverlayFrameSink, OverlayReceivers,
    OverlayServiceError, OverlayServiceHandle, OverlayServiceParts, SPEECH_FAILURE_ERROR_KEY,
    SPEECH_FAILURE_MESSAGE_KEY, TestFire,
};
pub use overlay_shows::{
    SHOW_CEILING, SHOW_QUEUE_CAPACITY, SHOW_SPEECH_START_WAIT, ShowDepthWatch, ShowEnd, ShowTicket,
};
pub use queue_depth::{QueueDepth, QueueDepthWatch, QueueDepths, QueueRunning, RunningAction};
pub use queue_scheduler::{
    ENGINE_STOPPED_REASON, MAX_PENDING_PER_QUEUE, MembershipOutcome, QUEUE_DELETED_REASON,
    QUEUE_DRAINING_REASON, QUEUE_NOT_FOUND_REASON, QUEUE_OVERFLOW_REASON, QUEUE_PAUSED_REASON,
    QueueAdmission, QueueIntake, QueueMode, QueueModeChanged, QueueProcessing, QueueRuntimeState,
    QueueScheduler, QueueSchedulerHandle, SchedulerError, SchedulerRequest,
};
pub use runtime_builder::{FrozenRuntime, RuntimeWiring, RuntimeWiringParts};
pub use scheduled_runs::{
    CatchUpSettle, HandOff, ScheduleDue, ScheduleError, ScheduleRequest, ScheduledRunsHandle,
    ScheduledRunsParts, SchedulingContext, WaitingForQueueWatch, WaitingRuns, spawn_scheduled_runs,
};
pub use script_registry::{ScriptRegistry, ScriptRegistryError, ScriptRegistryParts};
pub use speak_dispatcher::{
    ShowSpeech, SpeakDispatchError, SpeakDispatcher, SpeakingViewer, SpeechOrigin,
    SpeechStartSignal, VoiceDescriptor,
};
pub use stream_live::spawn_stream_live_signal;
pub use sub_action_runners::{
    CONTENT_SCHEMA_KEY, OVERLAY_SEND_KIND_ID, OVERLAY_TARGET_KEY, OverlaySendTarget, feeds_overlay,
    overlay_send_targets, register_core_sub_actions, register_scheduled_run_sub_actions,
};
pub use timer_scheduler::{TimerSchedulerHandle, spawn_timer_scheduler};
pub use trigger_evaluator::{
    COMMAND_LINE_TARGET, TRIGGER_BLOCKED_KIND, TriggerEvaluatorHandle, spawn_trigger_evaluator,
};
pub use triggers::register_core_triggers;
pub use twitch_emote_lexicon::{TwitchEmoteLexicon, spawn_twitch_emote_learning};
pub use viewer_tracker::spawn_viewer_tracker;

pub use actions::overlay_wiring::PlannedAction;
pub use actions::overlay_wiring::QueuePlacement;
pub use actions::types::ActionSummary;
pub use actions::{
    ActionDetail, ActionsService, OVERLAY_ALERT_QUEUE, OverlayFeed, OverlayWiringError,
    OverlayWiringOutcome, OverlayWiringPlan, OverlayWiringRecords, OverlayWiringRefusal,
    WiredQueue, plan_overlay_wiring,
};
pub use audio_runners::{
    ENGINE_OPTIONS_KEY, SPEAK_KIND_ID, SPEECH_TEXT_KEY, SpeakRunner, VOICE_OPTIONS_PREFIX,
};
pub use catalog::CatalogBinding;
pub use catalog::CatalogSnapshot;
pub use chain::ChainScope;
pub use condition::ConditionError;
pub use dashboard::{DashboardStats, compute_stats};
pub use donations::CatchUpWaker;
pub use integration_supervisor::LifecycleWatch;
pub use overlay_service::EnabledOverlay;
pub use overlay_service::OVERLAY_TEST_FIRE_KIND;
pub use queue_scheduler::QueueSchedulerStart;
pub use runtime_builder::ScheduledRunsWorker;
pub use scheduled_runs::ScheduleIntent;
pub use scheduled_runs::ScheduledPlacement;
pub use scheduled_runs::ScriptScheduling;
pub use scheduled_runs::{
    ACTION_DISABLED_REASON, ACTION_NOT_FOUND_REASON, ACTIONS_UNAVAILABLE_REASON,
    CATCH_UP_SETTLE_LIMIT, MAX_LATE_TOLERANCE, MAX_PENDING_SCHEDULED_RUNS,
    MAX_PENDING_SCHEDULED_RUNS_PER_ACTION, MAX_SCHEDULE_DELAY, MAX_SCHEDULE_KEY_CHARS,
    MAX_SCHEDULED_ARGS_BYTES, MIN_LATE_TOLERANCE, MIN_SCHEDULE_DELAY, MISSED_REASON,
    QUEUES_CLOSED_REASON, SCHEDULED_AT_VARIABLE, SCHEDULED_DUE_AT_VARIABLE, SCHEDULED_KEY_VARIABLE,
    SCHEDULED_LATE_SECONDS_VARIABLE, SCHEDULED_RUN_DUE_KIND, UNREADABLE_REASON, WALL_CLOCK_RECHECK,
};
pub use script_registry::CompiledScript;
pub use stream_live::LiveSource;
pub use stream_live::StreamLiveHandle;
pub use stream_live::StreamLiveState;
pub use sub_action_runners::CoreSubActionDeps;
pub use sub_action_runners::{
    CANCEL_SCHEDULED_KIND_ID, CHAT_SEND_DEFAULT_TARGET, CHAT_SEND_KIND_ID, CHAT_SEND_MESSAGE_KEY,
    CHAT_SEND_TARGET_KEY, CoreActionCancelRunner, CoreActionDisableRunner, CoreActionEnableRunner,
    CoreActionRunRunner, CoreActionToggleRunner, CoreArgsSetRunner, CoreFileDeleteRunner,
    CoreFileListRunner, CoreFileReadRunner, CoreFileWriteRunner, CoreLogicBreakLoopRunner,
    CoreLogicContinueLoopRunner, CoreLogicIfThenElseRunner, CoreLogicLoopRunner,
    CoreLogicStopRunner, CoreLogicSwitchCaseRunner, CoreLogicWaitRunner, CoreLogicWaitUntilRunner,
    CoreMathEvaluateRunner, CoreStringConcatRunner, CoreStringFormatRunner, CoreStringLengthRunner,
    CoreStringLowercaseRunner, CoreStringRegexMatchRunner, CoreStringReplaceRunner,
    CoreStringSplitRunner, CoreStringSubstringRunner, CoreStringTitlecaseRunner,
    CoreStringTrimRunner, CoreStringUppercaseRunner, CoreTimeFormatRunner, CoreTimeNowRunner,
    CoreTimeParseRunner, CoreTriggerDisableRunner, CoreTriggerEnableRunner,
    CoreTriggerToggleRunner, CoreUsersGetVarRunner, CoreUsersIncrementVarRunner,
    CoreUsersSetVarRunner, DesktopNotice, FILE_READ_KIND_ID, FILE_READ_PATH_KEY,
    FILE_READ_TARGET_VAR_KEY, GLOBAL_AMOUNT_KEY, GLOBAL_DEFAULT_AMOUNT, GLOBAL_INCREMENT_KIND_ID,
    GLOBAL_NAME_KEY, GLOBAL_SET_KIND_ID, GLOBAL_VALUE_KEY, LOG_DEFAULT_LEVEL, LOG_LEVEL_KEY,
    LOG_MESSAGE_KEY, LOG_WRITE_KIND_ID, NotifyPort, NotifyUrgency, OsPortError, RANDOM_INT_KIND_ID,
    RANDOM_INT_MAX_KEY, RANDOM_INT_MIN_KEY, RANDOM_INT_TARGET_VAR_KEY, SCHEDULE_ACTION_KIND_ID,
    SCRIPT_TARGET_VAR_KEY, ScriptRunInlineRunner, ServerBroadcastRunner, SystemNotifyPort,
    TwitchChatSendMessageRunner, WAIT_KIND_ID, WAIT_MS_KEY, translate_legacy_time_steps,
};
pub use task_stop::TaskStop;
pub use triggers::{
    DONATION_PROVIDER_OPTIONS_KEY, DonationReceivedDescriptor, TIMER_TICK_KIND, TimerSchedule,
    TimerTickDescriptor, timer_every_message,
};
