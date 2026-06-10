## Common actions shared across all screens

common_cancel = Cancel
common_save = Save
common_language = Language

## Navigation — screen labels (breadcrumb + sidebar)

nav_home = Home
nav_actions = Actions
nav_queues = Queues
nav_triggers = Triggers
nav_platforms = Platforms
nav_stream_apps = Stream apps
nav_builtin = Builtin
nav_integration = Integration
nav_live_chat = Live chat
nav_event_feed = Event feed
nav_globals = Globals
nav_settings = Settings
nav_tts = TTS
nav_soundboard = Soundboard
nav_script_editor = Script editor
nav_api_reference = API reference
nav_server = Server
nav_logs = Logs

## Navigation — sidebar section headers

nav_section_audience = AUDIENCE
nav_section_automation = AUTOMATION
nav_section_connections = CONNECTIONS

## Navigation — sidebar item labels

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
nav_item_settings = Settings

## Navigation — coming-soon placeholder

nav_coming_soon = Coming soon

## Home — hero section

home_hero_tagline = Open-source stream automation, forged for streamers
home_hero_import = Import
home_hero_new_action = New action

## Home — jump cards

home_card_audience_section = AUDIENCE
home_card_audience_title = Chat
home_card_audience_stat_label = viewers tracked
home_card_audience_hint = Talk to your audience and see who's watching
home_card_automation_section = AUTOMATION
home_card_automation_title = Actions
home_card_automation_hint = Set up triggers, commands and timers
home_card_connections_section = CONNECTIONS
home_card_connections_title = Connections
home_card_connections_stat_label = connected
home_card_connections_hint = Manage platforms, apps and modules

## Home — stream health card

home_health_title = Stream health
home_health_live = LIVE
home_health_refresh_hint = last 60s · auto-refresh
home_health_throughput_label = THROUGHPUT · ev/s
home_health_bitrate_label = BITRATE · OBS
home_health_dropped_label = DROPPED · OBS
home_health_fps_label = FPS
home_health_cpu_label = CPU

## Home — connections strip

home_connections_title = Builtin

## Home — connection cell statuses

home_conn_connected = connected
home_conn_offline = offline

## Home — recent events card

home_events_title = Recent events
home_events_empty = No events yet

## Home — at-a-glance card

home_glance_title = At a glance
home_glance_actions = Actions
home_glance_fired = Fired this session
home_glance_globals = Globals

## Home — actions fired stat label (count + fired today)

home_card_automation_stat_label = { $count ->
    [one] { $count } action · { $fired } fired today
   *[other] { $count } actions · { $fired } fired today
}

## Home — connections active/disconnected counts

home_connections_summary = { $active } active · { $disconnected } disconnected

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
settings_coming_soon_placeholder = Coming soon.

## Settings → Diagnostics pane

settings_about_build_label = Build
settings_diagnostics_section_title = Logs & diagnostics
settings_diagnostics_log_dir = Log directory: { $path }
settings_diagnostics_open_log_dir = Open log directory
settings_diagnostics_log_level_hint = Log level: controlled via RUST_LOG env var (e.g. info, debug, trace).

## Settings → Storage pane

settings_storage_section_title = Storage & backups
settings_storage_db_path = Database: { $path }
settings_storage_vacuum_btn = Vacuum (export compact snapshot)
settings_storage_vacuum_hint = Writes a vacuumed snapshot to a temp file; useful before manual backups.
settings_storage_backup_btn = Backup now
settings_storage_backup_hint = Creates a timestamped DB copy in the data directory.

## Settings → Queues pane

settings_queues_section_title = Queues & threading
settings_queues_thread_hint = Tokio threadpool: { $workers } worker(s) (auto-sized to system).
settings_queues_managed_hint = Per-queue concurrency limits and blocking flags are managed on the Queues screen.

## Settings → Notifications pane

settings_notifications_section_title = Notifications
settings_notifications_hint = Per-event-type toast customisation coming later. Errors and connection changes always surface in the status bar.

## Settings → Shortcuts pane

