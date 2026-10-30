use std::fmt::Display;

use forge_types::{
    ActionId, MessageCatalog, MessageCode, MessageRef, TriggerInstanceId, UpstreamDetail,
    VerbatimText,
};
use time::OffsetDateTime;

const CORE_NAMESPACE: &str = "core";

const ACTION_ARG: &str = "action";
const INSTANCE_ARG: &str = "instance";
const NAME_ARG: &str = "name";
const ACTUAL_ARG: &str = "actual";
const AMOUNT_ARG: &str = "amount";
const LIMIT_ARG: &str = "limit";
const ITEMS_ARG: &str = "items";
const FROM_ARG: &str = "from";
const TO_ARG: &str = "to";
const START_ARG: &str = "start";
const END_ARG: &str = "end";
const LENGTH_ARG: &str = "length";
const UNIT_ARG: &str = "unit";
const BASE_ARG: &str = "base";
const PATTERN_ARG: &str = "pattern";
const TEXT_ARG: &str = "text";
const FIELD_ARG: &str = "field";
const VOICE_ARG: &str = "voice";
const VARIABLE_ARG: &str = "variable";
const VARIABLES_ARG: &str = "variables";
const CHAR_ARG: &str = "char";

pub const UNKNOWN_ACTION: MessageCode = MessageCode::from_static(CORE_NAMESPACE, "unknown_action");
pub const UNKNOWN_TRIGGER_INSTANCE: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "unknown_trigger_instance");
pub const ACTION_NESTING_TOO_DEEP: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "action_nesting_too_deep");
pub const ACTION_RUN_CANCELLED: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "action_run_cancelled");
pub const ACTION_RUN_FAILED: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "action_run_failed");
pub const SCHEDULER_NOT_READY: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "scheduler_not_ready");
pub const SCHEDULER_STOPPED: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "scheduler_stopped");
pub const SCHEDULER_FAILED: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "scheduler_failed");
pub const SCHEDULE_FAILED: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "schedule_failed");
pub const STORAGE_FAILED: MessageCode = MessageCode::from_static(CORE_NAMESPACE, "storage_failed");
pub const SYSTEM_SERVICE_FAILED: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "system_service_failed");
pub const TASK_FAILED: MessageCode = MessageCode::from_static(CORE_NAMESPACE, "task_failed");
pub const HTTP_FAILED: MessageCode = MessageCode::from_static(CORE_NAMESPACE, "http_failed");
pub const CONDITION_FAILED: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "condition_failed");
pub const FIELD_REQUIRED: MessageCode = MessageCode::from_static(CORE_NAMESPACE, "field_required");
pub const FILE_PATH_EMPTY: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "file_path_empty");
pub const FILE_PATH_ABSOLUTE: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "file_path_absolute");
pub const FILE_PATH_PARENT: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "file_path_parent");
pub const FILE_PATH_ESCAPES: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "file_path_escapes");
pub const FILE_NOT_FOUND: MessageCode = MessageCode::from_static(CORE_NAMESPACE, "file_not_found");
pub const DIRECTORY_NOT_FOUND: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "directory_not_found");
pub const PATH_NOT_DIRECTORY: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "path_not_directory");
pub const PATH_NOT_FILE: MessageCode = MessageCode::from_static(CORE_NAMESPACE, "path_not_file");
pub const FILE_TOO_LARGE: MessageCode = MessageCode::from_static(CORE_NAMESPACE, "file_too_large");
pub const FILE_IO_FAILED: MessageCode = MessageCode::from_static(CORE_NAMESPACE, "file_io_failed");
pub const FILE_NOT_JSON: MessageCode = MessageCode::from_static(CORE_NAMESPACE, "file_not_json");
pub const FILE_NOT_BASE64: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "file_not_base64");
pub const FILE_CHAR_NOT_LATIN1: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "file_char_not_latin1");
pub const GLOBAL_MISSING: MessageCode = MessageCode::from_static(CORE_NAMESPACE, "global_missing");
pub const GLOBAL_NOT_ARRAY: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "global_not_array");
pub const GLOBAL_NOT_BOOL: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "global_not_bool");
pub const AMOUNT_NOT_NEGATABLE: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "amount_not_negatable");
pub const USER_VARIABLE_NOT_NUMERIC: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "user_variable_not_numeric");
pub const USER_LOGIN_UNSET: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "user_login_unset");
pub const USER_LOGIN_EMPTY: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "user_login_empty");
pub const LOOP_ITERATION_LIMIT: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "loop_iteration_limit");
pub const LOOP_ITERATIONS_UNRECORDED: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "loop_iterations_unrecorded");
pub const SUBSTRING_START_OUT_OF_RANGE: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "substring_start_out_of_range");
pub const SUBSTRING_END_OUT_OF_RANGE: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "substring_end_out_of_range");
pub const SUBSTRING_START_AFTER_END: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "substring_start_after_end");
pub const TIME_OUT_OF_RANGE: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "time_out_of_range");
pub const TIME_PATTERN_INVALID: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "time_pattern_invalid");
pub const TIME_TEXT_UNREADABLE: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "time_text_unreadable");
pub const SEARCH_PATTERN_INVALID: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "search_pattern_invalid");
pub const EXPRESSION_UNDEFINED_VARIABLES: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "expression_undefined_variables");
pub const EXPRESSION_FAILED: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "expression_failed");
pub const URL_NOT_HTTP: MessageCode = MessageCode::from_static(CORE_NAMESPACE, "url_not_http");
pub const OVERLAY_SERVICE_NOT_RUNNING: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "overlay_service_not_running");
pub const OVERLAY_SHOW_CLEARED: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "overlay_show_cleared");
pub const OVERLAY_SHOW_WITHDRAWN: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "overlay_show_withdrawn");
pub const OVERLAY_SEND_FAILED: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "overlay_send_failed");
pub const SCRIPT_NOT_FOUND: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "script_not_found");
pub const SCRIPT_FAILED: MessageCode = MessageCode::from_static(CORE_NAMESPACE, "script_failed");
pub const INLINE_SCRIPT_FAILED: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "inline_script_failed");
pub const SCRIPT_PANICKED: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "script_panicked");
pub const TEST_DONATION_NOT_ANNOUNCED: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "test_donation_not_announced");
pub const VOICE_NOT_ENGINE_VOICE: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "voice_not_engine_voice");
pub const VOICE_NOT_INSTALLED: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "voice_not_installed");

