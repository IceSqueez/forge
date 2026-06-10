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
