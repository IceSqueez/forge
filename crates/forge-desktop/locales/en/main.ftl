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
common_language = Language

## Navigation - screen labels (breadcrumb + sidebar)

nav_home = Home
nav_actions = Actions
nav_queues = Queues
nav_triggers = Triggers
nav_integration = Integration
nav_live_chat = Live chat
nav_event_feed = Event feed
nav_globals = Globals
nav_settings = Settings
nav_tts = TTS
nav_soundboard = Soundboard
nav_script_editor = Scripts
nav_api_reference = API reference
nav_server = Server

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
nav_group_modules = Builtin
nav_item_soundboard = Soundboard
nav_item_tts = Text-to-Speech
nav_item_ws_server = WebSocket server
nav_item_discord = Discord
nav_item_midi = MIDI
nav_item_hotkey = Hotkeys
nav_item_settings = Settings

## Home - hero section

home_hero_tagline = Open-source stream automation, forged for streamers
home_hero_import = Import
home_hero_new_action = New action
home_import_success = Imported action “{ $name }”
home_import_failed = Import failed: { $error }
home_stats_error = Couldn’t load dashboard stats: { $error }
home_stats_retry = Retry

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
settings_theme_mocha_desc = Dark, warm
settings_theme_tokyo_desc = Dark, cool
settings_theme_latte_desc = Light
settings_appearance_density_label = Interface density
settings_appearance_density_subtitle = How much breathing room the interface gets - applies instantly
settings_appearance_density_compact = Compact
settings_appearance_density_compact_hint = Tighter spacing, more rows on screen
settings_appearance_density_cozy = Cozy
settings_appearance_density_cozy_hint = Balanced spacing (default)
settings_appearance_density_spacious = Spacious
settings_appearance_density_spacious_hint = Extra breathing room between elements
settings_appearance_fonts_label = Fonts
settings_appearance_fonts_subtitle = Interface and code typefaces - applies instantly
settings_appearance_fonts_scanning = Scanning installed fonts…
settings_appearance_font_body_label = Interface font
settings_appearance_font_mono_label = Monospace font
settings_appearance_font_default_placeholder = { $family } (default)
settings_appearance_font_reset = Reset to default
settings_appearance_font_missing = "{ $family }" is not installed - the default is used until it returns
settings_appearance_font_show_all = Show all fonts
settings_appearance_font_preview = The quick brown fox jumps over the lazy dog · 0123456789
settings_appearance_theme_hint = How Forge should look
settings_appearance_font_interface = INTERFACE
settings_appearance_font_monospace = MONOSPACE
settings_theme_default = Default
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
settings_nav_language = Language
settings_nav_language_region = Language & region
settings_nav_shortcuts = Shortcuts
settings_nav_notifications = Notifications
settings_nav_audio = Audio
settings_nav_scripting = Scripting
settings_nav_queues = Queues
settings_nav_storage = Storage
settings_nav_websocket = WebSocket
settings_nav_hotkeys = Hotkeys
settings_nav_version = Version
settings_nav_diagnostics = Diagnostics

## Settings → Diagnostics pane

settings_about_build_label = Build
settings_about_rust_label = Rust
settings_about_os_label = OS
settings_diagnostics_log_dir_hint = Runtime logs stream to this folder.
settings_diagnostics_section_title = Logs & diagnostics
settings_diagnostics_log_dir = Log directory: { $path }
settings_diagnostics_log_dir_label = Log directory
settings_diagnostics_open_log_dir = Open log directory
settings_diagnostics_log_level_hint = Log level: controlled via RUST_LOG env var (e.g. info, debug, trace).

## Settings → Version pane

settings_version_title = Version & updates
settings_version_license = Open-source · MIT OR Apache-2.0
settings_version_check_updates = Check for updates
settings_version_recent_releases = RECENT RELEASES
settings_version_changelog_empty = No release history yet.

## Settings → Storage pane

settings_storage_section_title = Storage & backups
settings_storage_db_path = Database: { $path }
settings_storage_db_path_label = Database
settings_storage_backup_btn = Backup now
settings_storage_backup_hint = Creates a timestamped DB copy in the data directory.
settings_storage_keep_limit_label = Chat history keep limit
settings_storage_keep_limit_hint = How many chat messages to retain in the database.
settings_storage_display_limit_label = Chat history shown on open
settings_storage_display_limit_hint = How many recent messages load when the chat opens.

## Settings → Queues pane

