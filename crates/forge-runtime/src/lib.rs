mod chat;
mod config;
mod core_messages;
pub mod delivery;
mod engine;
mod error;
mod event_flow;
mod integrations;
mod overlays;
mod runtime_builder;
mod services;
pub mod settings;
mod speech;
mod sub_action_runners;
#[cfg(test)]
mod test_support;

pub use chat::chat_history_persistence::{
    ChatHistoryRetentionHandle, spawn_chat_history_persistence,
};
pub use chat::chat_send_router::spawn_chat_send_router;
pub use chat::first_chat_ledger::FirstChatLedger;
pub use chat::live_viewers::{
    LiveViewerAggregatorHandle, LiveViewerCount, spawn_live_viewer_aggregator,
};
pub use chat::stream_live::spawn_stream_live_signal;
pub use chat::twitch_emote_lexicon::{TwitchEmoteLexicon, spawn_twitch_emote_learning};
pub use chat::viewer_tracker::spawn_viewer_tracker;
pub use config::Config;
pub use core_messages::{CORE_MESSAGES, script_input_failure};
pub use engine::action_cancel::ActionCancelRegistry;
pub use engine::action_engine::{
    ActionEngineHandle, ActionEngineParts, ExecutionRequest, PendingQuickAction,
    spawn_action_engine,
};
pub use engine::chain::{ChainEngine, ChainRun};
pub use engine::condition::ConditionGate;
pub use engine::queue_depth::{
    QueueDepth, QueueDepthWatch, QueueDepths, QueueRunning, RunningAction,
};
pub use engine::queue_scheduler::{
    MAX_PENDING_PER_QUEUE, MembershipOutcome, QueueAdmission, QueueIntake, QueueMode,
    QueueModeChanged, QueueProcessing, QueueRuntimeState, QueueScheduler, QueueSchedulerHandle,
    SchedulerRequest,
};
pub use engine::scheduled_runs::{
    CatchUpSettle, HandOff, ScheduleDue, ScheduleRequest, ScheduledRunsHandle, ScheduledRunsParts,
    SchedulingContext, WaitingForQueueWatch, WaitingRuns, spawn_scheduled_runs,
};
pub use engine::timer_scheduler::{TimerSchedulerHandle, spawn_timer_scheduler};
pub use engine::trigger_evaluator::{
    COMMAND_LINE_TARGET, TRIGGER_BLOCKED_KIND, TriggerEvaluatorHandle, spawn_trigger_evaluator,
};
pub use engine::triggers::register_core_triggers;
pub use error::RuntimeError;
pub use event_flow::bus::{EventBus, EventHistory, EventStamp};
pub use event_flow::delivery_loss::{ConsumerLoss, DeliveryTier, LossCount, LossWatch};
pub use event_flow::event_log_bridge::spawn_event_log_bridge;
#[cfg(any(test, feature = "test-support"))]
#[cfg(any(test, feature = "test-support"))]
pub use event_flow::null_event_log::NullEventLogRepo;
pub use integrations::integration_gate::IntegrationGate;
pub use integrations::integration_start_error::{IntegrationStartError, StartFailureCategory};
pub use integrations::integration_supervisor::{
    DEFAULT_TEARDOWN_GRACE, IntegrationFactory, IntegrationObjects, IntegrationSlot,
    IntegrationSupervisor, LifecycleState, LifecycleStates, RunningIntegration, TaskGroup,
};
pub use integrations::notification_policy::{
    ActionFailureNotice, NotificationIntent, NotificationIntents, NotificationPolicyHandle,
    NotificationPreferences, spawn_notification_policy,
};
pub use overlays::chat_avatars::{
    AVATAR_BATCH_SPACING, AVATAR_BATCH_WINDOW, ChatAvatars, KNOWN_AVATARS_CAPACITY, KnownAvatar,
    WAITING_AVATARS_CAPACITY,
};
pub use overlays::chat_removal_feed::{removal_of, spawn_chat_removal_feed};
pub use overlays::latest_overlay_feed::spawn_latest_overlay_feed;
pub use overlays::overlay_definition_revision::OverlayDefinitionChanges;
pub use overlays::overlay_media::OverlayMediaLibrary;
pub use overlays::overlay_samples::{OverlaySampleSource, OverlaySamples};
pub use overlays::overlay_service::{
    MaterializePass, OVERLAY_NAME_KEY, OVERLAY_SPEECH_FAILED_KIND, OverlayConnectFanout,
    OverlayConnectListener, OverlayDelivery, OverlayDispatch, OverlayFrameSink, OverlayReceivers,
    OverlayServiceHandle, OverlayServiceParts, SPEECH_FAILURE_ERROR_KEY,
    SPEECH_FAILURE_MESSAGE_KEY, TestFire,
};
pub use overlays::overlay_shows::{
    SHOW_CEILING, SHOW_QUEUE_CAPACITY, SHOW_SPEECH_START_WAIT, ShowDepthWatch, ShowEnd, ShowTicket,
};
pub use runtime_builder::{FrozenRuntime, RuntimeWiring, RuntimeWiringParts};
pub use services::catalog::Catalog;
pub use services::date_names::DateNamesHandle;
pub use services::donations::{
    DonationAudience, DonationIngest, DonationOverlayAudience, register_donation_sub_actions,
};
pub use services::global_mutation::RuntimeScriptGlobals;
pub use services::latest::{LatestValues, register_latest_sub_actions, spawn_latest_projector};
pub use services::lifecycle_messages::LIFECYCLE_MESSAGES;
pub use services::script_catalog::{ScriptApiDeps, ScriptCatalog};
pub use speech::audio_runners::register_speech_sub_actions;
pub use speech::engine_voice_ref::EngineVoiceRef;
pub use speech::speak_dispatcher::{
    ShowSpeaker, ShowSpeech, SpeakDispatcher, SpeakQueueControl, SpeakingViewer, SpeechOrigin,
    SpeechPorts, SpeechRequest, SpeechStartSignal, SpeechVoice, VoiceAliasWriter, VoiceDescriptor,
};
pub use sub_action_runners::{
    CONTENT_SCHEMA_KEY, OVERLAY_SEND_KIND_ID, OVERLAY_TARGET_KEY, OverlaySendTarget, feeds_overlay,
    overlay_send_targets, register_core_sub_actions, register_scheduled_run_sub_actions,
};

