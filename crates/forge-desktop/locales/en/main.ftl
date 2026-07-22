## Boot - splash and data-open failure screens

boot_starting = Starting…
boot_upgrade_title = Update required
boot_upgrade_body = Your forge data uses schema version { $found }, newer than this build's version { $expected }. Update forge to the latest release to open it.
boot_upgrade_reassure = Your data is safe and untouched.
boot_retry = Retry
boot_failure_title = Couldn't open your data
boot_failure_reassure = Your data is safe. If this keeps happening, please report it.

## Common actions shared across all screens

common_cancel = Cancel
common_save = Save

## Navigation - screen labels (breadcrumb + sidebar)

nav_home = Home
nav_event_feed = Event feed
nav_script_editor = Scripts

## Navigation - sidebar section headers

nav_section_audience = AUDIENCE
nav_section_automation = AUTOMATION
nav_section_builtin = Builtin

## Navigation - sidebar item labels

nav_item_home = Home
nav_item_chat = Chat
nav_item_actions = Actions
nav_item_triggers = Triggers
nav_item_queues = Queues
nav_item_event_feed = Event feed
nav_item_globals = Globals
nav_item_platforms = Platforms
nav_item_stream_apps = Stream apps
nav_item_soundboard = Soundboard
nav_item_tts = Text-to-Speech
nav_item_ws_server = WebSocket server
nav_item_hotkey = Hotkeys
nav_item_settings = Settings

## Home - hero section

home_hero_tagline = Open-source stream automation, forged for streamers
home_hero_import = Import
home_hero_new_action = New action
home_import_success = Imported action “{ $name }”
home_import_failed = Import failed: { $error }

## Home - jump cards

home_card_audience_section = AUDIENCE
home_card_audience_title = Chat
home_card_audience_stat_label = viewers now
home_card_audience_hint = Talk to your audience and see who's watching
home_card_automation_section = AUTOMATION
home_card_automation_title = Actions
home_card_automation_hint = Set up triggers, commands and timers
home_card_connections_section = CONNECTIONS
home_card_connections_title = Connections
home_card_connections_stat_label = connected
home_card_connections_hint = Manage platforms, apps and modules

## Home - stream health card

home_health_title = Stream health
home_health_live = LIVE
home_health_offline = offline
home_health_refresh_hint = last 60s · auto-refresh
home_health_throughput_label = THROUGHPUT · ev/s
home_health_bitrate_label = BITRATE · OBS
home_health_dropped_label = DROPPED · OBS
home_health_fps_label = FPS
home_health_cpu_label = CPU

## Home - connections strip

home_connections_title = Integrations

## Home - connection cell statuses

home_conn_connected = connected
home_conn_offline = offline

## Home - recent events card

home_events_title = Recent events
home_events_empty = No events yet

## Home - at-a-glance card

home_glance_title = At a glance
home_glance_actions = Actions
home_glance_commands = Commands
home_glance_fired = Fired this session
home_glance_globals = Globals

## Home - actions fired stat label (label beside the big count)

home_card_automation_stat_label = actions · { $fired } fired today

## Home - connections active/disconnected counts

home_connections_summary = { $active } active · { $disconnected } disconnected

## Settings → Appearance pane

settings_appearance_title = Appearance
settings_appearance_theme_label = Theme
settings_theme_active = ACTIVE
settings_appearance_density_label = Interface density
settings_appearance_density_subtitle = How much breathing room the interface gets - applies instantly
settings_appearance_density_compact = Compact
settings_appearance_density_compact_hint = Tighter spacing, more rows on screen
settings_appearance_density_cozy = Cozy
settings_appearance_density_cozy_hint = Balanced spacing (default)
settings_appearance_density_spacious = Spacious
settings_appearance_density_spacious_hint = Extra breathing room between elements
settings_theme_persist_failed = Failed to save theme
settings_density_persist_failed = Failed to save interface density
settings_check_updates_failed = Could not open the releases page
settings_appearance_fonts_label = Fonts
settings_appearance_theme_hint = How Forge should look
settings_appearance_font_interface = INTERFACE
settings_appearance_font_monospace = MONOSPACE
settings_appearance_font_picker_body = Interface font
settings_appearance_font_picker_mono = Monospace font
settings_appearance_font_search = Search installed fonts
settings_appearance_font_default_body = Default (Inter)
settings_appearance_font_default_mono = Default (JetBrains Mono)
settings_appearance_font_persist_failed = Failed to save font
settings_theme_desc_dark = dark
settings_theme_desc_storm = Storm
settings_theme_desc_light_mode = Light mode

## Settings → Language pane

settings_language_title = Language
settings_language_subtitle = Choose how Forge talks to you

## Settings → navigation sidebar

settings_page_title = Settings
settings_nav_group_preferences = PREFERENCES
settings_nav_group_engine = ENGINE
settings_nav_group_about = ABOUT
settings_nav_appearance = Appearance
settings_nav_language_region = Language & region
settings_nav_shortcuts = Shortcuts
settings_nav_notifications = Notifications
settings_nav_audio = Audio
settings_nav_hotkeys = Hotkeys

## Settings → Diagnostics pane

settings_about_build_label = Build
settings_about_rust_label = Rust
settings_about_os_label = OS
settings_diagnostics_log_dir_hint = Runtime logs stream to this folder.
settings_diagnostics_section_title = Logs & diagnostics
settings_diagnostics_log_dir_label = Log directory
settings_diagnostics_open_log_dir = Open log directory

## Settings → Version pane

settings_version_title = Version & updates
settings_version_license = Open-source · MIT OR Apache-2.0
settings_version_check_updates = Check for updates
settings_version_recent_releases = RECENT RELEASES

## Settings → Storage pane

settings_storage_section_title = Storage & backups
settings_storage_db_path_label = Database
settings_storage_backup_btn = Backup now
settings_storage_backup_hint = Creates a timestamped DB copy in the data directory.
settings_storage_keep_limit_label = Chat history keep limit
settings_storage_keep_limit_hint = How many chat messages to retain in the database.
settings_storage_display_limit_label = Chat history shown on open
settings_storage_display_limit_hint = How many recent messages load when the chat opens.
settings_storage_retention_label = Event log retention
settings_storage_retention_hint = How many days of event log history to keep in the database.

## Settings → Queues pane

settings_queues_section_title = Queues & threading
settings_queues_workers_label = Worker threads
settings_queues_managed_hint = Per-queue concurrency limits and blocking flags are managed on the Queues screen.

## Settings → Notifications pane

settings_notifications_section_title = Notifications
settings_notifications_hint = Per-event-type toast customisation coming later. Errors and connection changes always surface in the status bar.

## Settings → Shortcuts pane

settings_shortcuts_title = Keyboard shortcuts
settings_shortcuts_subtitle = These shortcuts work only while the forge window is focused. System-wide combinations live under Hotkeys.
settings_shortcuts_action_nav_home = Go to Hub
settings_shortcuts_action_nav_live_chat = Open Live Chat
settings_shortcuts_action_nav_actions = Open Actions
settings_shortcuts_action_nav_triggers = Open Triggers
settings_shortcuts_action_nav_twitch = Open Twitch
settings_shortcuts_action_nav_settings = Open Settings
settings_shortcuts_unbound = Not bound
settings_shortcuts_capture_prompt = Press a shortcut... Esc to cancel
settings_shortcuts_rebind = Change
settings_shortcuts_reset = Reset
settings_shortcuts_reset_all = Reset all to defaults
settings_shortcuts_fixed_section = FIXED KEYS
settings_shortcuts_fixed_enter = Confirm a form or dialog
settings_shortcuts_fixed_escape = Close a modal or cancel capture
settings_shortcuts_fixed_note = These keys are built in and cannot be rebound.
settings_shortcuts_error_needs_modifier = Combine the key with Ctrl, Alt or Meta, or pick an F-key - plain keys would interfere with typing.
settings_shortcuts_conflict_title = Shortcut already assigned
settings_shortcuts_conflict_body = { $chord } is currently assigned to “{ $owner }”. Reassign it? The previous shortcut becomes unbound.
settings_shortcuts_conflict_steal = Reassign

## Settings → WebSocket pane