settings_queues_section_title = Queues & threading
settings_queues_thread_hint = Tokio threadpool: { $workers } worker(s) (auto-sized to system).
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
settings_shortcuts_action_nav_event_feed = Open Event Feed
settings_shortcuts_action_nav_actions = Open Actions
settings_shortcuts_action_nav_triggers = Open Triggers
settings_shortcuts_action_nav_twitch = Open Twitch
settings_shortcuts_action_nav_globals = Open Globals
settings_shortcuts_action_nav_script_editor = Open Scripts
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
settings_shortcuts_error_global_hotkey = { $chord } is already claimed by a global hotkey. Unbind it under Settings → Hotkeys first.
settings_shortcuts_conflict_body = { $chord } is currently assigned to “{ $owner }”. Reassign it? The previous shortcut becomes unbound.
settings_shortcuts_conflict_steal = Reassign

## Settings → WebSocket pane

settings_ws_title = WebSocket server
settings_ws_subtitle = Configure how overlays and third-party tools connect to Forge.
settings_ws_all_saved = All changes saved
settings_ws_saving = Saving…
settings_ws_save_failed = Save failed: { $error }
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
settings_hotkeys_conflict_body_prefix = Combo
settings_hotkeys_conflict_body_suffix = is already registered. Replace or cancel?
settings_hotkeys_replace_btn = Replace
settings_hotkeys_error_no_combo = Capture a hotkey combo first.
settings_hotkeys_error_no_action = Select an action to bind.
settings_hotkeys_error_unavailable = Hotkey system is not available.
settings_hotkeys_error_load_actions = Failed to load actions: { $error }
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
actions_new_btn = + New action
actions_loading = Loading...
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

actions_sub_select_kind = Choose a step type
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
actions_sub_chip_send_chat = Send chat
actions_sub_chip_set_global = Set global
actions_sub_chip_delay = Delay
actions_sub_chip_log = Log
actions_sub_chip_play_sound = Play sound
actions_sub_chip_speak = Speak
actions_sub_chip_read_file = Read file
actions_sub_chip_random_int = Random int
actions_sub_modal_add_btn = Add step
actions_sub_modal_save_btn = Save changes
actions_sub_modal_cancel_btn = Cancel

## Actions - sub-action config sections

actions_sub_section_message = MESSAGE
actions_sub_section_target_platform = TARGET PLATFORM
actions_sub_section_variable_name = VARIABLE NAME
actions_sub_section_value = VALUE
actions_sub_section_milliseconds = MILLISECONDS
actions_sub_section_level = LEVEL
actions_sub_section_clip = CLIP
actions_sub_section_text = TEXT
actions_sub_section_voice_override = VOICE OVERRIDE (optional)
actions_sub_section_path = PATH (relative to assets sandbox)
actions_sub_section_target_var = TARGET VARIABLE
actions_sub_section_min = MIN
actions_sub_section_max = MAX
actions_sub_helper_variables = Variables: %user%, %message%, %args%
actions_sub_helper_interpolation = Supports variable interpolation
actions_sub_voice_hint = Leave blank to use alias resolver
actions_sub_path_hint = Sandboxed under data_dir/assets/ · no ../ traversal · max 1 MiB
actions_sub_no_clips = No clips yet - add one in the Soundboard screen first.

## Actions - trigger picker (side-sheet)

actions_picker_title = Add trigger
actions_picker_loading = Loading triggers…
actions_picker_cancel = Cancel
actions_picker_select_platform = Select a platform
actions_picker_no_triggers = No triggers available
actions_picker_select_hint = Select a platform to browse triggers.
actions_picker_no_triggers_selection = No triggers available for this selection.
actions_picker_default_label = (default)

## Actions - trigger category display names (section headers)

actions_cat_chat_commands = CHAT COMMANDS
actions_cat_subs_bits = SUBS & BITS
actions_cat_bits = BITS
actions_cat_raids = RAIDS
actions_cat_obs_events = OBS EVENTS
actions_cat_server_events = SERVER EVENTS
actions_cat_timers = TIMERS
actions_cat_ungrouped = UNGROUPED
actions_cat_all = ALL

## Actions - trigger kind labels

