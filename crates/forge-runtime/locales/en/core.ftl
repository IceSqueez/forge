msg-core-unknown_action = Action { $action } does not exist
msg-core-unknown_trigger_instance = Trigger { $instance } does not exist
msg-core-action_nesting_too_deep = Actions are nested too deeply
msg-core-action_run_cancelled = The nested action was cancelled
msg-core-action_run_failed = The nested action could not be started
msg-core-scheduler_not_ready = The queue scheduler is not running yet
msg-core-scheduler_stopped = The scheduler for delayed runs is stopped
msg-core-scheduler_failed = The queue scheduler refused the request
msg-core-schedule_failed = The run could not be scheduled
msg-core-storage_failed = Data could not be read or saved
msg-core-system_service_failed = The system could not complete the request
msg-core-task_failed = The step stopped before it finished
msg-core-http_failed = The request failed
msg-core-condition_failed = The condition could not be evaluated
msg-core-field_required = The field { $field } is required
msg-core-file_path_empty = The file path is empty
msg-core-file_path_absolute = Absolute paths are not allowed. Put the file in the files folder and use a path relative to it
msg-core-file_path_parent = ".." is not allowed. The path must stay inside the files folder
msg-core-file_path_escapes = The path goes through a link that is broken or leads outside the files folder
msg-core-file_not_found = The file was not found in the files folder
msg-core-directory_not_found = The folder was not found in the files folder
msg-core-path_not_directory = The path is not a folder
msg-core-path_not_file = The path is not a file
msg-core-file_too_large = The file is larger than { $limit ->
    [one] { $limit } byte
   *[other] { $limit } bytes
}
msg-core-file_io_failed = The file could not be accessed
msg-core-file_not_json = The file is not valid JSON
msg-core-file_not_base64 = The content is not valid base64
msg-core-file_char_not_latin1 = The character { $char } cannot be written in Latin-1
msg-core-global_missing = The global variable { $name } does not exist
msg-core-global_not_array = Expected an array, but the global variable holds { $actual }
msg-core-global_not_bool = Expected a boolean, but the global variable holds { $actual }
msg-core-amount_not_negatable = The amount { $amount } cannot be negated
msg-core-user_variable_not_numeric = The stored user variable is not a number
msg-core-user_login_unset = The user login variable %{ $variable }% is not set in this run
msg-core-user_login_empty = The user login is empty
msg-core-loop_iteration_limit = The foreach loop stopped at the limit of { $limit } iterations. The array has { $items } items
msg-core-loop_iterations_unrecorded = The steps of iterations { $from } to { $to } are not recorded
msg-core-substring_start_out_of_range = The start index { $start } is beyond the end of the text (length { $length })
msg-core-substring_end_out_of_range = The end index { $end } is beyond the end of the text (length { $length })
msg-core-substring_start_after_end = The start index { $start } is greater than the end index { $end }
msg-core-time_out_of_range = Shifting { $base } by { $amount } ({ $unit }) leaves the supported date range
msg-core-time_pattern_invalid = The date pattern { $pattern } is not valid
msg-core-time_text_unreadable = The text { $text } does not match the date pattern { $pattern }
msg-core-search_pattern_invalid = The search text is not a valid regular expression
msg-core-expression_undefined_variables = The expression uses variables that are not set: { $variables }
msg-core-expression_failed = The expression could not be evaluated
msg-core-url_not_http = Only http and https addresses can be opened
msg-core-overlay_service_not_running = The overlay service is not running
msg-core-overlay_show_cleared = The overlay show queue was cleared before this show ran
msg-core-overlay_show_withdrawn = The overlay was disabled or deleted before the show ran
msg-core-overlay_send_failed = The overlay refused the message
msg-core-script_not_found = The script { $name } was not found
msg-core-script_failed = The script { $name } failed
msg-core-inline_script_failed = The inline script failed
msg-core-script_panicked = The script stopped unexpectedly
msg-core-test_donation_not_announced = The test donation could not be announced
msg-core-voice_not_engine_voice = { $voice } is not an engine voice. Pick a voice again
msg-core-voice_not_installed = The voice { $voice } is not installed. Pick a voice again
msg-core-run_depth_exceeded = Not started: the triggering event is more than { $limit } action runs deep (a loop between actions?)
msg-core-action_stopped = Stopped
msg-core-unknown_step_kind = Unknown step type: { $kind }
msg-core-step_disabled = Step is turned off
msg-core-condition_not_met = Condition not met
msg-core-regex_invalid = Not a valid regular expression
msg-core-time_pattern_legacy = The old pattern part { $component } has no date pattern equivalent. Rewrite the pattern with letters such as HH:mm dd.MM.yyyy
msg-core-entry_without_value = { $name } has no value
msg-core-random_max_below_min = Must not be below the minimum
msg-core-random_range_too_wide = The range is too wide to sample
msg-core-weight_not_a_number = Weight { $position } is not a number
msg-core-weight_count_mismatch = Lists { $weights } weights for { $items } items
msg-core-weights_unusable = These weights cannot be used
msg-core-output_not_convertible = This value cannot be stored as a variable
msg-core-currency_invalid = Must be a three-letter currency code
msg-core-amount_invalid = Must be a non-negative amount
msg-core-condition_any_donation_service = any donation service
msg-core-condition_any_event = any event
msg-core-condition_timer_live_only = only while live
msg-core-condition_donation_service = service = { $service }
msg-core-condition_event = event = { $event }
msg-core-condition_timer_every = every { $minutes } min
msg-core-condition_timer_min_messages = ≥{ $count } { $count ->
    [one] message
   *[other] messages
}