settings_shortcuts_title = Shortcuts
settings_shortcuts_subtitle = Quick keys across Forge
settings_shortcuts_note = Keyboard shortcuts not yet bound — labels only for now.
settings_shortcut_save = Save
settings_shortcut_new_action = New action
settings_shortcut_quick_switcher = Quick switcher
settings_shortcut_toggle_chat = Toggle Live Chat
settings_shortcut_toggle_events = Toggle Event Feed
settings_shortcut_run_script = Run script

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
settings_ws_port_section_title = Port
settings_ws_port_subtitle = Default 8081 · range 1024–65535
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
settings_ws_lan_modal_explanation = You're switching from 127.0.0.1 (localhost only) to 0.0.0.0 (all network interfaces). Other devices on your LAN — and anyone on the same Wi-Fi — will be able to reach the Forge server.
settings_ws_lan_modal_confirm_label = Expose to LAN
settings_ws_lan_bullet_phone = Phone / tablet / second PC can connect to overlays and the WS API
settings_ws_lan_bullet_token_warning = Anyone on your network can read all events and send chat messages if they know your bearer token
settings_ws_lan_bullet_public_wifi = If you're on public Wi-Fi (café, conference, hotel), do not enable this
settings_ws_lan_bullet_firewall = Your firewall must also allow the configured port for this to work

## Settings → Hotkeys pane

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

## Actions — page header / breadcrumb

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

## Actions — detail panel

actions_detail_empty_title = No action selected
actions_detail_empty_hint = Select an action from the list to view its details.
actions_detail_loading = Loading...
actions_detail_enabled = Enabled
actions_detail_disabled = Disabled
actions_detail_test_run = Test run
actions_detail_duplicate = Duplicate
actions_detail_section_triggers = TRIGGERS · { $count }
actions_detail_section_sub_actions = SUB-ACTIONS · { $count }
actions_detail_add_trigger = Add trigger
actions_detail_add_sub_action = Add sub-action
actions_detail_no_triggers = No triggers — this action will never fire on its own
actions_detail_no_steps = No steps yet — add one

## Actions — context menu

actions_menu_rename = Rename…
actions_menu_duplicate = Duplicate
actions_menu_enable = Enable
actions_menu_disable = Disable
actions_menu_delete = Delete…

## Actions — footer

actions_footer_showing = Showing { $visible } of { $total } · grouped by trigger
actions_footer_storage = Storage: —
actions_footer_autosaved = Auto-saved just now

## Actions — ESC hint

actions_esc_hint = ESC to cancel

## Actions — add-action modal

actions_modal_new_action_title = New action
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
actions_modal_cancel_btn = Cancel

## Actions — add-sub-action modal / step chips

actions_sub_modal_add_title = Add step
actions_sub_modal_edit_title = Edit step
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

## Actions — sub-action config sections

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
actions_sub_no_clips = No clips yet — add one in the Soundboard screen first.

## Actions — trigger picker (side-sheet)

actions_picker_title = Add trigger
actions_picker_loading = Loading triggers…
actions_picker_cancel = Cancel
actions_picker_select_platform = Select a platform
actions_picker_no_triggers = No triggers available
actions_picker_select_hint = Select a platform to browse triggers.
actions_picker_no_triggers_selection = No triggers available for this selection.
actions_picker_default_label = (default)

## Actions — trigger category display names (section headers)

actions_cat_chat_commands = CHAT COMMANDS
actions_cat_subs_bits = SUBS & BITS
actions_cat_bits = BITS
actions_cat_raids = RAIDS
actions_cat_obs_events = OBS EVENTS
actions_cat_server_events = SERVER EVENTS
actions_cat_timers = TIMERS
actions_cat_ungrouped = UNGROUPED
actions_cat_all = ALL

## Actions — trigger kind labels

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

## Actions — trigger kind summaries

actions_summary_twitch_chat_command = User types !command in chat
actions_summary_twitch_chat_message = Every chat message fires this
actions_summary_twitch_subscriber = Fires when someone subscribes
actions_summary_twitch_resubscriber = Existing sub renews
actions_summary_twitch_gift_sub = Someone gifts subs to channel
actions_summary_twitch_cheer = Viewer sends bits
actions_summary_twitch_raid = Another stream raids you
actions_summary_obs_scene_changed = Fires when OBS switches the active scene
actions_summary_server_custom_event = Fires when triggerCodeEvent is called via the WebSocket API