settings_ws_title = WebSocket server
settings_ws_subtitle = Configure how overlays and third-party tools connect to Forge.
settings_ws_enable_label = Enable server
settings_ws_enable_description = Starts on app launch, hosts overlays, accepts WS clients
settings_ws_bind_section_title = Bind address
settings_ws_bind_section_subtitle = Which interface the server listens on
settings_ws_bind_localhost_title = Localhost only
settings_ws_bind_localhost_description = Only apps on this machine can connect. Browser sources in OBS and local Stream Deck plugins work normally. Safe default.
settings_ws_bind_lan_title = All interfaces (LAN)
settings_ws_bind_lan_description = Lets other devices on your network (phone, tablet, second PC) connect to Forge. Exposes the server to anyone on the same Wi-Fi or LAN.
settings_ws_bind_lan_restart_warning = Restart server to apply bind address change.
settings_ws_badge_recommended = Recommended
settings_ws_badge_requires_confirmation = Requires confirmation
settings_ws_port_section_title = Port
settings_ws_port_subtitle = Default 8081 · range 1024-65535
settings_ws_token_section_title = Bearer token
settings_ws_token_clients_send = Clients send this in
settings_ws_auth_section_title = Authentication
settings_ws_auth_section_subtitle = Which clients need to authenticate
settings_ws_auth_require_ws_label = Require token for WebSocket clients
settings_ws_auth_require_ws_sublabel = Reject WS handshake without valid bearer token
settings_ws_auth_require_http_label = Require token for HTTP overlay files
settings_ws_auth_require_http_sublabel = Browser sources need ?token=… in URL
settings_ws_auth_cors_label = Allow CORS from any origin
settings_ws_auth_cors_sublabel = Disable to restrict to overlay browser sources only
settings_ws_overlay_section_title = Overlay host root
settings_ws_overlay_folder_prefix = Folder served at
settings_ws_browse_btn = Browse
settings_ws_lan_modal_title = Expose Forge to your network?
settings_ws_lan_modal_explanation = You're switching from 127.0.0.1 (localhost only) to 0.0.0.0 (all network interfaces). Other devices on your LAN - and anyone on the same Wi-Fi - will be able to reach the Forge server.
settings_ws_lan_modal_confirm_label = Expose to LAN
settings_ws_lan_bullet_phone = Phone / tablet / second PC can connect to overlays and the WS API
settings_ws_lan_bullet_token_warning = Anyone on your network can read all events and send chat messages if they know your bearer token
settings_ws_lan_bullet_public_wifi = If you're on public Wi-Fi (café, conference, hotel), do not enable this
settings_ws_lan_bullet_firewall = Your firewall must also allow the configured port for this to work

## Settings → Hotkeys pane

settings_hotkeys_scope_subtitle = These combinations are registered with the operating system and fire even when forge runs in the background.
settings_hotkeys_bind_section = BIND NEW HOTKEY
settings_hotkeys_registered_section = REGISTERED
settings_hotkeys_backend_section = BACKEND
settings_hotkeys_select_action = Select action…
settings_hotkeys_bind_btn = Bind
settings_hotkeys_no_bindings = No hotkeys registered yet.
settings_hotkeys_conflict_title = Hotkey already registered
settings_hotkeys_conflict_body_suffix = is already registered. Replace or cancel?
settings_hotkeys_replace_btn = Replace
settings_hotkeys_error_no_combo = Capture a hotkey combo first.
settings_hotkeys_error_no_action = Select an action to bind.
settings_hotkeys_error_unavailable = Hotkey system is not available.
settings_hotkeys_error_load_bindings = Failed to load bindings: { $error }
settings_hotkeys_error_unbind = Unbind failed: { $error }
settings_hotkeys_error_replace = Replace failed: { $error }
settings_hotkeys_error_conflict_not_found = Conflicting hotkey not found in local cache. Refresh and try again.
settings_hotkeys_capture_prompt = Press keys... Esc to cancel

## Actions - page header / breadcrumb

actions_breadcrumb_automation = Automation
actions_breadcrumb_actions = Actions
actions_filter_all = All
actions_filter_chat = Chat
actions_filter_timers = Timers
actions_filter_points = Points
actions_search_placeholder = Search actions...
actions_stat_actions = actions
actions_stat_enabled = enabled
actions_stat_disabled = disabled
actions_empty = No actions yet

## Actions - detail panel

actions_detail_empty_title = No action selected
actions_detail_empty_hint = Select an action from the list to view its details.

## Actions - context menu

actions_menu_rename = Rename…
actions_menu_duplicate = Duplicate
actions_menu_enable = Enable
actions_menu_disable = Disable
actions_menu_delete = Delete…

## Actions - footer

## Actions - ESC hint

actions_esc_hint = ESC to cancel

## Actions - add-action modal

actions_modal_new_action_title = New action
actions_modal_edit_action_title = Edit action
actions_modal_section_name = NAME
actions_modal_section_group = GROUP
actions_modal_section_queue = QUEUE
actions_modal_section_description = DESCRIPTION
actions_modal_section_behavior = BEHAVIOR
actions_modal_enabled_label = Enabled
actions_modal_enabled_desc = Action runs when a trigger fires.
actions_modal_concurrent_label = Concurrent execution
actions_modal_concurrent_desc = Allow parallel runs in this queue.
actions_modal_bypass_label = Bypass queue pause
actions_modal_bypass_desc = Always run even if queue is paused.
actions_modal_random_pick_label = Random pick
actions_modal_random_pick_desc = Run ONE random sub-action per trigger instead of all.
actions_modal_create_btn = Create action
actions_modal_save_btn = Save changes
actions_modal_cancel_btn = Cancel

## Actions - add-sub-action modal / step chips

actions_sub_no_config = This step has no settings.
actions_sub_select_placeholder = Select...
actions_sub_select_empty = No options available
sub_cat_chat = Chat
sub_cat_moderation = Moderation
sub_cat_channel_points = Channel Points
sub_cat_polls_predictions = Polls & predictions
sub_cat_globals = Globals
sub_cat_logic = Logic
sub_cat_delay = Delay
sub_cat_scripts = Scripts
sub_cat_files = Files
sub_cat_hotkey = Hotkey
sub_cat_audio = Audio
sub_cat_tts = Text-to-speech
sub_cat_http = HTTP
sub_cat_server = Server
sub_cat_util = Utilities

## Actions - sub-action config sections

## Actions - trigger picker (side-sheet)

## Actions - trigger category display names (section headers)

## Actions - trigger kind labels

## Actions - trigger kind summaries

## Action editor - breadcrumb / tree pane / detail pane

action_editor_loading = Loading action…
action_editor_no_description = No description
action_editor_test_run = Test run
action_editor_duplicate = Duplicate
action_editor_export = Export JSON
action_editor_export_done = Action exported to { $path }
action_editor_export_failed = Export failed: { $error }
action_editor_edit = Edit
action_editor_menu_delete = Delete action
action_editor_add_trigger = Add trigger
action_editor_add_step = Add step
action_editor_health_unknown_var = Uses %{ $name }%, which no trigger provides and no earlier step produces
action_editor_health_produced_later = Uses %{ $name }%, but it is only produced by a later step
action_editor_health_isolated_sibling = Uses %{ $name }%, produced by a sibling step that runs in isolation and does not share it
action_editor_health_some_triggers = Uses %{ $name }%, which only some of this action's triggers provide
action_editor_health_last_run_failed = Last run failed: { $message }
action_editor_health_ok = Healthy: all references resolve, last run passed
action_editor_health_warn = Static warning
action_editor_health_error = Error
action_editor_branch_modal_hint = Edit this branch from the step list below
action_editor_branch_empty = No steps yet · click Add step to start
action_editor_no_steps = This action has no steps yet
action_editor_breadcrumb_steps = Steps
action_editor_branch_then = Then
action_editor_branch_else = Else
action_editor_branch_body = Body
action_editor_branch_default = Default
action_editor_branch_fallback = Branch
action_editor_branch_case = Case
action_editor_branch_chain = Chain
action_editor_add_case = Add case
action_editor_case_multi = multi-value match (read-only)
action_editor_case_match_placeholder = match value
action_editor_branch_cap = Max nesting depth reached · cannot nest deeper here
action_editor_no_triggers = No triggers · click Add trigger to start
action_editor_section_triggers_count = TRIGGERS · { $count }
action_editor_sub_actions_count = { $count } sub-actions
actions_sub_file_browse = Browse
actions_sub_datetime_pick = Pick
actions_sub_datetime_now = Now
actions_sub_datetime_set = Set
action_editor_sub_count = { $count } sub
action_editor_enabled = Enabled
action_editor_disabled = Disabled

