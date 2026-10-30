use std::fmt::Display;

use forge_types::{
    ActionId, MessageArg, MessageCatalog, MessageCode, MessageRef, TriggerInstanceId,
    UpstreamDetail, VariantKind, VerbatimText,
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
const KIND_ARG: &str = "kind";
const COMPONENT_ARG: &str = "component";
const POSITION_ARG: &str = "position";
const WEIGHTS_ARG: &str = "weights";

pub const UNKNOWN_ACTION: MessageCode = MessageCode::from_static(CORE_NAMESPACE, "unknown_action");
pub const UNKNOWN_TRIGGER_INSTANCE: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "unknown_trigger_instance");
pub const ACTION_NESTING_TOO_DEEP: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "action_nesting_too_deep");
pub const ACTION_RUN_CANCELLED: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "action_run_cancelled");
pub const ACTION_RUN_DISABLED: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "action_run_disabled");
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
pub const DIRECTORY_TOO_LARGE: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "directory_too_large");
pub const ACTION_DISABLED: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "action_disabled");
pub const NESTED_STEPS_UNAVAILABLE: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "nested_steps_unavailable");
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
pub const RUN_DEPTH_EXCEEDED: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "run_depth_exceeded");
pub const ACTION_STOPPED: MessageCode = MessageCode::from_static(CORE_NAMESPACE, "action_stopped");
pub const UNKNOWN_STEP_KIND: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "unknown_step_kind");
pub const STEP_DISABLED: MessageCode = MessageCode::from_static(CORE_NAMESPACE, "step_disabled");
pub const CONDITION_NOT_MET: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "condition_not_met");
pub const REGEX_INVALID: MessageCode = MessageCode::from_static(CORE_NAMESPACE, "regex_invalid");
pub const TIME_PATTERN_LEGACY: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "time_pattern_legacy");
pub const ENTRY_WITHOUT_VALUE: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "entry_without_value");
pub const RANDOM_MAX_BELOW_MIN: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "random_max_below_min");
pub const RANDOM_RANGE_TOO_WIDE: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "random_range_too_wide");
pub const WEIGHT_NOT_A_NUMBER: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "weight_not_a_number");
pub const WEIGHT_COUNT_MISMATCH: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "weight_count_mismatch");
pub const WEIGHTS_UNUSABLE: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "weights_unusable");
pub const OUTPUT_NOT_CONVERTIBLE: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "output_not_convertible");
pub const CURRENCY_INVALID: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "currency_invalid");
pub const AMOUNT_INVALID: MessageCode = MessageCode::from_static(CORE_NAMESPACE, "amount_invalid");

pub(crate) const CONDITION_ANY_DONATION_SERVICE: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "condition_any_donation_service");
pub(crate) const CONDITION_ANY_EVENT: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "condition_any_event");
pub(crate) const CONDITION_TIMER_LIVE_ONLY: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "condition_timer_live_only");
const CONDITION_DONATION_SERVICE: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "condition_donation_service");
const CONDITION_EVENT: MessageCode = MessageCode::from_static(CORE_NAMESPACE, "condition_event");
const CONDITION_TIMER_EVERY: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "condition_timer_every");
const CONDITION_TIMER_MIN_MESSAGES: MessageCode =
    MessageCode::from_static(CORE_NAMESPACE, "condition_timer_min_messages");
const SERVICE_CONDITION_ARG: &str = "service";
const EVENT_CONDITION_ARG: &str = "event";
const MINUTES_CONDITION_ARG: &str = "minutes";
const COUNT_CONDITION_ARG: &str = "count";