## Action editor — breadcrumb / tree pane / detail pane

action_editor_breadcrumb_automation = Automation
action_editor_breadcrumb_actions = Actions
action_editor_loading = Loading action…
action_editor_no_description = No description
action_editor_test_run = Test run
action_editor_duplicate = Duplicate
action_editor_add_trigger = Add trigger
action_editor_add_step = Add step
action_editor_no_triggers = No triggers · click Add trigger to start
action_editor_delete = Delete
action_editor_section_triggers = TRIGGERS
action_editor_section_sub_actions = SUB-ACTIONS · { $count }
action_editor_sub_count = { $count } sub
action_editor_enabled = Enabled
action_editor_disabled = Disabled

## Action editor — step menu

action_editor_step_menu_edit = Edit step…
action_editor_step_menu_duplicate = Duplicate
action_editor_step_menu_move_top = Move to top
action_editor_step_menu_move_bottom = Move to bottom
action_editor_step_menu_delete = Delete step

## Action editor — sub-action card titles

action_editor_kind_send_chat = Send chat message
action_editor_kind_set_global = Set global
action_editor_kind_delay = Delay
action_editor_kind_log = Log
action_editor_kind_play_sound = Play sound
action_editor_kind_speak = Speak
action_editor_kind_read_file = Read file
action_editor_kind_random_int = Random int
action_editor_kind_sub_action = Sub-action

## Triggers registry — page header / filters

triggers_breadcrumb_automation = Automation
triggers_breadcrumb_triggers = Triggers
triggers_open_create_btn = + Create
triggers_search_placeholder = Search triggers…
triggers_filter_twitch = Twitch
triggers_filter_obs = OBS
triggers_filter_script = Script
triggers_filter_all = All
triggers_usage_all = All
triggers_usage_used = Used
triggers_usage_unused = Unused

## Triggers registry — list / empty states

triggers_empty_title = No custom trigger instances yet
triggers_empty_hint = Create a named trigger with custom settings to reuse across multiple actions.
triggers_empty_create = + Create trigger instance
triggers_no_results_title = No results
triggers_no_results_hint = Adjust or clear the filters to find your triggers.
triggers_clear_filters = Clear filters
triggers_usage_badge = used in { $count }
triggers_toggle_on = ON
triggers_toggle_off = OFF

## Triggers registry — sheet detail

triggers_sheet_section_configuration = CONFIGURATION
triggers_sheet_no_config = No configurable fields
triggers_sheet_not_registered = Trigger kind not registered
triggers_sheet_section_used_in = USED IN
triggers_sheet_section_platform = PLATFORM
triggers_sheet_delete_btn = Delete
triggers_sheet_any_platform = Any platform
triggers_sheet_will_fire_on = Will fire on: { $platform }
triggers_sheet_will_fire_on_scope = Will fire on: { $scope }

## Triggers registry — confirm-disable dialog

triggers_confirm_disable_body = Disabling this trigger will pause it for { $count } action(s). Continue?
triggers_confirm_disable_dismiss = Cancel
triggers_confirm_disable_accept = Disable anyway

## Triggers create form — kind picker

triggers_create_select_kind = Select trigger kind
triggers_create_search_placeholder = Search kinds…
triggers_create_no_results = No matching trigger kinds
triggers_create_cancel = Cancel

## Triggers create form — fill form

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

## Settings → Scripting pane