actions_kind_twitch_chat_command = Twitch · chat command
actions_kind_twitch_chat_message = Twitch · any chat message
actions_kind_twitch_subscriber = Twitch · new subscriber
actions_kind_twitch_resubscriber = Twitch · re-subscribe
actions_kind_twitch_gift_sub = Twitch · gift subs
actions_kind_twitch_cheer = Twitch · bits cheered
actions_kind_twitch_raid = Twitch · raid received
actions_kind_obs_scene_changed = OBS · scene changed
actions_kind_server_custom_event = Server · custom event
actions_kind_unknown = Unknown trigger

## Actions - trigger kind summaries

actions_summary_twitch_chat_command = User types !command in chat
actions_summary_twitch_chat_message = Every chat message fires this
actions_summary_twitch_subscriber = Fires when someone subscribes
actions_summary_twitch_resubscriber = Existing sub renews
actions_summary_twitch_gift_sub = Someone gifts subs to channel
actions_summary_twitch_cheer = Viewer sends bits
actions_summary_twitch_raid = Another stream raids you
actions_summary_obs_scene_changed = Fires when OBS switches the active scene
actions_summary_server_custom_event = Fires when triggerCodeEvent is called via the WebSocket API

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
action_editor_edit_modal_title = Edit action
action_editor_edit_save_btn = Save
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
action_editor_delete = Delete
action_editor_delete_cascade_hint = { $sub_actions } sub-actions and { $trigger_links } trigger links will also be removed.
action_editor_section_triggers = TRIGGERS
action_editor_section_triggers_count = TRIGGERS · { $count }
action_editor_triggers_hint = Click a trigger to edit it in the registry
action_editor_section_sub_actions = SUB-ACTIONS · { $count }
action_editor_section_sub_actions_label = SUB-ACTIONS
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
action_editor_saved_triggers = Your saved triggers
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
action_editor_no_unlinked_triggers = No unlinked triggers available - create one on the Triggers screen

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
triggers_open_create_btn = + Create
triggers_search_placeholder = Search triggers…
triggers_filter_twitch = Twitch
triggers_filter_youtube = YouTube
triggers_filter_kick = Kick
triggers_filter_obs = OBS
triggers_filter_vtube = VTube Studio
triggers_filter_midi = MIDI
triggers_filter_hotkey = Hotkey
triggers_filter_discord = Discord
triggers_filter_script = Script
triggers_filter_all = All
triggers_usage_all = All
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
triggers_empty_create = + Create trigger instance
triggers_no_results_title = No results
triggers_no_results_hint = Adjust or clear the filters to find your triggers.
triggers_clear_filters = Clear filters
triggers_usage_badge = used in { $count }
triggers_toggle_on = ON
triggers_toggle_off = OFF
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
triggers_sheet_config_save = Save
triggers_sheet_config_cancel = Cancel
triggers_sheet_no_config = No configurable fields
triggers_sheet_section_cooldown = COOLDOWN
triggers_sheet_cooldown_caption = seconds · 0 = off
triggers_sheet_cooldown_value = cooldown
triggers_sheet_cooldown_scope = Global cooldown
triggers_cooldown_suffix_global = { " · cooldown=" }{ $secs }{ "s global" }
triggers_cooldown_suffix_per_user = { " · cooldown=" }{ $secs }{ "s per-user" }
triggers_sheet_not_registered = Trigger kind not registered
triggers_sheet_section_used_in = USED IN
triggers_sheet_section_platform = PLATFORM
triggers_sheet_delete_btn = Delete
triggers_sheet_save_btn = Save
triggers_sheet_any_platform = Any platform
triggers_sheet_will_fire_on = Will fire on: { $platform }
triggers_sheet_will_fire_on_scope = Will fire on: { $scope }
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

triggers_rename_title = Rename trigger
triggers_rename_kbd_hint = ENTER to save · ESC to cancel

## Triggers registry - delete undo toast

triggers_toast_deleted = Deleted '{ $name }'

## Triggers create form - kind picker

triggers_create_select_kind = Select trigger kind
triggers_create_search_placeholder = Search kinds…
triggers_create_no_results = No matching trigger kinds
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
triggers_create_section_platform = PLATFORM
triggers_create_scope_any = Any
triggers_create_scope_custom = Custom…
triggers_create_will_fire = Will fire on: { $scope }
triggers_create_btn = Create
triggers_create_kbd_hint = ENTER to create · ESC to cancel

## Settings → Scripting pane