## Action editor - execution stats
action_stat_last_fired = LAST FIRED
action_stat_runs_today = RUNS · TODAY
action_stat_avg_time = AVG TIME
action_stat_errors_7d = ERRORS · 7d
action_stat_avg_ms = { $count } ms
action_stat_avg_none = -
action_stat_execution = EXECUTION

## Action editor - run history

action_editor_run_history = Run history…
action_editor_run_history_title = Run history
action_editor_run_history_loading = Loading run history…
action_editor_run_history_empty_title = No runs yet
action_editor_run_history_empty_hint = This action has not run yet
action_editor_run_history_duration_ms = { $count } ms
action_editor_run_history_outcome_success = Success
action_editor_run_history_outcome_failed = Failed
action_editor_run_history_outcome_cancelled = Cancelled
action_editor_run_history_step_ok = ok
action_editor_run_history_step_failed = failed
action_editor_run_history_step_skipped = skipped
action_editor_run_history_step_nested = ↳
action_editor_run_history_trigger_fallback = Trigger
action_editor_run_history_step_args_in = @in
action_editor_run_history_step_produced = @out

## Action editor - step menu

action_editor_step_menu_edit = Edit step…
action_editor_step_menu_duplicate = Duplicate
action_editor_step_menu_move_top = Move to top
action_editor_step_menu_move_bottom = Move to bottom
action_editor_step_menu_delete = Delete step
actions_step_disable = Disable
actions_step_enable = Enable
actions_step_continue_on_error = Continue on error
actions_step_continue_on_error_hint = Keep running later steps if this one fails
actions_step_subtitle = Step {$index} of {$total} · edit configuration
actions_step_advanced = ADVANCED
actions_step_condition_label = RUN ONLY IF (condition)
actions_step_condition_hint = Leave empty to always run this step

## Action editor - test run

action_editor_test_failed = Test trigger failed: { $error }
action_editor_test_run_title = Test run · { $name }
action_editor_test_run_subtitle_trigger = Simulated trigger · { $name }
action_editor_test_run_subtitle_none = No trigger attached
action_editor_test_run_trigger_pick = Simulate as trigger
action_editor_test_run_note_no_schema = Trigger declares no outputs · running with empty values
action_editor_test_run_note_no_triggers = No trigger attached · running with empty values
action_editor_test_run_empty = No sub-actions to run.
action_editor_test_run_default_error = Step execution failed
action_editor_test_run_status_queued = queued
action_editor_test_run_status_running = running…
action_editor_test_run_status_failed = failed
action_editor_test_run_status_skipped = skipped
action_editor_test_run_status_ms = { $ms } ms
action_editor_test_run_failed_banner = Run failed at step { $step } · { $name }
action_editor_test_run_completed = Completed { $count } steps · { $errors } errors
action_editor_test_run_notstarted = Action did not start · the queue may be paused
action_editor_test_run_foot_simulating = Simulating…
action_editor_test_run_foot_finished = Run finished
action_editor_test_run_foot_halted = Halted on error
action_editor_test_run_foot_notstarted = Did not start
action_editor_test_run_again = Run again
action_editor_test_run_close = Close

## Action editor - sub-action / trigger pickers

action_editor_this_action = this action
action_editor_recent_triggers = Recent
picker_favorites = Favorites
picker_favorites_empty = Star items to pin them here for quick access
action_editor_picker_add_sub_title = Add sub-action
action_editor_picker_inserting_into = Inserting into
action_editor_picker_sub_count = · { $count } sub-actions
action_editor_picker_footer_hint = Added with smart defaults - edit inline after
action_editor_picker_search = Search { $count } sub-actions…
action_editor_picker_fires = Fires
action_editor_picker_available_count = · { $count } available
action_editor_trigger_picker_footer_hint = Creates a new trigger of the chosen kind and links it

## Action editor - sub-action card titles

action_editor_kind_send_chat = Send chat message
action_editor_kind_set_global = Set global
action_editor_kind_delay = Delay
action_editor_kind_log = Log
action_editor_kind_play_sound = Play sound
action_editor_kind_speak = Speak
action_editor_kind_read_file = Read file
action_editor_kind_random_int = Random int
action_editor_kind_incr_global = Increment global
action_editor_kind_run_script = Run script
action_editor_persisted_note = (persisted)
action_editor_kind_sub_action = Sub-action

## Triggers registry - page header / filters

triggers_breadcrumb_automation = Automation
triggers_breadcrumb_triggers = Triggers
triggers_search_placeholder = Search triggers…
triggers_filter_hotkey = Hotkey
triggers_usage_used = Used
triggers_usage_unused = Unused
triggers_toast_error = Triggers: { $message }
triggers_stat_instances = instances
triggers_stat_used = used
triggers_stat_disabled = disabled
triggers_platform_clear = clear
triggers_platform_timer = Timer
triggers_platform_script = Script
triggers_platform_core = Core
triggers_new_trigger = New trigger

## Triggers registry - list / empty states

triggers_empty_title = No custom trigger instances yet
triggers_empty_hint = Create a named trigger with custom settings to reuse across multiple actions.
triggers_no_results_title = No results
triggers_no_results_hint = Adjust or clear the filters to find your triggers.
triggers_clear_filters = Clear filters
triggers_col_name = NAME
triggers_col_kind = KIND
triggers_col_used = USED IN
triggers_col_on = ON
triggers_override_badge =
    { $count ->
        [one] { $count } override
       *[other] { $count } overrides
    }
triggers_used_in_prefix = used in
triggers_row_unused = unused
triggers_empty_create_first = Create your first trigger

## Triggers registry - row overflow menu

triggers_menu_rename = Rename…
triggers_menu_template = Use as template
triggers_menu_delete = Delete…
triggers_template_copy_name = { $name } copy

## Triggers registry - sheet detail

triggers_sheet_section_configuration = CONFIGURATION
triggers_sheet_config_overridden = { $count } overridden
triggers_sheet_config_all_defaults = all defaults
triggers_sheet_no_config = No configurable fields
triggers_sheet_section_cooldown = COOLDOWN
triggers_sheet_cooldown_caption = seconds · 0 = off
triggers_sheet_cooldown_value = cooldown
triggers_sheet_cooldown_scope = Global cooldown
triggers_cooldown_suffix_global = { " · cooldown=" }{ $secs }{ "s global" }
triggers_cooldown_suffix_per_user = { " · cooldown=" }{ $secs }{ "s per-user" }
triggers_sheet_section_used_in = USED IN
triggers_sheet_delete_btn = Delete
triggers_sheet_save_btn = Save
triggers_detail_loading = Loading trigger…
triggers_sheet_config_authored = Authored on the step
triggers_sheet_section_used_in_count = USED IN ({ $count })
triggers_sheet_used_in_empty_title = Not linked to any action yet.
triggers_sheet_used_in_empty_hint = Open an action and add this trigger from the picker.

## Triggers registry - confirm-disable dialog

triggers_confirm_disable_title = Disable this trigger?
triggers_confirm_disable_body = Disabling this trigger will pause it for { $count } action(s). Continue?
triggers_confirm_disable_dismiss = Cancel
triggers_confirm_disable_accept = Disable anyway

## Triggers registry - confirm-delete dialog

triggers_confirm_delete_title = Delete trigger?
triggers_confirm_delete_body = This deletes the trigger instance permanently.

## Triggers registry - rename dialog

## Triggers registry - delete undo toast

triggers_toast_deleted = Deleted '{ $name }'

## Triggers create form - kind picker

triggers_create_cancel = Cancel
triggers_create_type_count = { $count } trigger types
triggers_create_search_types = Search { $count } trigger types…
triggers_create_footer_hint = Pick an event source - configure it next
triggers_create_cat_server = Server
triggers_create_cat_timer = Timer

## Triggers create form - fill form

triggers_create_back = Back
triggers_create_new_instance = New { $kind } instance
triggers_create_section_name = NAME
triggers_create_name_placeholder = Instance name (required)
triggers_create_section_config = CONFIGURATION
triggers_create_btn = Create
triggers_create_kbd_hint = ENTER to create · ESC to cancel

## Settings → Scripting pane