settings_scripting_title = Scripting (Rhai)
settings_scripting_all_saved = All changes saved
settings_scripting_saving = Saving…
settings_scripting_unsaved = Unsaved changes
settings_scripting_save_failed = Save failed: { $error }
settings_scripting_engine_section = Engine Limits
settings_scripting_op_limit_label = Op-count limit
settings_scripting_op_limit_hint = Range 1 000 – 10 000 000 (default 100 000)
settings_scripting_engine_timeout_label = Timeout (ms)
settings_scripting_engine_timeout_hint = Range 50 – 10 000 (default 500)
settings_scripting_http_section = HTTP Sandbox
settings_scripting_allowed_domains_label = Allowed domains
settings_scripting_allowed_domains_hint = Requests to unlisted domains are blocked. Wildcards: *.example.com
settings_scripting_domains_placeholder = e.g. api.example.com
settings_scripting_max_calls_label = Max calls per script
settings_scripting_max_calls_hint = Range 1 – 100 (default 10)
settings_scripting_http_timeout_label = Request timeout (ms)
settings_scripting_http_timeout_hint = Range 100 – 30 000 (default 5 000)
settings_scripting_max_response_label = Max response size (KiB)
settings_scripting_max_response_hint = Range 1 – 10 240 (default 1 024 KiB = 1 MiB)
settings_scripting_allow_local_label = Allow localhost / private IPs
settings_scripting_allow_local_description = Disables SSRF protections. Only enable for local development.
settings_scripting_ssrf_warning = WARNING — disables SSRF protections. Only enable for local development.

## Actions trigger picker — category labels

trigger_cat_chat = Chat
trigger_cat_subscriptions = Subscriptions
trigger_cat_bits = Bits
trigger_cat_raids = Raids
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

## Actions modals — placeholder literals

actions_name_placeholder = My automation
actions_group_placeholder = Examples
actions_description_placeholder = Plays a sound, shows overlay alert…
actions_log_message_placeholder = Action started
actions_speak_text_placeholder = Text to speak…

## Triggers registry — error messages

triggers_delete_reference_block = Remove this trigger from all actions before deleting.

## TTS — tab bar section labels

tts_tab_dashboard = Dashboard
tts_tab_engines = Engines
tts_tab_aliases = Voice aliases
tts_tab_filters = Filters
tts_tab_triggers = Triggers
tts_tab_cloud_engines = Cloud engines

## TTS — breadcrumb

tts_breadcrumb_builtin = Builtin
tts_breadcrumb_tts = TTS

## TTS Dashboard — control strip

tts_dash_pause_btn = Pause queue
tts_dash_resume_btn = Resume
tts_dash_skip_btn = Skip
tts_dash_stop_all_btn = Stop all
tts_dash_test_placeholder = Type to test a voice…
tts_dash_speak_btn = Speak

## TTS Dashboard — now speaking

tts_dash_now_speaking_header = NOW SPEAKING
tts_dash_no_speaking = —

## TTS Dashboard — queue

tts_dash_queue_header = Up next
tts_dash_queue_empty = Queue is empty

## TTS Dashboard — session stats

tts_dash_session_header = SESSION
tts_dash_stat_spoken = Spoken
tts_dash_stat_skipped = Skipped
tts_dash_stat_filtered = Filtered
tts_dash_stat_avg_latency = Avg latency
tts_dash_engines_header = ENGINES

## TTS Engines — list

tts_engines_header_prefix = CONFIGURED
tts_engines_more_placeholder = + More engines in future releases
tts_engines_select_hint = Select an engine to configure
tts_engines_status_ready = Ready

## TTS Engines — detail header

tts_engines_local_meta = local TTS engine
tts_engines_default_badge = DEFAULT

## TTS Engines — sections

tts_engines_section_engine = ENGINE
tts_engines_credentials_notice = Credentials stored in system keyring, never in config files
tts_engines_no_credentials = LOCAL — no credentials
tts_engines_section_params = DEFAULT VOICE PARAMETERS
tts_engines_param_pitch = Pitch
tts_engines_param_speed = Speed
tts_engines_param_volume = Volume

## TTS Engines — voices section

tts_engines_voices_header_prefix = AVAILABLE VOICES
tts_engines_voices_filter_placeholder = Filter voices…

## TTS Filters — pipeline column

tts_filters_pipeline_header = PROCESSING PIPELINE
tts_filters_pipeline_hint = Each message passes through these stages in order before being spoken

## TTS Filters — stage titles / subtitles