settings_scripting_title = Scripting (Rhai)
settings_scripting_all_saved = All changes saved
settings_scripting_saving = Saving…
settings_scripting_unsaved = Unsaved changes
settings_scripting_save_failed = Save failed: { $error }
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
trigger_cat_obs = Scenes
trigger_cat_hotkey = Hotkeys
trigger_cat_core = Core
trigger_cat_server = Server Events
trigger_cat_timer = Timers
trigger_cat_other = Other
trigger_subgroup_scenes = Scenes
trigger_subgroup_sources = Sources
trigger_subgroup_audio = Audio
trigger_subgroup_filters = Filters
trigger_subgroup_streaming = Streaming
trigger_subgroup_recording = Recording
trigger_subgroup_studio_mode = Studio Mode
trigger_subgroup_transitions = Transitions
trigger_subgroup_virtual_camera = Virtual Camera
trigger_subgroup_connection = Connection
trigger_subgroup_scene_collections = Scene Collections
trigger_subgroup_profiles = Profiles

## Actions modals - placeholder literals

actions_name_placeholder = My automation
actions_group_placeholder = Examples
actions_description_placeholder = Plays a sound, shows overlay alert…
actions_log_message_placeholder = Action started
actions_speak_text_placeholder = Text to speak…
actions_rename_placeholder = Name

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
tts_tab_triggers = Triggers
tts_tab_cloud_engines = Cloud engines

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
tts_engines_more_placeholder = + More engines in future releases
tts_engines_select_hint = Select an engine to configure
tts_engines_status_ready = Ready
tts_header_engines_ready = { $count ->
    [one] { $count } engine ready
   *[other] { $count } engines ready
}

## TTS Engines - detail header

tts_engines_local_meta = local TTS engine
tts_engines_default_badge = DEFAULT
tts_engines_detail_voice_count = { $count ->
    [one] { $count } voice
   *[other] { $count } voices
}

## TTS Engines - sections

tts_engines_section_engine = ENGINE
tts_engines_credentials_notice = Credentials stored encrypted in the local database, never in config files
tts_engines_no_credentials = LOCAL - no credentials
tts_engines_section_params = DEFAULT VOICE PARAMETERS
tts_engines_param_pitch = Pitch
tts_engines_param_speed = Speed
tts_engines_param_volume = Volume

## TTS Engines - voices section

tts_engines_voices_header_prefix = AVAILABLE VOICES
tts_engines_voices_filter_placeholder = Filter voices…
tts_engines_voices_loading = Loading voices…
tts_engines_voices_empty = No voices found

## TTS Filters - pipeline column

tts_filters_pipeline_header = PROCESSING PIPELINE
tts_filters_pipeline_hint = Each message passes through these stages in order before being spoken

## TTS Filters - numbered stage cards

tts_filters_stage_emote_url_title = Emote & URL handling
tts_filters_stage_replacements_title = Text replacements
tts_filters_stage_blocklist_title = Word blocklist
tts_filters_stage_output_title = Output length

## TTS Filters - rule list

tts_filters_no_rules = No filter rules yet
tts_filters_add_rule_btn = Add rule
tts_filters_rule_on = ON
tts_filters_rule_off = OFF
tts_filters_kind_literal = Text replace
tts_filters_kind_regex = Regex replace
tts_filters_kind_blocklist = Blocklist
tts_filters_badge_text = TEXT
tts_filters_badge_regex = REGEX
tts_filters_badge_block = BLOCK
tts_filters_stage_add = Add

## TTS Filters - rule draft editor

tts_filters_draft_header = RULE
tts_filters_draft_name_placeholder = Rule name (optional)
tts_filters_draft_pattern_placeholder = Match pattern
tts_filters_draft_replacement_placeholder = Replacement
tts_filters_draft_words_placeholder = Blocked words (comma-separated)
tts_filters_draft_add = Add rule
tts_filters_mode_censor = Censor
tts_filters_mode_skip = Skip msg

## TTS Filters - pipeline settings

tts_filters_url_label = URL HANDLING
tts_filters_url_speak = Read URL aloud
tts_filters_url_replace = Replace with "link"
tts_filters_url_suppress = Skip message
tts_filters_length_label = MAX LENGTH
tts_filters_length_placeholder = No limit
tts_filters_blocklist_default_label = DEFAULT BLOCKLIST MODE
tts_filters_strip_twitch = Strip Twitch emotes
tts_filters_strip_reward = Strip channel-point emotes
tts_filters_unsaved = Unsaved changes
tts_filters_saved = All changes saved

## TTS Filters - preview column

