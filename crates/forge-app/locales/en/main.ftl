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