settings_scripting_title = Scripting (Rhai)
settings_scripting_engine_section = Engine Limits
settings_scripting_op_limit_label = Op-count limit
settings_scripting_op_limit_hint = Range 1 000 - 10 000 000 (default 100 000)
settings_scripting_engine_timeout_label = Timeout (ms)
settings_scripting_engine_timeout_hint = Range 50 - 10 000 (default 500)
settings_scripting_http_section = HTTP Sandbox
settings_scripting_allowed_domains_label = Allowed domains
settings_scripting_allowed_domains_hint = Requests to unlisted domains are blocked. Wildcards: *.example.com
settings_scripting_domains_placeholder = e.g. api.example.com
settings_scripting_max_calls_label = Max calls per script
settings_scripting_max_calls_hint = Range 1 - 100 (default 10)
settings_scripting_http_timeout_label = Request timeout (ms)
settings_scripting_http_timeout_hint = Range 100 - 30 000 (default 5 000)
settings_scripting_max_response_label = Max response size (KiB)
settings_scripting_max_response_hint = Range 1 - 10 240 (default 1 024 KiB = 1 MiB)
settings_scripting_allow_local_label = Allow localhost / private IPs
settings_scripting_allow_local_description = Disables SSRF protections. Only enable for local development.
settings_scripting_core_allow_local_label = Allow localhost / private IPs (HTTP action)
settings_scripting_core_allow_local_description = Disables SSRF protections for the HTTP request action. Only enable for local development.
settings_scripting_ssrf_warning = WARNING - disables SSRF protections. Only enable for local development.

## Actions trigger picker - category labels

trigger_cat_chat = Chat
trigger_cat_subscriptions = Subscriptions
trigger_cat_bits = Bits
trigger_cat_raids = Raids
trigger_cat_moderation = Moderation
trigger_cat_channel_points = Channel Points
trigger_cat_polls = Polls
trigger_cat_predictions = Predictions
trigger_cat_hype = Hype Train
trigger_cat_charity = Charity
trigger_cat_goals = Goals
trigger_cat_clips = Clips
trigger_cat_streams = Streams
trigger_cat_users = Users
trigger_cat_core = Core
trigger_cat_other = Other

## Actions modals - placeholder literals

actions_name_placeholder = My automation
actions_group_placeholder = Examples
actions_description_placeholder = Plays a sound, shows overlay alert…

## Actions - list states, toasts, delete confirm

actions_tree_loading = Loading actions…
actions_loading_queues = Loading queues…
actions_no_queue = No queue available
actions_toast_error = Actions: { $message }
actions_rename_taken = Name '{ $name }' is already taken
actions_deleted_toast = Deleted '{ $name }'
actions_delete_title = Delete action?
actions_delete_body = This will remove the action and all of its sub-actions and triggers.

## Triggers registry - error messages

triggers_delete_reference_block = Remove this trigger from all actions before deleting.

## TTS - tab bar section labels

tts_tab_dashboard = Dashboard
tts_tab_engines = Engines
tts_tab_aliases = Voice aliases
tts_tab_filters = Filters

## TTS - breadcrumb

tts_breadcrumb_builtin = Builtin
tts_breadcrumb_tts = Text-to-Speech

## TTS Dashboard - control strip

tts_dash_pause_btn = Pause queue
tts_dash_resume_btn = Resume
tts_dash_skip_btn = Skip
tts_dash_stop_all_btn = Stop all
tts_dash_stop_all_confirm_name = Stop all TTS
tts_dash_stop_all_confirm_hint = Currently speaking message will be cut off and all queued messages dropped. Engines remain ready to handle new messages.
tts_dash_test_placeholder = Type to test a voice…
tts_dash_speak_btn = Speak
tts_dash_test_speaker_name = Test

## TTS Dashboard - now speaking

tts_dash_now_speaking_header = NOW SPEAKING
tts_dash_no_speaking = -
tts_dash_last_drop = Last request dropped: { $reason }

## TTS Dashboard - queue

tts_dash_queue_header = Up next
tts_dash_queue_total = ~{ $secs }s total
tts_dash_queue_empty = Queue is empty
tts_dash_play_now = Play now
tts_dash_remove_queued = Remove from queue

## TTS Dashboard - session stats

tts_dash_session_header = SESSION
tts_dash_stat_spoken = Spoken
tts_dash_stat_skipped = Skipped
tts_dash_stat_filtered = Filtered
tts_dash_stat_avg_latency = Avg latency
tts_dash_engines_header = ENGINES
tts_dash_engines_none = No engines available
tts_dash_engine_no_voices = no voices installed

## TTS Engines - list

tts_engines_header_prefix = CONFIGURED
tts_engines_add_engine = Add engine
tts_engines_add_none_left = All cloud engines configured
tts_engines_select_hint = Select an engine to configure
tts_header_engines_ready = { $count ->
    [one] { $count } engine ready
   *[other] { $count } engines ready
}
tts_engines_rail_sub = { $kind } · { $count ->
    [one] { $count } voice
   *[other] { $count } voices
}

## TTS Engines - detail header

tts_engines_detail_sub = { $kind } engine · { $count ->
    [one] { $count } voice
   *[other] { $count } voices
}
tts_engines_detail_sub_region = { $kind } engine · { $region } · { $count ->
    [one] { $count } voice
   *[other] { $count } voices
}

## TTS Engines - sections

tts_engines_section_credentials = CREDENTIALS
tts_engines_creds_encrypted_note = Stored encrypted in the local database
tts_engines_section_params = DEFAULT VOICE PARAMETERS
tts_engines_param_pitch = Pitch
tts_engines_param_speed = Speed
tts_engines_param_volume = Volume

## TTS Engines - voices section

tts_engines_voices_header_prefix = VOICES
tts_engines_voices_available = { $count } available
tts_engines_voices_empty = No voices found
tts_engines_voice_preview_sample = This is my voice.
tts_engines_toggle_failed = Failed to change engine state
tts_engines_persist_disabled_failed = Failed to save engine state

## TTS Filters - pipeline column

tts_filters_pipeline_intro = Pipeline that text passes through before being spoken.

## TTS Filters - numbered stage cards

tts_filters_stage_skip_title = Skip rules
tts_filters_stage_replacements_title = Text replacements
tts_filters_stage_blocklist_title = Word blocklist
tts_filters_stage_output_title = Output

## TTS Filters - skip rules

tts_filters_skip_contains_url = Contains URL
tts_filters_skip_prefix = Starts with { $prefix }
tts_filters_skip_bot_accounts = From bot accounts
tts_filters_skip_longer_than = Message longer than { $chars } chars
tts_filters_skip_repeat = Identical to last { $window } messages
tts_filters_skip_emote_only = Emote-only messages
tts_filters_skip_mostly_non_latin = Mostly non-Latin script
tts_filters_skip_regex_row = Regex: { $pattern }

## TTS Filters - word blocklist

tts_filters_blocklist_censor = Censor matched words
tts_filters_blocklist_censor_meta = replace with ***
tts_filters_blocklist_skip = Skip entire message if matched
tts_filters_blocklist_more = +{ $count } more
tts_filters_blocklist_empty = No blocked words yet

## TTS Filters - text replacements

tts_filters_replacements_empty = No replacements yet

## TTS Filters - output

tts_filters_output_read_name = Read display name first
tts_filters_output_read_name_meta = e.g. "koval_dev says: ..."
tts_filters_output_emote = Emote → word
tts_filters_output_emote_meta = convert :pog: → "pog"
tts_filters_output_sanitize = Strip repeated punctuation
tts_filters_output_sanitize_meta = "!!!" → "!"

## TTS Filters - rule list

tts_filters_badge_text = TEXT
tts_filters_badge_regex = REGEX
tts_filters_stage_add = Add

## TTS Filters - add filter modal

tts_filters_modal_skip_title = Add skip rule
tts_filters_modal_skip_subtitle = Messages matching this are never spoken
tts_filters_modal_blocklist_title = Add blocked words
tts_filters_modal_blocklist_subtitle = Matched words are censored or drop the message
tts_filters_modal_replace_title = Add text replacement
tts_filters_modal_replace_subtitle = Rewrite text before it is spoken
tts_filters_modal_output_title = Add output option
tts_filters_modal_output_subtitle = Shape the final spoken output

tts_filters_modal_condition_label = CONDITION
tts_filters_modal_cancel = Cancel
tts_filters_modal_add_rule = Add rule
tts_filters_modal_add_words = Add words
tts_filters_modal_footer_valid = Runs top-to-bottom within this stage
tts_filters_modal_footer_invalid = Fill required fields

