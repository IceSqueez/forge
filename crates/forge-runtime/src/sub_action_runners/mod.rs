mod core_action_disable;
mod core_action_enable;
mod core_action_toggle;
mod core_args_set;
mod core_clipboard_copy;
mod core_clipboard_read;
mod core_file_delete;
mod core_file_list;
mod core_file_read;
mod core_file_write;
mod core_globals_array_append;
mod core_globals_array_remove;
mod core_globals_decrement;
mod core_globals_delete;
mod core_globals_get;
mod core_globals_increment;
mod core_globals_set;
mod core_globals_toggle;
mod core_http;
mod core_log_write;
mod core_logic_wait;
mod core_math_evaluate;
mod core_notify_show;
mod core_queue_clear;
mod core_queue_pause;
mod core_queue_resume;
mod core_queue_shared;
mod core_random_bool;
mod core_random_float;
mod core_random_int;
mod core_random_pick;
mod core_string_concat;
mod core_string_format;
mod core_string_length;
mod core_string_lowercase;
mod core_string_regex_match;
mod core_string_replace;
mod core_string_split;
mod core_string_substring;
mod core_string_titlecase;
mod core_string_trim;
mod core_string_uppercase;
mod core_test_fire_trigger;
mod core_time_add;
mod core_time_diff;
mod core_time_format;
mod core_time_now;
mod core_time_parse;
mod core_trigger_disable;
mod core_trigger_enable;
mod core_trigger_toggle;
mod core_url_open;
mod core_users_get_var;
mod core_users_increment_var;
mod core_users_set_var;
mod core_users_shared;
mod file_sandbox;
mod interpolate;
mod os_ports;
mod script_emit_event;
mod script_run_inline;
mod script_run_named;
mod server_broadcast;
mod twitch_chat_send_message;

pub use core_action_disable::CoreActionDisableRunner;
pub use core_action_enable::CoreActionEnableRunner;
pub use core_action_toggle::CoreActionToggleRunner;
pub use core_args_set::CoreArgsSetRunner;
pub use core_clipboard_copy::CoreClipboardCopyRunner;
pub use core_clipboard_read::CoreClipboardReadRunner;
pub use core_file_delete::CoreFileDeleteRunner;
pub use core_file_list::CoreFileListRunner;
pub use core_file_read::CoreFileReadRunner;
pub use core_file_write::CoreFileWriteRunner;
pub use core_globals_array_append::CoreGlobalsArrayAppendRunner;
pub use core_globals_array_remove::CoreGlobalsArrayRemoveRunner;
pub use core_globals_decrement::CoreGlobalsDecrementRunner;
pub use core_globals_delete::CoreGlobalsDeleteRunner;
pub use core_globals_get::CoreGlobalsGetRunner;
pub use core_globals_increment::CoreGlobalsIncrementRunner;
pub use core_globals_set::CoreGlobalsSetRunner;
pub use core_globals_toggle::CoreGlobalsToggleRunner;
pub use core_http::CoreHttpRunner;
pub use core_log_write::CoreLogWriteRunner;
pub use core_logic_wait::CoreLogicWaitRunner;
pub use core_math_evaluate::CoreMathEvaluateRunner;
pub use core_notify_show::CoreNotifyShowRunner;
pub use core_queue_clear::CoreQueueClearRunner;
pub use core_queue_pause::CoreQueuePauseRunner;
pub use core_queue_resume::CoreQueueResumeRunner;
pub use core_random_bool::CoreRandomBoolRunner;
pub use core_random_float::CoreRandomFloatRunner;
pub use core_random_int::CoreRandomIntRunner;
pub use core_random_pick::CoreRandomPickRunner;
pub use core_string_concat::CoreStringConcatRunner;
pub use core_string_format::CoreStringFormatRunner;
pub use core_string_length::CoreStringLengthRunner;
pub use core_string_lowercase::CoreStringLowercaseRunner;
pub use core_string_regex_match::CoreStringRegexMatchRunner;
pub use core_string_replace::CoreStringReplaceRunner;
pub use core_string_split::CoreStringSplitRunner;
pub use core_string_substring::CoreStringSubstringRunner;
pub use core_string_titlecase::CoreStringTitlecaseRunner;
pub use core_string_trim::CoreStringTrimRunner;
pub use core_string_uppercase::CoreStringUppercaseRunner;
pub use core_test_fire_trigger::CoreTestFireTriggerRunner;
pub use core_time_add::CoreTimeAddRunner;
pub use core_time_diff::CoreTimeDiffRunner;
pub use core_time_format::CoreTimeFormatRunner;
pub use core_time_now::CoreTimeNowRunner;
pub use core_time_parse::CoreTimeParseRunner;
pub use core_trigger_disable::CoreTriggerDisableRunner;
pub use core_trigger_enable::CoreTriggerEnableRunner;
pub use core_trigger_toggle::CoreTriggerToggleRunner;
pub use core_url_open::CoreUrlOpenRunner;
pub use core_users_get_var::CoreUsersGetVarRunner;
pub use core_users_increment_var::CoreUsersIncrementVarRunner;
pub use core_users_set_var::CoreUsersSetVarRunner;
pub use os_ports::{
    ClipboardPort, DesktopNotice, NotifyPort, NotifyUrgency, OsPortError, SystemClipboardPort,
    SystemNotifyPort, SystemUrlOpenPort, UrlOpenPort,
};
pub use script_emit_event::ScriptEmitEventRunner;
pub use script_run_inline::ScriptRunInlineRunner;
pub use script_run_named::ScriptRunNamedRunner;
pub use server_broadcast::ServerBroadcastRunner;
pub use twitch_chat_send_message::TwitchChatSendMessageRunner;