tts_filters_preview_header = PIPELINE PREVIEW
tts_filters_preview_input_label = INPUT MESSAGE
tts_filters_preview_input_placeholder = Type a message to preview…
tts_filters_preview_empty = Enter a message above to preview
tts_filters_preview_output_label = FINAL OUTPUT
tts_filters_speak_preview_btn = Speak preview
tts_filters_preview_speaker_name = Preview
tts_filters_preview_tip = Type any message above to see how filters transform it in real time
tts_filters_stage_n = STAGE { $n }
tts_filters_stage_pass = pass
tts_filters_stage_skipped = skipped
tts_filters_preview_skipped = [message would be skipped]
tts_filters_delete_title = Delete rule?
tts_filters_delete_body = This rule will be removed from the preprocessing pipeline.

## TTS Triggers - header

tts_triggers_header = WHAT GETS SPOKEN
tts_triggers_hint = Enable sources and set who can trigger them

## TTS Triggers - command card

tts_triggers_cmd_title = Chat command
tts_triggers_cmd_subtitle = !tts <message>
tts_triggers_cmd_meta = cooldown 8s · max 250 chars

## TTS Triggers - channel points card

tts_triggers_points_title = Channel point reward
tts_triggers_points_subtitle = "Speak my message" · 500 pts
tts_triggers_points_meta = no cooldown · priority queue

## TTS Triggers - bits card

tts_triggers_bits_title = Bits / cheers
tts_triggers_bits_subtitle = Speak cheer message
tts_triggers_bits_min_label = Minimum
tts_triggers_bits_min_value = 100 bits
tts_triggers_bits_meta = louder = longer message

## TTS Triggers - sub messages card

tts_triggers_subs_title = Sub messages
tts_triggers_subs_subtitle = Speak resub / gift messages
tts_triggers_subs_disabled = Disabled - toggle to enable

## TTS Triggers - format card

tts_triggers_format_header = MESSAGE FORMAT
tts_triggers_format_read_username = Read username before message
tts_triggers_format_template_header = TEMPLATE
tts_triggers_format_speak_emotes = Speak emotes as words

## TTS Triggers - queue behavior card

tts_triggers_queue_header = QUEUE BEHAVIOR
tts_triggers_queue_max_length = Max queue length
tts_triggers_queue_per_user_limit = Per-user limit in queue
tts_triggers_queue_bits_skip = Bits & points skip the line

## TTS Triggers - role chips

tts_triggers_role_subscribers = Subscribers
tts_triggers_role_vips = VIPs
tts_triggers_role_mods = Mods
tts_triggers_role_everyone = Everyone

## Cloud TTS Engines - header

tts_cloud_header = CLOUD ENGINES · 4

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

tts_aliases_role_mod = MOD
tts_aliases_role_vip = VIP
tts_aliases_role_sub = SUB
tts_aliases_role_blocked = BLOCKED

## Voice Aliases - assign/edit modal

tts_aliases_form_title_assign = Assign a voice
tts_aliases_form_title_edit = Edit voice alias
tts_aliases_form_viewer_label = VIEWER
tts_aliases_form_viewer_placeholder = Viewer name
tts_aliases_form_engine_label = ENGINE
tts_aliases_form_engine_placeholder = Select engine
tts_aliases_form_voice_label = VOICE
tts_aliases_form_voice_placeholder = Voice id
tts_aliases_form_pitch_label = PITCH (st)
tts_aliases_form_pitch_placeholder = 0
tts_aliases_form_rate_label = RATE (x)
tts_aliases_form_rate_placeholder = 1.0
tts_aliases_form_create = Create
tts_aliases_form_block_label = Block from TTS
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

tts_aliases_footer_caption = Showing { $shown } of { $total } manual aliases

## Soundboard - breadcrumb

soundboard_breadcrumb_builtin = Builtin
soundboard_breadcrumb_soundboard = Soundboard

## Soundboard - header / modal

soundboard_add_clip_btn = Add clip
soundboard_loading = Loading clips…
soundboard_empty_title = No clips yet
soundboard_empty_hint = Click "Add clip" to add your first sound.
soundboard_playback_error_prefix = Playback error: { $error }

## Soundboard - modal

soundboard_modal_title_add = Add clip
soundboard_modal_title_edit = Edit clip
soundboard_modal_no_file = No file selected
soundboard_modal_browse_btn = Browse
soundboard_modal_name_placeholder = Clip name
soundboard_modal_hotkey_placeholder = e.g. Ctrl+1
soundboard_modal_devices_loading = Loading devices…
soundboard_modal_save_btn = Save
soundboard_modal_saving_btn = Saving…
soundboard_modal_cancel_btn = Cancel
soundboard_modal_validation_error = Name and audio file are required.