pub const CORE_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: CORE_NAMESPACE,
    codes: &[
        UNKNOWN_ACTION,
        UNKNOWN_TRIGGER_INSTANCE,
        ACTION_NESTING_TOO_DEEP,
        ACTION_RUN_CANCELLED,
        ACTION_RUN_FAILED,
        SCHEDULER_NOT_READY,
        SCHEDULER_STOPPED,
        SCHEDULER_FAILED,
        SCHEDULE_FAILED,
        STORAGE_FAILED,
        SYSTEM_SERVICE_FAILED,
        TASK_FAILED,
        HTTP_FAILED,
        CONDITION_FAILED,
        FIELD_REQUIRED,
        FILE_PATH_EMPTY,
        FILE_PATH_ABSOLUTE,
        FILE_PATH_PARENT,
        FILE_PATH_ESCAPES,
        FILE_NOT_FOUND,
        DIRECTORY_NOT_FOUND,
        PATH_NOT_DIRECTORY,
        PATH_NOT_FILE,
        FILE_TOO_LARGE,
        FILE_IO_FAILED,
        FILE_NOT_JSON,
        FILE_NOT_BASE64,
        FILE_CHAR_NOT_LATIN1,
        GLOBAL_MISSING,
        GLOBAL_NOT_ARRAY,
        GLOBAL_NOT_BOOL,
        AMOUNT_NOT_NEGATABLE,
        USER_VARIABLE_NOT_NUMERIC,
        USER_LOGIN_UNSET,
        USER_LOGIN_EMPTY,
        LOOP_ITERATION_LIMIT,
        LOOP_ITERATIONS_UNRECORDED,
        SUBSTRING_START_OUT_OF_RANGE,
        SUBSTRING_END_OUT_OF_RANGE,
        SUBSTRING_START_AFTER_END,
        TIME_OUT_OF_RANGE,
        TIME_PATTERN_INVALID,
        TIME_TEXT_UNREADABLE,
        SEARCH_PATTERN_INVALID,
        EXPRESSION_UNDEFINED_VARIABLES,
        EXPRESSION_FAILED,
        URL_NOT_HTTP,
        OVERLAY_SERVICE_NOT_RUNNING,
        OVERLAY_SHOW_CLEARED,
        OVERLAY_SHOW_WITHDRAWN,
        OVERLAY_SEND_FAILED,
        SCRIPT_NOT_FOUND,
        SCRIPT_FAILED,
        INLINE_SCRIPT_FAILED,
        SCRIPT_PANICKED,
        TEST_DONATION_NOT_ANNOUNCED,
        VOICE_NOT_ENGINE_VOICE,
        VOICE_NOT_INSTALLED,
    ],
    en_ftl: include_str!("../locales/en/core.ftl"),
    uk_ftl: include_str!("../locales/uk/core.ftl"),
};