tts_filters_stage_skip_title = Skip rules
tts_filters_stage_skip_subtitle = message dropped if matched
tts_filters_stage_blocklist_title = Word blocklist
tts_filters_stage_replacements_title = Text replacements
tts_filters_stage_engine_title = Sent to voice engine
tts_filters_stage_words_count = { $count ->
    [one] { $count } word
   *[other] { $count } words
}
tts_filters_stage_rules_count = { $count ->
    [one] { $count } rule
   *[other] { $count } rules
}

## TTS Filters — skip rule chips

tts_filters_chip_contains_url = Contains URL
tts_filters_chip_starts_bang = Starts with !
tts_filters_chip_from_bots = From bots
tts_filters_chip_add_rule = + Add rule

## TTS Filters — blocklist

tts_filters_blocklist_manage = Manage blocklist…
tts_filters_mode_censor = Censor
tts_filters_mode_skip = Skip msg

## TTS Filters — replacements

tts_filters_no_replacements = No replacement rules

## TTS Filters — preview column

tts_filters_preview_header = PIPELINE PREVIEW
tts_filters_preview_input_label = INPUT MESSAGE
tts_filters_preview_input_placeholder = Type a message to preview…
tts_filters_preview_empty = Enter a message above to preview
tts_filters_preview_output_label = FINAL OUTPUT
tts_filters_speak_preview_btn = Speak preview
tts_filters_preview_tip = Type any message above to see how filters transform it in real time

## TTS Triggers — header

tts_triggers_header = WHAT GETS SPOKEN
tts_triggers_hint = Enable sources and set who can trigger them

## TTS Triggers — command card

tts_triggers_cmd_title = Chat command
tts_triggers_cmd_subtitle = !tts <message>
tts_triggers_cmd_meta = cooldown 8s · max 250 chars

## TTS Triggers — channel points card

tts_triggers_points_title = Channel point reward
tts_triggers_points_subtitle = "Speak my message" · 500 pts
tts_triggers_points_meta = no cooldown · priority queue

## TTS Triggers — bits card

tts_triggers_bits_title = Bits / cheers
tts_triggers_bits_subtitle = Speak cheer message
tts_triggers_bits_min_label = Minimum
tts_triggers_bits_meta = louder = longer message

## TTS Triggers — sub messages card

tts_triggers_subs_title = Sub messages
tts_triggers_subs_subtitle = Speak resub / gift messages
tts_triggers_subs_disabled = Disabled — toggle to enable

## TTS Triggers — format card

tts_triggers_format_header = MESSAGE FORMAT
tts_triggers_format_read_username = Read username before message
tts_triggers_format_template_header = TEMPLATE
tts_triggers_format_speak_emotes = Speak emotes as words

## TTS Triggers — queue behavior card

tts_triggers_queue_header = QUEUE BEHAVIOR
tts_triggers_queue_max_length = Max queue length
tts_triggers_queue_per_user_limit = Per-user limit in queue
tts_triggers_queue_bits_skip = Bits & points skip the line

## TTS Triggers — role chips

tts_triggers_role_subscribers = Subscribers
tts_triggers_role_vips = VIPs
tts_triggers_role_mods = Mods
tts_triggers_role_everyone = Everyone

## Cloud TTS Engines — header

tts_cloud_header = CLOUD ENGINES · 4

## Cloud TTS Engines — card buttons

tts_cloud_test_connection_btn = Test connection
tts_cloud_testing_btn = Testing…
tts_cloud_save_credentials_btn = Save credentials

## Cloud TTS Engines — status badges

tts_cloud_not_configured = NOT CONFIGURED
tts_cloud_configured = CONFIGURED
tts_cloud_connection_failed = CONNECTION FAILED

## Cloud TTS Engines — test result

tts_cloud_connection_verified = Connection verified

## Cloud TTS Engines — toast messages

tts_cloud_saved_toast = Restart app to enable the { $name } engine.
tts_cloud_save_failed_toast = Failed to save { $name } credentials: { $error }

## Voice Aliases — strategy banner