tts_filters_preset_skip_url = Contains a URL
tts_filters_preset_skip_prefix = Starts with a prefix
tts_filters_preset_skip_prefix_label = PREFIX
tts_filters_preset_skip_prefix_placeholder = !
tts_filters_preset_skip_bots = From bot accounts
tts_filters_preset_skip_length = Longer than N characters
tts_filters_preset_skip_length_label = MAX CHARACTERS
tts_filters_preset_skip_length_placeholder = 200
tts_filters_preset_skip_repeat = Identical to recent messages
tts_filters_preset_skip_emote_only = Emote-only messages
tts_filters_preset_skip_non_latin = Mostly non-Latin script
tts_filters_preset_skip_regex = Custom regex match
tts_filters_preset_skip_regex_label = REGEX PATTERN
tts_filters_preset_skip_regex_placeholder = (buy|cheap) followers

tts_filters_preset_output_name_hint = e.g. "koval_dev says: ..."
tts_filters_preset_output_emote_hint = convert :pog: → "pog"
tts_filters_preset_output_lang = Auto-detect language
tts_filters_preset_output_lang_hint = pick voice per message language
tts_filters_preset_output_maxdur = Cut off after N seconds
tts_filters_preset_output_maxdur_hint = stop long messages early
tts_filters_preset_output_sanitize_hint = "!!!" → "!"

tts_filters_modal_blocklist_words_label = WORDS OR PHRASES
tts_filters_modal_blocklist_words_placeholder = one per line, or comma-separated...
tts_filters_modal_blocklist_note = Matching is case-insensitive and matches whole words only; multi-word phrases will not match as a unit.
tts_filters_modal_blocklist_when_matched_label = WHEN MATCHED
tts_filters_modal_blocklist_censor_row = Censor the word
tts_filters_modal_blocklist_censor_row_hint = replace with ***
tts_filters_modal_blocklist_skip_row = Skip the whole message
tts_filters_modal_blocklist_skip_row_hint = nothing is spoken

tts_filters_modal_replace_text_tab = TEXT
tts_filters_modal_replace_regex_tab = REGEX
tts_filters_modal_replace_find_label = FIND
tts_filters_modal_replace_match_label = MATCH PATTERN
tts_filters_modal_replace_find_placeholder = POG
tts_filters_modal_replace_replace_label = REPLACE WITH
tts_filters_modal_replace_replace_text_placeholder = respect
tts_filters_modal_replace_note = Leave replacement empty to strip matched text.

## TTS Filters - pipeline settings

## TTS Filters - preview column

tts_filters_preview_header = Live preview
tts_filters_preview_input_label = INPUT MESSAGE
tts_filters_preview_input_placeholder = Type a message to preview…
tts_filters_preview_empty = Enter a message above to preview
tts_filters_preview_output_label = STAGE OUTPUTS
tts_filters_preview_final_label = OUTPUT
tts_filters_speak_preview_btn = Speak preview
tts_filters_preview_speaker_name = Preview
tts_filters_stage_pass = pass
tts_filters_stage_skipped = skipped
tts_filters_stage_name_skip_rules = SKIP RULES
tts_filters_stage_name_replacements = REPLACEMENTS
tts_filters_stage_name_blocklist = BLOCKLIST
tts_filters_stage_name_output = OUTPUT
tts_filters_skip_reason_rule = matched a skip rule
tts_filters_skip_reason_blocked = blocked word
tts_filters_skip_reason_empty = empty after filters

## Cloud TTS Engines - card buttons

tts_cloud_test_connection_btn = Test connection
tts_cloud_testing_btn = Testing…
tts_cloud_save_credentials_btn = Save credentials

## Cloud TTS Engines - status badges

tts_cloud_not_configured = NOT CONFIGURED
tts_cloud_configured = CONFIGURED
tts_cloud_connection_failed = CONNECTION FAILED

## Cloud TTS Engines - test result

tts_cloud_connection_verified = Connection verified

## Cloud TTS Engines - toast messages

tts_cloud_saved_toast = { $name } engine is ready - no restart needed.
tts_cloud_save_failed_toast = Failed to save { $name } credentials: { $error }

## Voice Aliases - strategy banner

tts_aliases_strategy_label = Default assignment strategy
tts_aliases_strategy_sublabel = How a voice is chosen for viewers without a manual alias
tts_aliases_strategy_deterministic = Deterministic by name
tts_aliases_strategy_random = Random
tts_aliases_strategy_single = Single voice

## Voice Aliases - toolbar

tts_aliases_search_placeholder = Search viewers…
tts_aliases_count = { $count ->
    [one] { $count } manual alias
   *[other] { $count } manual aliases
}
tts_aliases_assign_btn = Assign voice

## Voice Aliases - table headers

tts_aliases_col_viewer = VIEWER
tts_aliases_col_voice = VOICE
tts_aliases_col_pitch = PITCH
tts_aliases_col_speed = SPEED
tts_aliases_col_actions = ACTIONS

## Voice Aliases - empty state

tts_aliases_empty = No voice aliases configured
tts_aliases_loading = Loading voice aliases…

## Voice Aliases - blocked row

tts_aliases_never_speak = Never speak

## TTS Voice aliases - role badges

tts_aliases_role_blocked = BLOCKED

## Voice Aliases - assign/edit modal

tts_aliases_form_title_assign = Assign a voice
tts_aliases_form_title_edit = Edit voice alias
tts_aliases_form_viewer_label = VIEWER
tts_aliases_form_viewer_placeholder = Viewer name
tts_aliases_form_engine_label = ENGINE
tts_aliases_form_voice_label = VOICE
tts_aliases_form_voice_placeholder = Voice id
tts_aliases_form_pitch_label = PITCH (st)
tts_aliases_form_pitch_placeholder = 0
tts_aliases_form_rate_label = RATE (x)
tts_aliases_form_rate_placeholder = 1.0
tts_aliases_form_create = Create
tts_aliases_form_block_label = Never speak
tts_aliases_form_block_desc = This viewer's messages are never spoken.
tts_aliases_form_blocked_note = Never speak - voice settings do not apply.

## Voice Aliases - delete confirm

tts_aliases_delete_title = Delete voice alias?
tts_aliases_delete_body = { $viewer } will fall back to the default voice assignment strategy.
common_delete = Delete
common_undo = Undo

## Voice Aliases - preview

tts_aliases_preview_text = This is a voice preview.

## Voice Aliases - footer caption

tts_aliases_footer_caption = Showing { $shown } of { $total } manual aliases · { $auto } viewers use auto-assignment

## Soundboard - breadcrumb

soundboard_breadcrumb_builtin = Builtin
soundboard_breadcrumb_soundboard = Soundboard

## Soundboard - header / modal

soundboard_loading = Loading clips…
soundboard_empty_title = No clips yet
soundboard_playback_error_prefix = Playback error: { $error }

## Soundboard - modal

soundboard_modal_title_add = Add clip
soundboard_modal_title_edit = Edit clip
soundboard_modal_no_file = No file selected
soundboard_modal_browse_btn = Browse
soundboard_modal_name_placeholder = Clip name
soundboard_modal_save_btn = Save
soundboard_modal_cancel_btn = Cancel
soundboard_modal_validation_error = Name and audio file are required.
soundboard_delete_title = Delete clip?
soundboard_delete_body = This removes the clip from your soundboard permanently.

## Soundboard - modal section labels

soundboard_modal_section_file = FILE
soundboard_modal_section_name = NAME
soundboard_device_system_default = System default

## Soundboard - device load error

## Soundboard - audio player error

## Soundboard - feedback

## Soundboard - redesigned screen

soundboard_search_placeholder = Search sounds…
soundboard_header_summary = { $device } Output · { $count } sounds
soundboard_hero_title = Soundboard
soundboard_hero_blurb = Trigger sound clips from pads, hotkeys, or actions. Routed to a virtual output OBS can capture.
soundboard_hero_enabled = Enabled
soundboard_hero_disabled = Disabled
soundboard_category_all = All { $count }
soundboard_category_memes = Memes
soundboard_category_alerts = Alerts
soundboard_category_music = Stingers
soundboard_category_voice = Voice
soundboard_stop_all = Stop all
soundboard_pad_playing = playing…
soundboard_no_matches = No sounds match your filter
soundboard_library_section = Library
soundboard_library_import = Import
soundboard_add_sound = Add sound
soundboard_modal_section_category = CATEGORY
soundboard_modal_ready = Ready to add
soundboard_modal_fill_required = Fill required fields
soundboard_routing_section = Output routing
soundboard_routing_device = DEVICE
soundboard_routing_hint = Add this device as an Audio Input Capture in OBS.
soundboard_routing_volume = MASTER VOLUME · { $pct }%
soundboard_routing_headphones = Also play in headphones
soundboard_footer_left = { $sounds } sounds · { $categories } categories · { $size }
soundboard_output_ready = Output device ready
soundboard_output_missing = Output device missing