pub const CORE_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: CORE_NAMESPACE,
    codes: &[
        UNKNOWN_ACTION,
        UNKNOWN_TRIGGER_INSTANCE,
        ACTION_NESTING_TOO_DEEP,
        ACTION_RUN_CANCELLED,
        ACTION_RUN_DISABLED,
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
        DIRECTORY_TOO_LARGE,
        ACTION_DISABLED,
        NESTED_STEPS_UNAVAILABLE,
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
        RUN_DEPTH_EXCEEDED,
        ACTION_STOPPED,
        UNKNOWN_STEP_KIND,
        STEP_DISABLED,
        CONDITION_NOT_MET,
        REGEX_INVALID,
        TIME_PATTERN_LEGACY,
        ENTRY_WITHOUT_VALUE,
        RANDOM_MAX_BELOW_MIN,
        RANDOM_RANGE_TOO_WIDE,
        WEIGHT_NOT_A_NUMBER,
        WEIGHT_COUNT_MISMATCH,
        WEIGHTS_UNUSABLE,
        OUTPUT_NOT_CONVERTIBLE,
        CURRENCY_INVALID,
        AMOUNT_INVALID,
        CONDITION_ANY_DONATION_SERVICE,
        CONDITION_ANY_EVENT,
        CONDITION_TIMER_LIVE_ONLY,
        CONDITION_DONATION_SERVICE,
        CONDITION_EVENT,
        CONDITION_TIMER_EVERY,
        CONDITION_TIMER_MIN_MESSAGES,
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

pub(crate) fn run_depth_exceeded(limit: impl Into<MessageArg>) -> MessageRef {
    MessageRef::new(RUN_DEPTH_EXCEEDED).with_arg(LIMIT_ARG, limit)
}

pub(crate) fn action_stopped() -> MessageRef {
    MessageRef::new(ACTION_STOPPED)
}

pub(crate) fn unknown_step_kind(kind: &str) -> MessageRef {
    MessageRef::new(UNKNOWN_STEP_KIND).with_arg(KIND_ARG, verbatim(kind))
}

pub(crate) fn step_disabled() -> MessageRef {
    MessageRef::new(STEP_DISABLED)
}

pub(crate) fn condition_not_met() -> MessageRef {
    MessageRef::new(CONDITION_NOT_MET)
}

pub(crate) fn regex_invalid(cause: impl Display) -> MessageRef {
    with_cause(REGEX_INVALID, cause)
}

pub(crate) fn time_pattern_legacy(component: &str) -> MessageRef {
    MessageRef::new(TIME_PATTERN_LEGACY).with_arg(COMPONENT_ARG, verbatim(component))
}

pub(crate) fn entry_without_value(name: &str) -> MessageRef {
    MessageRef::new(ENTRY_WITHOUT_VALUE).with_arg(NAME_ARG, verbatim(name))
}

pub(crate) fn random_max_below_min() -> MessageRef {
    MessageRef::new(RANDOM_MAX_BELOW_MIN)
}

pub(crate) fn random_range_too_wide() -> MessageRef {
    MessageRef::new(RANDOM_RANGE_TOO_WIDE)
}

pub(crate) fn weight_not_a_number(position: usize) -> MessageRef {
    MessageRef::new(WEIGHT_NOT_A_NUMBER).with_arg(POSITION_ARG, position)
}

pub(crate) fn weight_count_mismatch(weights: usize, items: usize) -> MessageRef {
    MessageRef::new(WEIGHT_COUNT_MISMATCH)
        .with_arg(WEIGHTS_ARG, weights)
        .with_arg(ITEMS_ARG, items)
}

pub(crate) fn weights_unusable(cause: impl Display) -> MessageRef {
    with_cause(WEIGHTS_UNUSABLE, cause)
}

pub(crate) fn output_not_convertible(cause: impl Display) -> MessageRef {
    with_cause(OUTPUT_NOT_CONVERTIBLE, cause)
}

pub(crate) fn currency_invalid() -> MessageRef {
    MessageRef::new(CURRENCY_INVALID)
}

pub(crate) fn amount_invalid() -> MessageRef {
    MessageRef::new(AMOUNT_INVALID)
}

fn stored_kind(contract_name: &str) -> MessageArg {
    VariantKind::from_contract_name(contract_name)
        .map_or_else(|| verbatim(contract_name).into(), MessageArg::from)
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

pub(crate) fn action_run_disabled(name: &str) -> MessageRef {
    MessageRef::new(ACTION_RUN_DISABLED).with_arg(ACTION_ARG, verbatim(name))
}

pub(crate) fn action_run_failed(cause: impl Display) -> MessageRef {
    with_cause(ACTION_RUN_FAILED, cause)
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

pub(crate) fn directory_too_large(limit: usize) -> MessageRef {
    MessageRef::new(DIRECTORY_TOO_LARGE).with_arg(LIMIT_ARG, limit)
}

pub(crate) fn nested_steps_unavailable() -> MessageRef {
    MessageRef::new(NESTED_STEPS_UNAVAILABLE)
}

pub(crate) fn action_disabled() -> MessageRef {
    MessageRef::new(ACTION_DISABLED)
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
    MessageRef::new(GLOBAL_NOT_ARRAY).with_arg(ACTUAL_ARG, stored_kind(actual))
}

pub(crate) fn global_not_bool(actual: &str) -> MessageRef {
    MessageRef::new(GLOBAL_NOT_BOOL).with_arg(ACTUAL_ARG, stored_kind(actual))
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

pub(crate) fn condition_donation_service(service: &str) -> MessageRef {
    MessageRef::new(CONDITION_DONATION_SERVICE)
        .with_arg(SERVICE_CONDITION_ARG, VerbatimText::new(service))
}

pub(crate) fn condition_event(event: &str) -> MessageRef {
    MessageRef::new(CONDITION_EVENT).with_arg(EVENT_CONDITION_ARG, VerbatimText::new(event))
}

pub(crate) fn condition_timer_every(minutes: i64) -> MessageRef {
    MessageRef::new(CONDITION_TIMER_EVERY).with_arg(MINUTES_CONDITION_ARG, minutes)
}

pub(crate) fn condition_timer_min_messages(count: i64) -> MessageRef {
    MessageRef::new(CONDITION_TIMER_MIN_MESSAGES).with_arg(COUNT_CONDITION_ARG, count)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    fn ftl_variables(source: &str) -> BTreeMap<String, BTreeSet<String>> {
        let mut found: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut current = None;
        for line in source.lines() {
            if let Some((id, _)) = line.split_once(" = ").filter(|_| line.starts_with("msg-")) {
                current = Some(id.trim().to_owned());
                found.entry(id.trim().to_owned()).or_default();
            }
            let Some(id) = &current else { continue };
            let mut rest = line;
            while let Some(at) = rest.find('$') {
                rest = &rest[at + 1..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                    .collect();
                found.get_mut(id).unwrap().insert(name);
            }
        }
        found
    }

    fn every_constructor() -> Vec<MessageRef> {
        vec![
            unknown_action(ActionId::new()),
            unknown_trigger_instance(TriggerInstanceId::new()),
            action_nesting_too_deep(),
            action_run_cancelled(),
            action_run_disabled("a"),
            action_run_failed("c"),
            MessageRef::new(SCHEDULER_NOT_READY),
            MessageRef::new(SCHEDULER_STOPPED),
            scheduler_failed("c"),
            schedule_failed("c"),
            storage_failed("c"),
            system_service_failed("c"),
            task_failed("c"),
            http_failed("c"),
            condition_failed("c"),
            field_required("f"),
            file_path_empty(),
            file_path_absolute(),
            file_path_parent(),
            file_path_escapes(),
            file_not_found(),
            directory_not_found(),
            path_not_directory(),
            path_not_file(),
            file_too_large(1),
            directory_too_large(1),
            nested_steps_unavailable(),
            action_disabled(),
            file_io_failed("c"),
            file_not_json("c"),
            file_not_base64("c"),
            file_char_not_latin1('x'),
            global_missing("g"),
            global_not_array("INT"),
            global_not_bool("INT"),
            amount_not_negatable(1),
            user_variable_not_numeric(),
            user_login_unset("v"),
            user_login_empty(),
            loop_iteration_limit(1, 2),
            loop_iterations_unrecorded(1, 2),
            substring_start_out_of_range(1, 2),
            substring_end_out_of_range(1, 2),
            substring_start_after_end(1, 2),
            time_out_of_range(1, "day", OffsetDateTime::UNIX_EPOCH),
            time_pattern_invalid("p", "c"),
            time_text_unreadable("t", "p", "c"),
            search_pattern_invalid("c"),
            expression_undefined_variables(&["a".to_owned()]),
            expression_failed("c"),
            url_not_http(),
            MessageRef::new(OVERLAY_SERVICE_NOT_RUNNING),
            overlay_show_cleared(),
            overlay_show_withdrawn(),
            overlay_send_failed("c"),
            script_not_found("s"),
            script_failed("s", "c"),
            inline_script_failed("c"),
            script_panicked("c"),
            test_donation_not_announced(),
            voice_not_engine_voice("v"),
            voice_not_installed("v"),
            run_depth_exceeded(8_i64),
            action_stopped(),
            unknown_step_kind("k"),
            step_disabled(),
            condition_not_met(),
            regex_invalid("c"),
            time_pattern_legacy("[x]"),
            entry_without_value("n"),
            random_max_below_min(),
            random_range_too_wide(),
            weight_not_a_number(1),
            weight_count_mismatch(1, 2),
            weights_unusable("c"),
            output_not_convertible("c"),
            currency_invalid(),
            amount_invalid(),
            MessageRef::new(CONDITION_ANY_DONATION_SERVICE),
            MessageRef::new(CONDITION_ANY_EVENT),
            MessageRef::new(CONDITION_TIMER_LIVE_ONLY),
            condition_donation_service("s"),
            condition_event("e"),
            condition_timer_every(1),
            condition_timer_min_messages(1),
        ]
    }

    #[test]
    fn every_constructor_code_is_catalogued_and_localized_in_both_languages_with_matching_variables()
     {
        let en = ftl_variables(CORE_MESSAGES.en_ftl);
        let uk = ftl_variables(CORE_MESSAGES.uk_ftl);
        for message in every_constructor() {
            let code = message.code();
            assert!(CORE_MESSAGES.codes.contains(code), "{code} not catalogued");
            let passed: BTreeSet<String> = message.args().map(|(n, _)| n.to_owned()).collect();
            for (language, ftl) in [("en", &en), ("uk", &uk)] {
                let used = ftl
                    .get(&code.ftl_key())
                    .unwrap_or_else(|| panic!("{code} missing in {language}"));
                assert_eq!(used, &passed, "{code} variables in {language}");
            }
        }
    }

    #[test]
    fn every_catalogued_code_has_a_constructor_in_the_table() {
        let produced: BTreeSet<MessageCode> = every_constructor()
            .iter()
            .map(|m| m.code().clone())
            .collect();
        let catalogued: BTreeSet<MessageCode> = CORE_MESSAGES.codes.iter().cloned().collect();
        assert_eq!(produced, catalogued);
    }

    #[test]
    fn english_and_ukrainian_declare_the_same_messages_with_the_same_variables() {
        assert_eq!(
            ftl_variables(CORE_MESSAGES.en_ftl),
            ftl_variables(CORE_MESSAGES.uk_ftl)
        );
    }

    #[test]
    fn a_stored_contract_kind_is_a_value_kind_and_any_other_text_stays_verbatim() {
        for (actual, expected) in [
            ("int", MessageArg::ValueKind(VariantKind::Int)),
            (
                "someday",
                MessageArg::Verbatim(VerbatimText::new("someday")),
            ),
        ] {
            assert_eq!(
                global_not_array(actual).arg(ACTUAL_ARG),
                Some(&expected),
                "{actual}"
            );
            assert_eq!(
                global_not_bool(actual).arg(ACTUAL_ARG),
                Some(&expected),
                "{actual}"
            );
        }
    }

    #[test]
    fn upstream_cause_text_is_carried_as_detail_and_never_as_an_argument() {
        let failure = script_failed("greet", "runtime error: boom");

        assert_eq!(
            failure.detail().map(|d| d.as_str()),
            Some("runtime error: boom")
        );
        assert!(
            failure
                .args()
                .all(|(_, v)| !format!("{v:?}").contains("boom"))
        );
    }
}