fn verbatim(text: impl Into<String>) -> VerbatimText {
    VerbatimText::new(text)
}

fn with_cause(code: MessageCode, cause: impl Display) -> MessageRef {
    MessageRef::new(code).with_detail(UpstreamDetail::from_redacted(cause.to_string()))
}

pub(crate) fn unknown_action(action: ActionId) -> MessageRef {
    MessageRef::new(UNKNOWN_ACTION).with_arg(ACTION_ARG, verbatim(action.to_string()))
}

pub(crate) fn unknown_trigger_instance(instance: TriggerInstanceId) -> MessageRef {
    MessageRef::new(UNKNOWN_TRIGGER_INSTANCE).with_arg(INSTANCE_ARG, verbatim(instance.to_string()))
}

pub(crate) fn action_nesting_too_deep() -> MessageRef {
    MessageRef::new(ACTION_NESTING_TOO_DEEP)
}

pub(crate) fn action_run_cancelled() -> MessageRef {
    MessageRef::new(ACTION_RUN_CANCELLED)
}

pub(crate) fn action_run_failed(cause: impl Display) -> MessageRef {
    with_cause(ACTION_RUN_FAILED, cause)
}

pub(crate) fn scheduler_not_ready() -> MessageRef {
    MessageRef::new(SCHEDULER_NOT_READY)
}

pub(crate) fn scheduler_stopped() -> MessageRef {
    MessageRef::new(SCHEDULER_STOPPED)
}

pub(crate) fn scheduler_failed(cause: impl Display) -> MessageRef {
    with_cause(SCHEDULER_FAILED, cause)
}

pub(crate) fn schedule_failed(cause: impl Display) -> MessageRef {
    with_cause(SCHEDULE_FAILED, cause)
}

pub(crate) fn storage_failed(cause: impl Display) -> MessageRef {
    with_cause(STORAGE_FAILED, cause)
}

pub(crate) fn system_service_failed(cause: impl Display) -> MessageRef {
    with_cause(SYSTEM_SERVICE_FAILED, cause)
}

pub(crate) fn task_failed(cause: impl Display) -> MessageRef {
    with_cause(TASK_FAILED, cause)
}

pub(crate) fn http_failed(cause: impl Display) -> MessageRef {
    with_cause(HTTP_FAILED, cause)
}

pub(crate) fn condition_failed(cause: impl Display) -> MessageRef {
    with_cause(CONDITION_FAILED, cause)
}

pub(crate) fn field_required(field: &str) -> MessageRef {
    MessageRef::new(FIELD_REQUIRED).with_arg(FIELD_ARG, verbatim(field))
}