## Soundboard - modal section labels

soundboard_modal_section_file = FILE
soundboard_modal_section_name = NAME
soundboard_modal_section_hotkey = HOTKEY
soundboard_modal_section_device = OUTPUT DEVICE
soundboard_device_system_default = System default
soundboard_modal_section_volume = VOLUME

## Soundboard - device load error

soundboard_modal_device_load_error = Device load failed: { $error }

## Soundboard - audio player error

soundboard_player_not_init = Audio player not initialised - check Settings → Audio.

## Soundboard - feedback

soundboard_playing_feedback = Playing "{ $name }" → { $device }. Live audio is wired via the runtime soon.
soundboard_removed_feedback = Removed "{ $name }".
soundboard_saved_feedback = Saved "{ $name }". Playback routing is wired via the runtime soon.
soundboard_modal_kbd_hint = Enter to save · Esc to cancel

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
queues_drain_feedback = Draining “{ $name }”.
queues_configure_btn = Configure
queues_drain_btn = Drain
queues_pause_btn = Pause
queues_resume_btn = Resume

## Queues - card menu

queues_menu_configure = Configure…
queues_menu_rename = Rename…
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
queues_concurrency_serial = Serial - only one action at a time
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
queues_metric_serial = serial
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

tts_dash_engine_local_ready = local · ready
tts_dash_priority_high = HIGH
tts_dash_priority_bits = BITS { $amount }

## TTS engines - unknown engine fallback

tts_engines_unknown = Unknown engine


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
platform_generic_features_coming = WHAT YOU'LL BE ABLE TO DO
platform_generic_kind_platform = Streaming platform
platform_generic_kind_stream_app = Stream app
platform_generic_status_available = available - click Connect to authorize
platform_generic_status_coming = not yet implemented
platform_generic_parent_platforms = Platforms
platform_generic_parent_stream_apps = Stream apps
platform_generic_connect_btn = Connect

## Twitch panel

twitch_breadcrumb_platforms = Platforms
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

obs_breadcrumb_stream_apps = Stream apps
obs_header_subtitle = Connect to control scenes, sources, audio, filters, and recording
obs_instructions_title = Before you start
obs_instructions_lead = In OBS Studio, enable the built-in WebSocket server, then copy the settings here.
obs_step1 = In OBS: Tools → WebSocket Server Settings
obs_step2 = Check 'Enable WebSocket server'
obs_step3 = Note the port (default 4455)
obs_step4 = Click 'Show Connect Info' to reveal password
obs_requirements_header = REQUIREMENTS
obs_req_version = OBS Studio 28+ (WebSocket v5 built-in)
obs_req_network = Running on the same machine or LAN-reachable
obs_form_title = Connection settings
obs_field_host = HOST
obs_field_port = PORT
obs_field_password = PASSWORD
obs_field_keychain = stored encrypted in the local database
obs_toggle_reconnect_title = Auto-reconnect on disconnect
obs_toggle_reconnect_subtitle = Retry with exponential backoff
obs_toggle_launch_title = Connect on app launch
obs_toggle_launch_subtitle = Start connecting when Forge opens
obs_btn_test = Test connection
obs_btn_connect = Connect
obs_test_running = Testing connection…
obs_test_success = Test successful
obs_test_failed = Test failed
obs_tip = Running OBS on a different PC? Set host to that machine's IP. Make sure OBS WebSocket is configured to bind to 0.0.0.0 instead of localhost, and the port is open in firewall.
obs_port_invalid = port must be a number 1-65535

## Builtin detail

builtin_breadcrumb = Builtin
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
oauth_btn_connect = Connect
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
server_stat_http_sub = overlays served
server_stat_bandwidth = BANDWIDTH
server_stat_bandwidth_sub = peak { $peak } KB/s
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
server_clients_live = live
server_footer_totals = Total sent: { $sent } · Total events: { $events }
server_disconnect_confirm_title = Disconnect client?
server_disconnect_esc_hint = to cancel
server_btn_disconnect = Disconnect

## Common status badges (shared across platform detail pages)

common_status_not_connected = Not connected
common_status_coming_soon = Coming soon

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

