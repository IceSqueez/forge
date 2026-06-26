mod core_file_delete;
mod core_file_list;
mod core_file_read;
mod core_file_write;
mod core_globals_delete;
mod core_globals_get;
mod core_globals_increment;
mod core_globals_set;
mod core_log_write;
mod core_logic_wait;
mod core_math_evaluate;
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
mod core_time_add;
mod core_time_diff;
mod core_time_format;
mod core_time_now;
mod core_time_parse;
mod file_sandbox;
mod interpolate;
mod script_run_inline;
mod script_run_named;
mod twitch_chat_send_message;

pub use core_file_delete::CoreFileDeleteRunner;
pub use core_file_list::CoreFileListRunner;
pub use core_file_read::CoreFileReadRunner;
pub use core_file_write::CoreFileWriteRunner;
pub use core_globals_delete::CoreGlobalsDeleteRunner;
pub use core_globals_get::CoreGlobalsGetRunner;
pub use core_globals_increment::CoreGlobalsIncrementRunner;
pub use core_globals_set::CoreGlobalsSetRunner;
pub use core_log_write::CoreLogWriteRunner;
pub use core_logic_wait::CoreLogicWaitRunner;
pub use core_math_evaluate::CoreMathEvaluateRunner;
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
pub use core_time_add::CoreTimeAddRunner;
pub use core_time_diff::CoreTimeDiffRunner;
pub use core_time_format::CoreTimeFormatRunner;
pub use core_time_now::CoreTimeNowRunner;
pub use core_time_parse::CoreTimeParseRunner;
pub use script_run_inline::ScriptRunInlineRunner;
pub use script_run_named::ScriptRunNamedRunner;
pub use twitch_chat_send_message::TwitchChatSendMessageRunner;

use std::sync::Arc;

use forge_events::EventPublisher;
use forge_registry::{RegistryError, SubActionRegistry};
use forge_storage::{GlobalsRepo, SettingsRepo};

use crate::script_registry::ScriptRegistry;

pub fn register_core_sub_actions(
    reg: &mut SubActionRegistry,
    globals: Arc<dyn GlobalsRepo>,
    scripts: Arc<ScriptRegistry>,
    publisher: Arc<dyn EventPublisher>,
    settings: Arc<dyn SettingsRepo>,
) -> Result<(), RegistryError> {
    reg.register(Box::new(CoreGlobalsSetRunner::new(Arc::clone(&globals))))?;
    reg.register(Box::new(CoreGlobalsGetRunner::new(Arc::clone(&globals))))?;
    reg.register(Box::new(CoreGlobalsIncrementRunner::new(Arc::clone(
        &globals,
    ))))?;
    reg.register(Box::new(CoreGlobalsDeleteRunner::new(Arc::clone(&globals))))?;
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
        settings,
    )))?;
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
    Ok(())
}