pub(crate) fn file_path_empty() -> MessageRef {
    MessageRef::new(FILE_PATH_EMPTY)
}

pub(crate) fn file_path_absolute() -> MessageRef {
    MessageRef::new(FILE_PATH_ABSOLUTE)
}

pub(crate) fn file_path_parent() -> MessageRef {
    MessageRef::new(FILE_PATH_PARENT)
}

pub(crate) fn file_path_escapes() -> MessageRef {
    MessageRef::new(FILE_PATH_ESCAPES)
}

pub(crate) fn file_not_found() -> MessageRef {
    MessageRef::new(FILE_NOT_FOUND)
}

pub(crate) fn directory_not_found() -> MessageRef {
    MessageRef::new(DIRECTORY_NOT_FOUND)
}

pub(crate) fn path_not_directory() -> MessageRef {
    MessageRef::new(PATH_NOT_DIRECTORY)
}

pub(crate) fn path_not_file() -> MessageRef {
    MessageRef::new(PATH_NOT_FILE)
}

pub(crate) fn file_too_large(limit: u64) -> MessageRef {
    MessageRef::new(FILE_TOO_LARGE).with_arg(LIMIT_ARG, limit)
}

pub(crate) fn file_io_failed(cause: impl Display) -> MessageRef {
    with_cause(FILE_IO_FAILED, cause)
}

pub(crate) fn file_not_json(cause: impl Display) -> MessageRef {
    with_cause(FILE_NOT_JSON, cause)
}

pub(crate) fn file_not_base64(cause: impl Display) -> MessageRef {
    with_cause(FILE_NOT_BASE64, cause)
}

pub(crate) fn file_char_not_latin1(character: char) -> MessageRef {
    MessageRef::new(FILE_CHAR_NOT_LATIN1).with_arg(CHAR_ARG, verbatim(character))
}

pub(crate) fn global_missing(name: &str) -> MessageRef {
    MessageRef::new(GLOBAL_MISSING).with_arg(NAME_ARG, verbatim(name))
}

pub(crate) fn global_not_array(actual: &str) -> MessageRef {
    MessageRef::new(GLOBAL_NOT_ARRAY).with_arg(ACTUAL_ARG, verbatim(actual))
}

pub(crate) fn global_not_bool(actual: &str) -> MessageRef {
    MessageRef::new(GLOBAL_NOT_BOOL).with_arg(ACTUAL_ARG, verbatim(actual))
}

pub(crate) fn amount_not_negatable(amount: i64) -> MessageRef {
    MessageRef::new(AMOUNT_NOT_NEGATABLE).with_arg(AMOUNT_ARG, amount)
}

pub(crate) fn user_variable_not_numeric() -> MessageRef {
    MessageRef::new(USER_VARIABLE_NOT_NUMERIC)
}

pub(crate) fn user_login_unset(variable: &str) -> MessageRef {
    MessageRef::new(USER_LOGIN_UNSET).with_arg(VARIABLE_ARG, verbatim(variable))
}

pub(crate) fn user_login_empty() -> MessageRef {
    MessageRef::new(USER_LOGIN_EMPTY)
}

pub(crate) fn loop_iteration_limit(limit: i64, items: usize) -> MessageRef {
    MessageRef::new(LOOP_ITERATION_LIMIT)
        .with_arg(LIMIT_ARG, limit)
        .with_arg(ITEMS_ARG, items)
}

pub(crate) fn loop_iterations_unrecorded(from: i64, to: i64) -> MessageRef {
    MessageRef::new(LOOP_ITERATIONS_UNRECORDED)
        .with_arg(FROM_ARG, from)
        .with_arg(TO_ARG, to)
}

pub(crate) fn substring_start_out_of_range(start: usize, length: usize) -> MessageRef {
    MessageRef::new(SUBSTRING_START_OUT_OF_RANGE)
        .with_arg(START_ARG, start)
        .with_arg(LENGTH_ARG, length)
}