vtube_description = Vtuber avatar control: hotkeys, expressions, item triggers.
vtube_feature_hotkeys = Trigger hotkeys from chat events
vtube_feature_expressions = Switch expressions and outfits
vtube_feature_item_drops = Spawn item drops on bits/subs

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
chat_send_placeholder_disconnected = Connect a platform to send...
chat_send_placeholder_connected = Send to chat...
chat_send_placeholder_to = Send to {$platform} chat...
chat_no_messages_title = No messages
chat_no_messages_empty = Not connected - go to Settings → Platforms to connect.
chat_no_events_yet = No events yet.
chat_no_search_matches = No messages match your search.
chat_messages_count = { $count ->
    [one] { $count } message
   *[other] { $count } messages
}
chat_matches_count = { $count ->
    [one] { $count } match
   *[other] { $count } matches
}
chat_header_viewers = { $count ->
    [one] { $formatted } viewer
   *[other] { $formatted } viewers
}
chat_show_viewers = Show viewers
chat_hide_viewers = Hide viewers
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
event_feed_pause = Pause
event_feed_resume = Resume
event_feed_clear = Clear
event_feed_export = Export
event_feed_export_success = Exported event feed to { $path }
event_feed_export_failed = Event feed export failed: { $error }
event_feed_no_events = No events yet - system activity appears here in real time.
event_feed_no_filter_match = No events match the active filter.
event_feed_inspector_title = Event inspector
event_feed_inspector_hint = Select an event to inspect its payload.
event_feed_auto_scroll_on = Auto-scroll on
event_feed_auto_scroll_off = Auto-scroll off
event_feed_buffer = Buffer: { $count } / 10,000
event_feed_rate = { $rate } ev/s
event_feed_breadcrumb_automation = Automation
event_feed_breadcrumb_feed = Event Feed
event_feed_status_live = LIVE
event_feed_status_paused = PAUSED
event_feed_header_count = { $count } events
event_feed_streaming_status = Streaming · WebSocket :8081
event_feed_events_live_stream = events · live stream

## Globals - page header / filters

globals_breadcrumb_automation = Automation
globals_breadcrumb_globals = Globals
globals_filter_all = All
globals_filter_persisted = Persisted
globals_filter_session = Session
globals_search_placeholder = Search variables...
globals_export_btn = Export JSON
globals_new_btn = + New variable
globals_loading = Loading...
globals_empty_title = No globals here
globals_empty_desc = Adjust the filter or search, or create one with + New variable.
globals_edit_action = Edit value
globals_delete_action = Delete
globals_deleted_toast = Deleted '{ $name }'
globals_breadcrumb = Global variables
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

script_editor_breadcrumb = Script Editor
script_editor_breadcrumb_automation = Automation
script_editor_edited_prefix = edited
script_editor_run = Test run
script_editor_save = Save
script_editor_format = Format
script_editor_api_docs = API docs
script_editor_debug = Debug
script_editor_debug_tip = Debugger planned for post-1.0
script_editor_output_header = Output
script_editor_output_clear = Clear
script_editor_api_reference = API reference
script_editor_scripts_label = SCRIPTS
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
script_editor_new_btn = + New
script_editor_empty_title = Select a script or click + New
script_editor_empty_desc = Scripts let you run rhai code from any action.
script_editor_running = Running…
script_editor_run_modal_cancel = Cancel
script_editor_save_blocked = Save blocked - fix syntax errors first
script_editor_discard_title = Discard unsaved changes?
script_editor_discard_body = This script has unsaved edits. Continue and lose them, or stay to keep editing.
script_editor_discard_confirm = Discard
script_editor_discard_cancel = Keep editing
script_editor_discard_esc_hint = to keep editing
script_editor_shared = Shared
script_editor_sandbox_label = Sandbox:
script_editor_sandbox_enabled = enabled
script_editor_problems_tab = Problems
script_editor_console_cleared = Console cleared.
script_editor_no_problems = No problems.
script_editor_rename_placeholder = Script name

## Script Editor - run modal

script_editor_health = { $ok }/{ $total } healthy
script_editor_type_check_passed = Type-check passed
script_editor_type_check_errors = { $count ->
    [one] { $count } error
   *[other] { $count } errors
}
script_editor_run_modal_title = Run { $name }
script_editor_run_modal_title_generic = Run script
script_editor_run_input_placeholder = Enter { $label } value…
script_editor_run_input_error = Enter a value for { $name }

## Action telemetry - stat column headers