## Queues - page header

queues_breadcrumb_automation = Automation
queues_breadcrumb_queues = Queues
queues_pause_all_btn = Pause all
queues_new_queue_btn = New queue
queues_subtitle = Manage action queues, their concurrency, and pause state
queues_stat_queues = queues
queues_stat_running = running
queues_stat_paused = paused
queues_empty = No queues configured.
queues_loading = Loading queues…
queues_search_placeholder = Search queues…
queues_no_filter_match = No queues match the active filter.
queues_filter_all = All
queues_filter_running = Running
queues_filter_paused = Paused
queues_filter_parallel = Parallel
queues_filter_sequential = Sequential
queues_drain_feedback = Draining “{ $name }”.
queues_configure_btn = Configure
queues_drain_btn = Drain
queues_pause_btn = Pause
queues_resume_btn = Resume

## Queues - card menu

queues_menu_configure = Configure…
queues_menu_pause = Pause
queues_menu_resume = Resume
queues_menu_drain = Drain queue
queues_menu_delete = Delete…
queues_delete_confirm_title = Delete queue
queues_delete_confirm_body = Actions in this queue move to Default. This cannot be undone.

## Queues - new queue modal

queues_create_title = New queue
queues_create_name_label = Name
queues_create_name_placeholder = Queue name (required)
queues_create_desc_label = Description
queues_create_desc_placeholder = What this queue is for
queues_create_desc_optional = (optional)
queues_concurrency_label = Concurrency
queues_concurrency_serial = Sequential - only one action at a time
queues_concurrency_parallel = Parallel - up to { $count } actions concurrently
queues_create_btn = Create queue
queues_create_cancel = Cancel
queues_edit_btn = Save changes
queues_edit_title = Configure { $name }
queues_create_subtitle = How actions run in this queue
queues_create_kbd_hint = Esc to cancel

## Queues - card metrics

queues_metric_concurrency = CONCURRENCY
queues_metric_pending = PENDING
queues_metric_actions = ACTIONS
queues_metric_assigned = assigned
queues_metric_serial = sequential
queues_metric_parallel = parallel
queues_metric_in_flight = in flight
queues_metric_idle = idle
queues_metric_held = held

## Queues - paused panel

queues_paused_with_time = { $pending } actions waiting - paused { $mins } min ago
queues_paused_simple = Queue is paused

## Queues - running panel

queues_running_now_header = RUNNING NOW
queues_no_actions_running = No actions running
queues_running_label = running

## Queues - status badge

queues_status_paused = PAUSED
queues_status_running = RUNNING

## Queues - live-membership divergence

queues_not_live_badge = NOT LIVE · RESTART

## Queues - overflow pill

queues_overflow_more = +{ $count } more

## Queues - built-in queue descriptions

## TTS dashboard - engine card sublabels / priority badge

tts_dash_priority_high = HIGH
tts_dash_priority_bits = BITS { $amount }

## TTS engines - unknown engine fallback

## Cloud TTS - form field labels

tts_cloud_field_api_key = API Key
tts_cloud_field_region = Region
tts_cloud_field_access_key_id = Access key ID
tts_cloud_field_secret_key = Secret key
tts_cloud_field_placeholder_subscription_key = Subscription key

## Soundboard - file-dialog filter

soundboard_file_filter_audio = Audio

## Platforms overview

platforms_title = Streaming platforms
platforms_subtitle = Connect once, Forge listens to all chats and events in one place.
platforms_breadcrumb = Platforms

platforms_status_connected = Connected
platforms_status_not_connected = Not connected

platforms_twitch_desc = Chat, EventSub subscriptions, channel points, bits, raids
platforms_youtube_desc = Live chat, super chats, channel memberships, subscribers
platforms_kick_desc = Chat, channel events, subscribers - newer streaming platform

## Platforms - feature chips

platforms_feature_irc_chat = IRC chat
platforms_feature_channel_points = Channel points
platforms_feature_bits_subs = Bits & subs
platforms_feature_live_chat = Live chat
platforms_feature_super_chat = Super chat
platforms_feature_memberships = Memberships
platforms_feature_chat = Chat
platforms_feature_subs = Subs
platforms_feature_channel_events = Channel events

## Platform generic detail

platform_generic_features_available = WHAT YOU CAN DO ONCE CONNECTED
platform_generic_kind_platform = Streaming platform
platform_generic_status_available = available - click Connect to authorize
platform_generic_connect_btn = Connect

## Twitch panel

twitch_header_subtitle = Connect to enable chat, subs, bits, raids, channel points, and EventSub
twitch_auth_title = Authorize Forge on Twitch
twitch_auth_subtitle = Twitch uses device code authorization. You'll see a code here, enter it on Twitch's site, and we'll auto-detect when you're done. We never see your password.
twitch_btn_start = Start authorization
twitch_btn_try_again = Try again
twitch_btn_cancel = Cancel
twitch_btn_restart = Restart
twitch_btn_open = Open
twitch_requesting = Requesting authorization code from Twitch…
twitch_authorizing = Code accepted. Finalising authorization…
twitch_polling_primary = Waiting for you to authorize on Twitch…
twitch_polling_secondary = polling every 5s
twitch_step1_title = Open this URL in any browser
twitch_step2_title = Approve in your browser
twitch_step2_detail = forge is listening on a local port for the OAuth callback. The window will refresh once you approve.
twitch_timer_prefix = Times out in
twitch_scopes_header = Permissions Forge will request
twitch_scopes_count = { $count } scopes
twitch_missing_client_id = Twitch integration is not configured. Set FORGE_TWITCH_CLIENT_ID with your own registered application's client_id and restart the app.
twitch_reauth_title = Twitch token is missing required scopes
twitch_reauth_detail = EventSub rejected the chat subscription. Re-authorize to refresh the token with all current scopes.
twitch_reauth_btn = Re-authorize

## OBS panel

## Builtin detail

builtin_picker_scene = Choose a Scene
builtin_picker_source = Choose a Source
builtin_picker_audio_input = Choose an Audio Input
builtin_picker_hotkey = Choose a Hotkey
builtin_picker_expression = Choose an Expression
builtin_picker_midi_port = Choose a MIDI Port

## OAuth / local callback flow

oauth_header_subtitle = Connect to enable live chat and events
oauth_auth_title = Authorize Forge on { $name }
oauth_auth_subtitle = This platform uses device code authorization. You will see a code below - enter it on the platform's site and we will detect when you are done. We never see your password.
oauth_btn_retry = Retry
oauth_btn_cancel = Cancel
oauth_btn_return = Return to Platforms
oauth_step1_title = Open this URL in any browser
oauth_step1_open = Open
oauth_step2_title = Approve in your browser
oauth_step2_detail = forge is listening on a local port for the OAuth callback. The window will refresh once you approve.
oauth_polling_primary = Waiting for you to authorize on the platform…
oauth_polling_secondary = polling every 5s
oauth_requesting = Requesting authorization code…
oauth_authorized_title = Connected to { $name }!
oauth_authorized_subtitle = Authorization complete.
oauth_failed_title = Authorization failed

## Server screen

server_breadcrumb_builtin = Builtin
server_breadcrumb_server = Server
server_header_title = Built-in Server
server_header_desc = Internal HTTP + WebSocket server for overlays and remote control
server_status_running = Running
server_status_stopped = Stopped
server_status_error = Error
server_not_running = Not running
server_up_prefix = Up { $uptime }
server_bind_address = BIND ADDRESS
server_bearer_token = BEARER TOKEN
server_btn_restart = Restart
server_btn_restarting = Restarting…
server_btn_stop = Stop
server_btn_stopping = Stopping…
server_btn_copy = COPY
server_stat_clients = CLIENTS
server_stat_clients_sub = connected
server_stat_events_out = EVENTS OUT
server_stat_events_sub = avg { $avg } ev/s
server_stat_http = HTTP REQUESTS
server_stat_http_sub = since restart
server_stat_bandwidth = BANDWIDTH
server_stat_bandwidth_sub = peak { $peak } KB/s
server_stat_dropped = DROPPED EVENTS
server_stat_dropped_sub = since restart
server_clients_header = Connected Clients
server_clients_empty = No clients connected
server_col_client = CLIENT
server_col_subscriptions = SUBSCRIPTIONS
server_col_evs = EV/S
server_col_uptime = UPTIME
server_overlay_files_empty = No overlay files found
server_overlay_dir_items = { $count } items
server_disconnect_confirm_hint = Client at { $info } will be disconnected from the WebSocket server. Other clients are not affected.
server_btn_regenerate = Regenerate
server_regen_warning_title = Regenerating disconnects all clients
server_regen_warning_body = Connected WebSocket clients must reconnect with the new token.
server_throughput_title = Throughput
server_throughput_meta = last { $seconds }s · peak { $peak } KB/s
server_overlay_files_title = Overlay Files
server_btn_open = OPEN
server_open_overlay_folder_failed = Could not open the overlay folder
server_clients_live = live
server_footer_totals = Total sent: { $sent } · Total events: { $events }
server_disconnect_confirm_title = Disconnect client?
server_disconnect_esc_hint = to cancel
server_btn_disconnect = Disconnect