pub(crate) fn substring_end_out_of_range(end: usize, length: usize) -> MessageRef {
    MessageRef::new(SUBSTRING_END_OUT_OF_RANGE)
        .with_arg(END_ARG, end)
        .with_arg(LENGTH_ARG, length)
}

pub(crate) fn substring_start_after_end(start: usize, end: usize) -> MessageRef {
    MessageRef::new(SUBSTRING_START_AFTER_END)
        .with_arg(START_ARG, start)
        .with_arg(END_ARG, end)
}

pub(crate) fn time_out_of_range(amount: i64, unit: &str, base: OffsetDateTime) -> MessageRef {
    MessageRef::new(TIME_OUT_OF_RANGE)
        .with_arg(AMOUNT_ARG, amount)
        .with_arg(UNIT_ARG, verbatim(unit))
        .with_arg(BASE_ARG, base)
}

pub(crate) fn time_pattern_invalid(pattern: &str, cause: impl Display) -> MessageRef {
    with_cause(TIME_PATTERN_INVALID, cause).with_arg(PATTERN_ARG, verbatim(pattern))
}

pub(crate) fn time_text_unreadable(text: &str, pattern: &str, cause: impl Display) -> MessageRef {
    with_cause(TIME_TEXT_UNREADABLE, cause)
        .with_arg(TEXT_ARG, verbatim(text))
        .with_arg(PATTERN_ARG, verbatim(pattern))
}

pub(crate) fn search_pattern_invalid(cause: impl Display) -> MessageRef {
    with_cause(SEARCH_PATTERN_INVALID, cause)
}

pub(crate) fn expression_undefined_variables(variables: &[String]) -> MessageRef {
    MessageRef::new(EXPRESSION_UNDEFINED_VARIABLES)
        .with_arg(VARIABLES_ARG, verbatim(variables.join(", ")))
}

pub(crate) fn expression_failed(cause: impl Display) -> MessageRef {
    with_cause(EXPRESSION_FAILED, cause)
}

pub(crate) fn url_not_http() -> MessageRef {
    MessageRef::new(URL_NOT_HTTP)
}

pub(crate) fn overlay_service_not_running() -> MessageRef {
    MessageRef::new(OVERLAY_SERVICE_NOT_RUNNING)
}

pub(crate) fn overlay_show_cleared() -> MessageRef {
    MessageRef::new(OVERLAY_SHOW_CLEARED)
}

pub(crate) fn overlay_show_withdrawn() -> MessageRef {
    MessageRef::new(OVERLAY_SHOW_WITHDRAWN)
}

pub(crate) fn overlay_send_failed(cause: impl Display) -> MessageRef {
    with_cause(OVERLAY_SEND_FAILED, cause)
}

pub(crate) fn script_not_found(name: &str) -> MessageRef {
    MessageRef::new(SCRIPT_NOT_FOUND).with_arg(NAME_ARG, verbatim(name))
}

pub(crate) fn script_failed(name: &str, cause: impl Display) -> MessageRef {
    with_cause(SCRIPT_FAILED, cause).with_arg(NAME_ARG, verbatim(name))
}

pub(crate) fn inline_script_failed(cause: impl Display) -> MessageRef {
    with_cause(INLINE_SCRIPT_FAILED, cause)
}

pub(crate) fn script_panicked(cause: impl Display) -> MessageRef {
    with_cause(SCRIPT_PANICKED, cause)
}

pub(crate) fn test_donation_not_announced() -> MessageRef {
    MessageRef::new(TEST_DONATION_NOT_ANNOUNCED)
}

pub(crate) fn voice_not_engine_voice(voice: &str) -> MessageRef {
    MessageRef::new(VOICE_NOT_ENGINE_VOICE).with_arg(VOICE_ARG, verbatim(voice))
}

pub(crate) fn voice_not_installed(voice: &str) -> MessageRef {
    MessageRef::new(VOICE_NOT_INSTALLED).with_arg(VOICE_ARG, verbatim(voice))
}