telemetry_stat_last_fired = LAST FIRED
telemetry_stat_runs_today = RUNS · TODAY
telemetry_stat_avg_time = AVG TIME
telemetry_stat_errors_7d = ERRORS · 7D

## Action editor - validation errors

action_editor_error_message_required = Message is required.
action_editor_error_var_required = Variable name is required.
action_editor_error_delay_invalid = Milliseconds must be a non-negative integer.
action_editor_error_log_required = Log message is required.
action_editor_error_clip_required = Select a clip to play.
action_editor_error_speak_required = Speak text is required.
action_editor_error_file_required = Path and target variable are required.
action_editor_error_random_invalid = min, max (min ≤ max), and target variable are required.
action_editor_pill_custom = Custom
action_editor_pill_default = Default

## Integration detail - OBS / quick-action

builtin_quick_action_fallback = Quick Action
builtin_obs_not_connected = OBS not connected
builtin_obs_not_supported = Not supported for OBS
builtin_disconnect_confirm_hint = You will be disconnected and will need to reconnect manually. Live events from this integration stop arriving until then.
integration_disconnect_title = Disconnect integration
integration_settings_coming_soon = Settings coming soon
integration_quick_action_na = N/A
integration_state_connecting_title = Connecting…
integration_state_connecting_detail = Establishing a session with this integration.
integration_state_reconnecting_title = Reconnecting…
integration_state_reconnecting_detail = The session dropped; forge is re-establishing it.
integration_state_disconnected_detail = Use Reconnect above to link this integration.

## OAuth / authentication errors

auth_error_credentials_missing_youtube = YouTube OAuth client credentials are not configured
auth_error_credentials_missing_kick = Kick OAuth client credentials are not configured
auth_error_flow_consumed = OAuth flow already consumed
auth_error_unknown = Unknown error

## Widget - key capture

widget_key_capture_placeholder = Press a combo…

## Widget - event inspector

widget_event_replay = Replay this event
widget_event_replaying = Replaying…
widget_event_payload_header = PAYLOAD
widget_event_caused_header = CAUSED

## Widget - chat row

widget_chat_subscribed = subscribed (Tier { $tier })
widget_chat_cheered = cheered
widget_chat_raiding_with = is raiding with
widget_chat_viewers = { $viewers } viewers
widget_chat_triggered = Triggered: { $action }

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
widget_header_uptime = uptime { $duration }
widget_header_uptime_only = uptime { $duration }
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

widget_file_list_header = Overlay host root
widget_file_list_path_label = PATH
widget_file_list_files_label = FILES
widget_file_list_url_label = BROWSER SOURCE URL
widget_file_list_dir_count =
    { $count ->
        [one] { $count } file
       *[other] { $count } files
    }


## Widget - server confirm modal

widget_confirm_what_this_means = WHAT THIS MEANS
widget_confirm_type_prefix = Type
widget_confirm_type_suffix = to confirm:
widget_confirm_esc_to_cancel = to cancel
widget_confirm_cancel = Cancel

## Widget - destructive confirm modal

widget_confirm_delete_title = Delete { $kind }?
widget_confirm_delete_hint = This item will be permanently removed. This action cannot be undone.
widget_confirm_delete_kind_action = action
widget_confirm_delete_kind_step = step
widget_confirm_delete_kind_trigger_link = trigger link
widget_confirm_delete_kind_global = global
widget_confirm_delete_kind_script = script
widget_confirm_delete_kind_client = client

## Widget - server bearer token

widget_bearer_copy = COPY
widget_bearer_regenerate = REGENERATE
widget_bearer_regen_warning = Regenerating disconnects all clients
widget_bearer_regen_warning_body = Connected WebSocket clients must reconnect with the new token.

## Widget - server bind card

widget_bind_badge_recommended = Recommended
widget_bind_badge_requires_confirmation = Requires confirmation

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

widget_console_no_output = No output yet

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

widget_layout_app_name = Forge
widget_layout_footer_app = forge
widget_layout_connected = { $connected }/{ $total } connected
widget_layout_uptime_suffix = uptime

## Widget - volume slider

widget_volume_label = VOL

## Locale-aware formatting - feed time

fmt_feed_time_pattern = %HH%:%MM%:%SS%.%mmm%

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
storage_error_title = Database could not be opened
storage_error_data_safe = Your on-disk data was not modified. The app is running on temporary storage, so changes made now will be lost on restart.
storage_error_report = This is a bug worth reporting.

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