## Common status badges (shared across platform detail pages)

common_status_not_connected = Not connected

## YouTube platform detail

youtube_description = Live chat, super chats, channel memberships, subscribers.
youtube_feature_live_chat = Live chat with sentiment markers
youtube_feature_super_chat = Super Chat alerts with bits-equivalent tiers
youtube_feature_memberships = Channel memberships join/upgrade/cancel events
youtube_feature_subscribers = Subscriber milestone triggers

## Kick platform detail

kick_description = Chat, subs, hosts - hybrid: official OAuth API for send, community Pusher WS for receive. Not affiliated with Kick.com.
kick_feature_live_chat = Live chat (receive + send via OAuth)
kick_feature_subs = Subscription and gifted-sub events
kick_feature_hosts_bans = Host and ban events
kick_feature_deleted_replies = Message-deleted and reply events

## VTube Studio platform detail

## Stream apps overview

stream_apps_title = Stream apps
stream_apps_subtitle = Local apps Forge talks to over WebSocket. Connect to control them from actions.
stream_apps_breadcrumb = Stream Apps
stream_apps_obs_desc = Scenes, sources, recording control, replay buffers - full obs-websocket API
stream_apps_vtube_desc = Vtuber avatar control: hotkeys, expressions, item triggers

## Live Chat - page header / filters

chat_breadcrumb_audience = Audience
chat_breadcrumb_chat = Chat
chat_filter_all = All
chat_filter_events = Events only
chat_filter_hide_bots = Hide bots
chat_viewers_unit = viewers
chat_no_filter_matches = No messages match these filters.
chat_send_placeholder_connected = Send to chat...
chat_send_placeholder_to = Send to {$platform} chat...
chat_search_placeholder = Search messages...
chat_new_message = 1 new message
chat_new_messages = { $count } new messages
chat_viewers_title = Viewers

## Live Chat - viewer drawer

chat_drawer_search_placeholder = Search viewers...
chat_drawer_active_count = { $total } active · { $shown } shown
chat_drawer_section_active = ACTIVE NOW · { $count }
chat_drawer_no_matches = No chat participants match the search
chat_drawer_click_hint = Click a username in chat to see details
chat_drawer_last_seen = Last seen { $when }
chat_drawer_shoutout = Shoutout
chat_drawer_whisper = Whisper
chat_drawer_whisper_title = Whisper to { $recipient }
chat_drawer_whisper_placeholder = Type a message…
chat_drawer_whisper_send = Send
chat_drawer_whisper_cancel = Cancel
chat_drawer_set_tts_voice = Set TTS voice…
chat_drawer_block_tts = Block from TTS
chat_drawer_timeout = Timeout 10 min
chat_drawer_ban = Ban from channel
chat_stat_watch_time = WATCH TIME
chat_stat_messages = MESSAGES
chat_stat_sub = SUB
chat_stat_sub_yes = Yes
chat_stat_follow = FOLLOW
chat_drawer_shoutout_sent = Shoutout sent
chat_drawer_shoutout_failed = Shoutout failed: { $error }
chat_drawer_whisper_sent = Whisper sent
chat_drawer_whisper_failed = Whisper failed: { $error }
chat_drawer_timeout_sent = Viewer timed out for 10 min
chat_drawer_timeout_failed = Timeout failed: { $error }
chat_drawer_ban_sent = Viewer banned
chat_drawer_ban_failed = Ban failed: { $error }
chat_drawer_block_tts_sent = Viewer blocked from TTS
chat_drawer_block_tts_failed = Block from TTS failed: { $error }
chat_ctx_timeout_10m = Timeout 10 min
chat_ctx_timeout_1h = Timeout 1 hour
chat_ctx_timeout_2w = Timeout 2 weeks
chat_ctx_ban = Ban
chat_ctx_timeout_sent = Timeout applied
chat_reply = Reply
chat_reply_title = Replying to @{ $recipient }
chat_reply_placeholder = Type a reply…
chat_reply_sent = Reply sent
chat_reply_failed = Reply failed: { $error }

## Event Feed - page header / filters

event_feed_filter_all = All { $n }
event_feed_filter_chat = Chat { $n }
event_feed_filter_subs = Subs { $n }
event_feed_filter_bits = Bits { $n }
event_feed_filter_timers = Timers { $n }
event_feed_filter_obs = OBS { $n }
event_feed_filter_errors = Errors { $n }
event_feed_export_success = Exported event feed to { $path }
event_feed_export_failed = Event feed export failed: { $error }
event_feed_no_events = No events yet - system activity appears here in real time.
event_feed_no_filter_match = No events match the active filter.
event_feed_inspector_title = Event inspector
event_feed_auto_scroll_on = Auto-scroll on
event_feed_auto_scroll_off = Auto-scroll off
event_feed_breadcrumb_automation = Automation
event_feed_events_live_stream = events · live stream
event_feed_status_live = Live
event_feed_status_paused = Paused
event_feed_search_placeholder = Search events…

## Globals - page header / filters

globals_breadcrumb_automation = Automation
globals_breadcrumb_globals = Globals
globals_filter_all = All
globals_filter_persisted = Persisted
globals_filter_session = Session
globals_search_placeholder = Search variables...
globals_loading = Loading...
globals_deleted_toast = Deleted '{ $name }'
globals_stat_total = total
globals_stat_persisted = persisted
globals_stat_in_memory = in-memory
globals_empty_caption = No variables match this filter.
globals_col_modified = LAST MODIFIED
globals_col_reads_writes = READS · WRITES
globals_col_persist = PERSIST
globals_col_actions = ACTIONS
globals_rename_taken = Name '{ $name }' is already taken
globals_menu_rename = Rename
globals_menu_persist = Persist
globals_menu_session_only = Session only
globals_toast_error = Globals: { $message }
globals_export_done = Globals exported to { $path }
globals_export_failed = Globals export failed: { $error }

## Globals - variant editor modal

globals_editor_title_create = New variable
globals_editor_title_edit = Edit variable
globals_editor_section_name = NAME
globals_editor_section_type = TYPE
globals_editor_type_locked_hint = Type is fixed after creation and can't be changed here
globals_editor_section_persistence = PERSISTENCE
globals_editor_section_value = VALUE
globals_editor_persist_label = Save across restarts
globals_editor_persist_desc = Persisted globals survive app close; session-only reset on launch
globals_editor_cancel = Cancel
globals_editor_save = Save
globals_editor_saving = Saving...
globals_editor_kbd_hint = ⌘ Enter to save
globals_editor_name_placeholder = my_variable
globals_error_invalid_int = Invalid integer
globals_error_invalid_float = Invalid float
globals_error_invalid_datetime = Invalid ISO 8601 datetime (e.g. 2026-05-18T14:23:00Z)
globals_error_invalid_json_array = Invalid JSON array
globals_error_invalid_json_object = Invalid JSON object
globals_error_name_required = Name is required
globals_error_name_taken = A global with this name already exists
globals_delete_confirm_title = Delete global variable
globals_delete_confirm_body = This permanently removes the variable and its value.

## Globals - value inspector modal

globals_inspect_subtitle_items = { $kind } · { $count } items · read-only
globals_inspect_subtitle_keys = { $kind } · { $count } keys · read-only
globals_inspect_snapshot = Live value snapshot · updates on next read
globals_inspect_close = Close
globals_inspect_edit = Edit value

## Script Editor - page / toolbar

