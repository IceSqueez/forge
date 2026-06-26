mod core_file_read;
mod core_globals_delete;
mod core_globals_get;
mod core_globals_increment;
mod core_globals_set;
mod core_log_write;
mod core_logic_wait;
mod core_random_int;
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
mod interpolate;
mod script_run_inline;
mod script_run_named;
mod twitch_chat_send_message;

pub use core_file_read::CoreFileReadRunner;
pub use core_globals_delete::CoreGlobalsDeleteRunner;
pub use core_globals_get::CoreGlobalsGetRunner;
pub use core_globals_increment::CoreGlobalsIncrementRunner;
pub use core_globals_set::CoreGlobalsSetRunner;
pub use core_log_write::CoreLogWriteRunner;
pub use core_logic_wait::CoreLogicWaitRunner;
pub use core_random_int::CoreRandomIntRunner;
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
    reg.register(Box::new(CoreRandomIntRunner::new(Arc::clone(&globals))))?;
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
    Ok(())
}