tts_aliases_strategy_label = Default assignment strategy
tts_aliases_strategy_deterministic = Deterministic by name
tts_aliases_strategy_random = Random
tts_aliases_strategy_single = Single voice

## Voice Aliases — toolbar

tts_aliases_search_placeholder = Search viewers…
tts_aliases_count = { $count ->
    [one] { $count } manual alias
   *[other] { $count } manual aliases
}
tts_aliases_assign_btn = Assign voice

## Voice Aliases — table headers

tts_aliases_col_viewer = VIEWER
tts_aliases_col_voice = VOICE
tts_aliases_col_pitch = PITCH
tts_aliases_col_speed = SPEED
tts_aliases_col_actions = ACTIONS

## Voice Aliases — empty state

tts_aliases_empty = No voice aliases configured

## Voice Aliases — blocked row

tts_aliases_never_speak = Never speak

## TTS Voice aliases — role badges

tts_aliases_role_mod = MOD
tts_aliases_role_vip = VIP
tts_aliases_role_sub = SUB
tts_aliases_role_blocked = BLOCKED

## Soundboard — breadcrumb

soundboard_breadcrumb_builtin = Builtin
soundboard_breadcrumb_soundboard = Soundboard

## Soundboard — header / modal

soundboard_add_clip_btn = Add clip
soundboard_loading = Loading clips…
soundboard_empty_title = No clips yet
soundboard_empty_hint = Click "Add clip" to add your first sound.
soundboard_playback_error_prefix = Playback error: { $error }

## Soundboard — modal

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

## Soundboard — modal section labels

soundboard_modal_section_file = FILE
soundboard_modal_section_name = NAME
soundboard_modal_section_hotkey = HOTKEY
soundboard_modal_section_device = OUTPUT DEVICE
soundboard_modal_section_volume = VOLUME

## Soundboard — device load error

soundboard_modal_device_load_error = Device load failed: { $error }

## Soundboard — audio player error

soundboard_player_not_init = Audio player not initialised — check Settings → Audio.

## Queues — page header

queues_breadcrumb_automation = Automation
queues_breadcrumb_queues = Queues
queues_pause_all_btn = Pause all
queues_new_queue_btn = New queue
queues_empty = No queues configured.
queues_configure_btn = Configure
queues_drain_btn = Drain
queues_pause_btn = Pause
queues_resume_btn = Resume

## Queues — card metrics

queues_metric_concurrency = CONCURRENCY
queues_metric_pending = PENDING
queues_metric_actions = ACTIONS
queues_metric_assigned = assigned
queues_metric_serial = serial
queues_metric_parallel = parallel
queues_metric_in_flight = in flight
queues_metric_idle = idle
queues_metric_held = held

## Queues — paused panel

queues_paused_with_time = { $pending } actions waiting — paused { $mins } min ago
queues_paused_simple = Queue is paused

## Queues — running panel

queues_running_now_header = RUNNING NOW
queues_no_actions_running = No actions running
queues_running_label = running —

## Queues — status badge

queues_status_paused = PAUSED
queues_status_running = RUNNING

## Queues — overflow pill

queues_overflow_more = +{ $count } more

## Queues — built-in queue descriptions

queues_desc_default = Catch-all queue for actions without explicit queue assignment
queues_desc_alerts = Subs, raids, cheers · serialized so overlays don't overlap
queues_desc_background = Logging, analytics, side-effect-free tasks · parallel execution
queues_desc_moderation = Auto-bans, timeouts, message deletions · paused for review

## TTS dashboard — engine card sublabels / priority badge

tts_dash_engine_local_ready = local · ready
tts_dash_priority_high = HIGH

## TTS engines — unknown engine fallback

tts_engines_unknown = Unknown engine

## TTS filters — pipeline stage fallback label

tts_filters_stage_fallback = STAGE

## Cloud TTS — form field labels

tts_cloud_field_api_key = API Key
tts_cloud_field_region = Region
tts_cloud_field_access_key_id = Access key ID
tts_cloud_field_secret_key = Secret key
tts_cloud_field_placeholder_subscription_key = Subscription key

## Soundboard — file-dialog filter

soundboard_file_filter_audio = Audio