script_editor_breadcrumb_automation = Automation
script_editor_edited_prefix = edited
script_editor_run = Test run
script_editor_save = Save
script_editor_format = Format
script_editor_api_docs = API docs
script_editor_debug = Debug
script_editor_output_header = Output
script_editor_api_reference = API reference
script_editor_search_placeholder = Search scripts…
script_editor_new_script = New script
script_editor_no_scripts = No scripts yet
script_editor_group_action = Action scripts
script_editor_group_standalone = Standalone
script_editor_manual_run = manual run
script_editor_rename_action = Rename
script_editor_enable_action = Enable
script_editor_disable_action = Disable
script_editor_delete_action = Delete
script_editor_running = Running…
script_editor_run_modal_cancel = Cancel
script_editor_save_blocked = Save blocked - fix syntax errors first
script_editor_discard_title = Discard unsaved changes?
script_editor_discard_body = This script has unsaved edits. Continue and lose them, or stay to keep editing.
script_editor_discard_confirm = Discard
script_editor_discard_cancel = Keep editing
script_editor_discard_esc_hint = to keep editing
script_editor_sandbox_label = Sandbox:
script_editor_sandbox_enabled = enabled
script_editor_problems_tab = Problems
script_editor_console_cleared = Console cleared.
script_editor_no_problems = No problems.

## Script Editor - run modal

script_editor_health = { $ok }/{ $total } healthy
script_editor_type_check_passed = Type-check passed
script_editor_type_check_errors = { $count ->
    [one] { $count } error
   *[other] { $count } errors
}
script_editor_run_modal_title = Run { $name }
script_editor_run_input_placeholder = Enter { $label } value…

## Action telemetry - stat column headers

## Action editor - validation errors

## Integration detail - OBS / quick-action

builtin_disconnect_confirm_hint = You will be disconnected and will need to reconnect manually. Live events from this integration stop arriving until then.
integration_disconnect_title = Disconnect integration
integration_settings_coming_soon = Settings coming soon
integration_control_failed = Control command failed
integration_quick_action_failed = Quick action failed
integration_open_url_failed = Could not open the link in your browser
integration_quick_action_na = N/A
integration_state_connecting_title = Connecting…
integration_state_connecting_detail = Establishing a session with this integration.
integration_state_reconnecting_title = Reconnecting…
integration_state_reconnecting_detail = The session dropped; forge is re-establishing it.
integration_state_disconnected_detail = Use Reconnect above to link this integration.

## OAuth / authentication errors

auth_error_credentials_missing_youtube = YouTube OAuth client credentials are not configured
auth_error_credentials_missing_kick = Kick OAuth client credentials are not configured

## Widget - key capture

widget_key_capture_placeholder = Press a combo…

## Widget - event inspector

widget_event_replay = Replay this event
widget_event_payload_header = PAYLOAD
widget_event_caused_header = CAUSED

## Widget - chat row

## Live chat - event descriptors

chat_event_subscribed = subscribed (Tier { $tier })
chat_event_raided = raided with
chat_event_cheered = cheered
chat_event_viewers = { $viewers } viewers
chat_event_super_chat = sent a Super Chat ({ $amount } { $currency })
chat_event_new_member = became a member
chat_event_member_milestone = member milestone

## Widget - builtin header actions

widget_header_action_reconnect = Reconnect
widget_header_action_refresh_token = Refresh Token
widget_header_action_disconnect = Disconnect
widget_header_action_settings = Settings
widget_header_capability_limited = Limited

## Widget - builtin content

widget_builtin_stream_health = STREAM HEALTH
widget_builtin_active_badge = ACTIVE
widget_builtin_live_badge = LIVE
widget_builtin_active_count = { $count } active
widget_builtin_event_count =
    { $count ->
        [one] { $count } event
       *[other] { $count } events
    }

## Widget - server file list

## Widget - server confirm modal

widget_confirm_type_prefix = Type
widget_confirm_type_suffix = to confirm:
widget_confirm_esc_to_cancel = to cancel
widget_confirm_cancel = Cancel

## Widget - destructive confirm modal

widget_confirm_delete_title = Delete { $kind }?
widget_confirm_delete_hint = This item will be permanently removed. This action cannot be undone.
widget_confirm_delete_kind_script = script

## Widget - save indicator

widget_copied_toast = Copied to clipboard
widget_save_all_saved = All changes saved
widget_save_saving = Saving…
widget_save_unsaved = Unsaved changes
widget_save_failed = Save failed: { $error }

## Widget - server bearer token

## Widget - server bind card

## Widget - picker modal

widget_picker_search_placeholder = Search…
widget_picker_loading = Loading…
widget_picker_no_results = No results.

## Widget - output device picker

widget_device_default_suffix = (default)
widget_device_test = Test

## Widget - quick actions panel

widget_quick_actions_title = Quick actions

## Widget - console (script output)

## Settings - audio output

settings_audio_scanning = Scanning devices…
settings_audio_title = Audio
settings_audio_output_devices = OUTPUT DEVICES
settings_audio_test_section = TEST
settings_audio_test_tone = Play 440 Hz test tone
settings_audio_test_playing = Playing…
settings_audio_test_error = Test tone error: { $error }
settings_audio_persist_error = Failed to save device selection: { $error }

## Script editor - API docs panel

script_editor_api_no_matches = No matches
script_editor_api_search_placeholder = Search modules…

## Script editor - details panel

script_editor_details_heading = DETAILS
script_editor_signature_heading = SIGNATURE
script_editor_details_type = Type
script_editor_details_linked = Linked to
script_editor_type_action = Action scripts
script_editor_type_standalone = Standalone
script_editor_open_action = Open action
script_editor_details_lines = Lines
script_editor_details_edited = Edited
script_editor_details_returns = returns
script_editor_run_stats_heading = RUN STATS
script_editor_stat_runs = RUNS
script_editor_stat_avg = AVG
script_editor_stat_runs_value = { $n } today
script_editor_stat_avg_value = { $n } ms

## Widget - layout chrome

## Widget - volume slider

## Locale-aware formatting - month abbreviations (en)

fmt_month_abbr_01 = Jan
fmt_month_abbr_02 = Feb
fmt_month_abbr_03 = Mar
fmt_month_abbr_04 = Apr
fmt_month_abbr_05 = May
fmt_month_abbr_06 = Jun
fmt_month_abbr_07 = Jul
fmt_month_abbr_08 = Aug
fmt_month_abbr_09 = Sep
fmt_month_abbr_10 = Oct
fmt_month_abbr_11 = Nov
fmt_month_abbr_12 = Dec

## Locale-aware formatting - relative time

fmt_relative_never = never
fmt_relative_seconds = { $count ->
    [one] { $count }s ago
   *[other] { $count }s ago
}
fmt_relative_minutes = { $count ->
    [one] { $count } min ago
   *[other] { $count } min ago
}
fmt_relative_hours = { $count ->
    [one] { $count }h ago
   *[other] { $count }h ago
}
fmt_relative_days = { $count ->
    [one] { $count }d ago
   *[other] { $count }d ago
}

## Storage error screen

## Integration seed
iseed_metric_chat = Chat
iseed_metric_messages = Messages
iseed_metric_eventsub = EventSub
iseed_metric_api_budget = API budget
iseed_metric_websocket = WebSocket
iseed_metric_streaming = Streaming
iseed_metric_mode = Mode
iseed_metric_activity = Activity
iseed_metric_session = Session
iseed_metric_detail = Detail
iseed_scenes = Scenes
iseed_sources = Sources
iseed_dropped = Dropped
iseed_channel = Channel
iseed_status = Status
iseed_stat_bitrate = Bitrate
iseed_stat_fps = FPS
iseed_field_viewers = Viewers
iseed_field_category = Category
iseed_field_uptime = Uptime
iseed_field_latency = Latency
iseed_field_since = Since
iseed_section_eventsub_subs = EventSub subscriptions
iseed_section_oauth_scopes = OAuth scopes
iseed_section_live_broadcast = Live broadcast
iseed_section_stream_stats = Stream stats
iseed_section_overview = Overview
iseed_section_details = Details
iseed_cta_manage_subscriptions = Manage subscriptions
iseed_action_run_ad = Run ad
iseed_action_create_clip = Create clip
iseed_action_commercial = Commercial
iseed_action_shoutout = Shoutout
iseed_action_switch_scene = Switch scene
iseed_action_toggle_source = Toggle source
iseed_action_record = Record
iseed_action_toggle_mute = Toggle mute
iseed_action_send_message = Send message
iseed_action_clear_chat = Clear chat
iseed_action_slow_mode = Slow mode
iseed_action_ban_user = Ban user
iseed_kick_capability = Hybrid transport
iseed_kick_banner_title = Hybrid chat transport
iseed_kick_banner_body = Chat receive rides the community Pusher WebSocket; writes use the official API.
iseed_generic_connect_hint = Connect to see live status