pub use chat::stream_live::LiveSource;
pub use chat::stream_live::StreamLiveHandle;
pub use chat::stream_live::StreamLiveState;
pub use engine::action_engine::QuickActionOrigin;
pub use engine::chain::ChainScope;
pub use engine::queue_scheduler::ClearMode;
pub use engine::queue_scheduler::QueueSchedulerStart;
pub use engine::queue_service::{QueueApplyError, QueueService};
pub use engine::scheduled_runs::ScheduleIntent;
pub use engine::scheduled_runs::ScheduledPlacement;
pub use engine::scheduled_runs::ScriptScheduling;
pub use engine::scheduled_runs::{
    CATCH_UP_SETTLE_LIMIT, MAX_LATE_TOLERANCE, MAX_PENDING_SCHEDULED_RUNS,
    MAX_PENDING_SCHEDULED_RUNS_PER_ACTION, MAX_SCHEDULE_DELAY, MAX_SCHEDULE_KEY_CHARS,
    MAX_SCHEDULED_ARGS_BYTES, MIN_LATE_TOLERANCE, MIN_SCHEDULE_DELAY, SCHEDULED_AT_VARIABLE,
    SCHEDULED_DUE_AT_VARIABLE, SCHEDULED_KEY_VARIABLE, SCHEDULED_LATE_SECONDS_VARIABLE,
    SCHEDULED_RUN_DUE_KIND, WALL_CLOCK_RECHECK,
};
pub use engine::triggers::{
    DONATION_PROVIDER_OPTIONS_KEY, DonationReceivedDescriptor, TIMER_TICK_KIND, TimerSchedule,
    TimerTickDescriptor, timer_every_message,
};
pub use integrations::integration_supervisor::LifecycleWatch;
pub use overlays::overlay_service::EnabledOverlay;
pub use overlays::overlay_service::OVERLAY_TEST_FIRE_KIND;
pub use overlays::overlay_service::OverlaySendOptions;
pub use runtime_builder::ScheduledRunsWorker;
pub use services::actions::overlay_wiring::PlannedAction;
pub use services::actions::overlay_wiring::QueuePlacement;
pub use services::actions::types::ActionSummary;
pub use services::actions::{
    ActionDetail, ActionsService, OVERLAY_ALERT_QUEUE, OverlayFeed, OverlayWiringOutcome,
    OverlayWiringPlan, OverlayWiringRecords, OverlayWiringRefusal, WiredQueue, plan_overlay_wiring,
};
pub use services::catalog::CatalogBinding;
pub use services::catalog::CatalogSnapshot;
pub use services::dashboard::{DashboardStats, compute_stats};
pub use services::donations::CatchUpWaker;
pub use services::script_catalog::LoadedScript;
pub use speech::audio_runners::{
    ENGINE_OPTIONS_KEY, SPEAK_KIND_ID, SPEECH_TEXT_KEY, SpeakRunner, VOICE_OPTIONS_PREFIX,
};
pub use sub_action_runners::CoreSubActionDeps;
pub use sub_action_runners::{
    CANCEL_SCHEDULED_KIND_ID, CHAT_SEND_DEFAULT_TARGET, CHAT_SEND_KIND_ID, CHAT_SEND_MESSAGE_KEY,
    CHAT_SEND_TARGET_KEY, ChatSendMessageRunner, ClipboardPort, CoreActionCancelRunner,
    CoreActionRunRunner, CoreActionStateRunner, CoreArgsSetRunner, CoreFileDeleteRunner,
    CoreFileListRunner, CoreFileReadRunner, CoreFileWriteRunner, CoreLogicBreakLoopRunner,
    CoreLogicContinueLoopRunner, CoreLogicIfThenElseRunner, CoreLogicLoopRunner,
    CoreLogicStopRunner, CoreLogicSwitchCaseRunner, CoreLogicWaitRunner, CoreLogicWaitUntilRunner,
    CoreMathEvaluateRunner, CoreStringConcatRunner, CoreStringFormatRunner, CoreStringLengthRunner,
    CoreStringLowercaseRunner, CoreStringRegexMatchRunner, CoreStringReplaceRunner,
    CoreStringSplitRunner, CoreStringSubstringRunner, CoreStringTitlecaseRunner,
    CoreStringTrimRunner, CoreStringUppercaseRunner, CoreTimeFormatRunner, CoreTimeNowRunner,
    CoreTimeParseRunner, CoreTriggerStateRunner, CoreUsersGetVarRunner,
    CoreUsersIncrementVarRunner, CoreUsersSetVarRunner, DesktopNotice, FILE_READ_KIND_ID,
    FILE_READ_PATH_KEY, FILE_READ_TARGET_VAR_KEY, GLOBAL_AMOUNT_KEY, GLOBAL_DEFAULT_AMOUNT,
    GLOBAL_INCREMENT_KIND_ID, GLOBAL_NAME_KEY, GLOBAL_SET_KIND_ID, GLOBAL_VALUE_KEY,
    LOG_DEFAULT_LEVEL, LOG_LEVEL_KEY, LOG_MESSAGE_KEY, LOG_WRITE_KIND_ID, NotifyPort,
    NotifyUrgency, OsPorts, RANDOM_INT_KIND_ID, RANDOM_INT_MAX_KEY, RANDOM_INT_MIN_KEY,
    RANDOM_INT_TARGET_VAR_KEY, SCHEDULE_ACTION_KIND_ID, SCRIPT_TARGET_VAR_KEY,
    ScriptRunInlineRunner, ServerBroadcastRunner, UrlOpenPort, WAIT_KIND_ID, WAIT_MS_KEY,
};
pub use sub_action_runners::{
    DEFAULT_CHAIN_KEY, ELSE_CHAIN_KEY, LOOP_BODY_KEY, MAX_FOREACH_ITERATIONS,
    MAX_RECORDED_ITERATIONS, THEN_CHAIN_KEY,
};