use std::sync::Arc;

use forge_events::EventPublisher;
use forge_registry::{RegistryError, SubActionRegistry};
use forge_storage::{ActionRepo, GlobalsRepo, SettingsRepo, TriggerInstanceRepo, UserGlobalsRepo};

use crate::SchedulerCell;
use crate::egress::{EgressClient, HttpMethod};
use crate::script_registry::ScriptRegistry;

#[allow(clippy::too_many_arguments)]
pub fn register_core_sub_actions(
    reg: &mut SubActionRegistry,
    globals: Arc<dyn GlobalsRepo>,
    user_globals: Arc<dyn UserGlobalsRepo>,
    scripts: Arc<ScriptRegistry>,
    publisher: Arc<dyn EventPublisher>,
    settings: Arc<dyn SettingsRepo>,
    scheduler: SchedulerCell,
    trigger_instances: Arc<dyn TriggerInstanceRepo>,
    actions: Arc<dyn ActionRepo>,
) -> Result<(), RegistryError> {
    reg.register(Box::new(CoreArgsSetRunner))?;
    reg.register(Box::new(CoreQueuePauseRunner::new(scheduler.clone())))?;
    reg.register(Box::new(CoreQueueResumeRunner::new(scheduler.clone())))?;
    reg.register(Box::new(CoreQueueClearRunner::new(scheduler.clone())))?;
    reg.register(Box::new(CoreActionEnableRunner::new(Arc::clone(&actions))))?;
    reg.register(Box::new(CoreActionDisableRunner::new(Arc::clone(&actions))))?;
    reg.register(Box::new(CoreActionToggleRunner::new(Arc::clone(&actions))))?;
    reg.register(Box::new(CoreTriggerEnableRunner::new(Arc::clone(
        &trigger_instances,
    ))))?;
    reg.register(Box::new(CoreTriggerDisableRunner::new(Arc::clone(
        &trigger_instances,
    ))))?;
    reg.register(Box::new(CoreTriggerToggleRunner::new(Arc::clone(
        &trigger_instances,
    ))))?;
    reg.register(Box::new(CoreTestFireTriggerRunner::new(
        trigger_instances,
        actions,
        scheduler,
    )))?;
    reg.register(Box::new(CoreGlobalsSetRunner::new(Arc::clone(&globals))))?;
    reg.register(Box::new(CoreGlobalsGetRunner::new(Arc::clone(&globals))))?;
    reg.register(Box::new(CoreGlobalsIncrementRunner::new(Arc::clone(
        &globals,
    ))))?;
    reg.register(Box::new(CoreGlobalsDecrementRunner::new(Arc::clone(
        &globals,
    ))))?;
    reg.register(Box::new(CoreGlobalsToggleRunner::new(Arc::clone(&globals))))?;
    reg.register(Box::new(CoreGlobalsArrayAppendRunner::new(Arc::clone(
        &globals,
    ))))?;
    reg.register(Box::new(CoreGlobalsArrayRemoveRunner::new(Arc::clone(
        &globals,
    ))))?;
    reg.register(Box::new(CoreGlobalsDeleteRunner::new(Arc::clone(&globals))))?;
    reg.register(Box::new(CoreUsersGetVarRunner::new(
        Arc::clone(&globals),
        Arc::clone(&user_globals),
    )))?;
    reg.register(Box::new(CoreUsersSetVarRunner::new(
        Arc::clone(&globals),
        Arc::clone(&user_globals),
    )))?;
    reg.register(Box::new(CoreUsersIncrementVarRunner::new(
        Arc::clone(&globals),
        user_globals,
    )))?;
    reg.register(Box::new(CoreLogicWaitRunner))?;
    reg.register(Box::new(CoreLogWriteRunner::new(Arc::clone(&globals))))?;
    reg.register(Box::new(CoreFileReadRunner::new(Arc::clone(&globals))))?;
    reg.register(Box::new(CoreFileWriteRunner::new(Arc::clone(&globals))))?;
    reg.register(Box::new(CoreFileDeleteRunner::new(Arc::clone(&globals))))?;
    reg.register(Box::new(CoreFileListRunner::new(Arc::clone(&globals))))?;
    reg.register(Box::new(CoreRandomIntRunner::new(Arc::clone(&globals))))?;
    reg.register(Box::new(CoreRandomFloatRunner::new(Arc::clone(&globals))))?;
    reg.register(Box::new(CoreRandomBoolRunner::new(Arc::clone(&globals))))?;
    reg.register(Box::new(CoreRandomPickRunner::new(Arc::clone(&globals))))?;
    reg.register(Box::new(CoreMathEvaluateRunner::new()))?;
    reg.register(Box::new(TwitchChatSendMessageRunner::new(Arc::clone(
        &globals,
    ))))?;
    reg.register(Box::new(ScriptRunNamedRunner::new(
        Arc::clone(&scripts),
        Arc::clone(&globals),
        Arc::clone(&publisher),
        Arc::clone(&settings),
    )))?;
    reg.register(Box::new(ScriptRunInlineRunner::new(
        Arc::clone(&scripts),
        Arc::clone(&globals),
        Arc::clone(&publisher),
        Arc::clone(&settings),
    )))?;
    reg.register(Box::new(ScriptEmitEventRunner::new(Arc::clone(&publisher))))?;
    reg.register(Box::new(ServerBroadcastRunner::new(Arc::clone(&publisher))))?;
    reg.register(Box::new(CoreStringConcatRunner))?;
    reg.register(Box::new(CoreStringSubstringRunner))?;
    reg.register(Box::new(CoreStringReplaceRunner))?;
    reg.register(Box::new(CoreStringLowercaseRunner))?;
    reg.register(Box::new(CoreStringUppercaseRunner))?;
    reg.register(Box::new(CoreStringTitlecaseRunner))?;
    reg.register(Box::new(CoreStringTrimRunner))?;
    reg.register(Box::new(CoreStringSplitRunner))?;
    reg.register(Box::new(CoreStringLengthRunner))?;
    reg.register(Box::new(CoreStringRegexMatchRunner))?;
    reg.register(Box::new(CoreStringFormatRunner))?;
    reg.register(Box::new(CoreTimeNowRunner))?;
    reg.register(Box::new(CoreTimeFormatRunner))?;
    reg.register(Box::new(CoreTimeDiffRunner))?;
    reg.register(Box::new(CoreTimeAddRunner))?;
    reg.register(Box::new(CoreTimeParseRunner))?;
    reg.register(Box::new(CoreNotifyShowRunner::new(Arc::new(
        SystemNotifyPort,
    ))))?;
    reg.register(Box::new(CoreClipboardCopyRunner::new(Arc::new(
        SystemClipboardPort,
    ))))?;
    reg.register(Box::new(CoreClipboardReadRunner::new(Arc::new(
        SystemClipboardPort,
    ))))?;
    reg.register(Box::new(CoreUrlOpenRunner::new(Arc::new(
        SystemUrlOpenPort,
    ))))?;

    let egress =
        Arc::new(EgressClient::new().map_err(|e| RegistryError::RunnerInit(e.to_string()))?);
    for method in [
        HttpMethod::Get,
        HttpMethod::Post,
        HttpMethod::Put,
        HttpMethod::Patch,
        HttpMethod::Delete,
    ] {
        reg.register(Box::new(CoreHttpRunner::new(
            method,
            Arc::clone(&globals),
            Arc::clone(&settings),
            Arc::clone(&egress),
        )))?;
    }
    Ok(())
}
