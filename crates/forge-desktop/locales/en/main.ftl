## Boot - splash and data-open failure screens

boot_starting = Starting...
boot_upgrade_title = Update required
boot_upgrade_body = Your forge data uses schema version { $found }, newer than this build's version { $expected }. Update forge to the latest release to open it.
boot_upgrade_reassure = Your data is safe and untouched.
boot_retry = Retry
boot_failure_title = Couldn't open your data
boot_failure_reassure = Your data is safe. If this keeps happening, please report it.
boot_pre_baseline_title = Open your data in an earlier version first
boot_pre_baseline_body = This data folder was last used by a forge older than { $release } (database schema version { $found }). Open it once in forge { $release } first, then start this version again.
boot_pre_baseline_reassure = Nothing was changed. Your data is untouched.
boot_pre_baseline_open_folder = Open data folder
boot_pre_baseline_quit = Quit

## Common actions shared across all screens

common_cancel = Cancel
common_save = Save
common_copy = Copy

## Navigation - screen labels (breadcrumb + sidebar)

nav_home = Home
nav_event_feed = Event feed
nav_script_editor = Scripts

## Navigation - sidebar section headers

nav_section_audience = AUDIENCE
nav_section_automation = AUTOMATION

## Navigation - sidebar item labels

nav_item_home = Home
nav_item_chat = Chat
nav_item_actions = Actions
nav_item_triggers = Triggers
nav_item_queues = Queues
nav_item_event_feed = Event feed
nav_item_globals = Globals
nav_item_soundboard = Soundboard
nav_item_tts = Text-to-Speech
nav_item_overlays = Overlays
nav_item_ws_server = WebSocket server
nav_item_hotkey = Hotkeys
nav_item_settings = Settings

## Home - hero section

home_hero_tagline = Open-source stream automation, forged for streamers
home_hero_import = Import
home_hero_export = Export
home_export_full = Full backup
home_export_pack = Pack...
pack_export_title = Export pack
pack_export_subtitle = Share actions with everything they need
pack_export_search_placeholder = Filter actions
pack_export_actions_label = Actions
pack_export_selected_count = { $count } selected
pack_export_loading = Loading actions...
pack_export_load_failed = Could not load actions: { $error }
pack_export_no_actions = No actions to export yet
pack_export_no_matches = No actions match the filter
pack_export_name_label = Name
pack_export_name_placeholder = Pack name
pack_export_name_required = Required
pack_export_author_label = Author
pack_export_author_placeholder = Optional
pack_export_description_label = Description
pack_export_description_placeholder = Optional
pack_export_summary_label = Pack contents
pack_export_summary_empty = Pick at least one action
pack_export_summary_resolving = Collecting dependencies...
pack_export_summary_failed = Could not collect dependencies: { $error }
pack_export_kind_actions = Actions
pack_export_kind_triggers = Triggers
pack_export_kind_queues = Queues
pack_export_kind_scripts = Scripts
pack_export_kind_globals = Globals
pack_export_kind_clips = Sound clips
pack_export_kind_overlays = Overlays
pack_export_kind_media = Media files
pack_export_integrations_label = Required integrations
pack_export_integrations_none = None
pack_export_unresolved = References to missing items: { $count }
pack_export_names_not_followed = Names inside scripts and text fields are not followed, so the pack may miss what they refer to
pack_export_confirm_btn = Export
pack_export_dialog_filter = forge packs
pack_export_done = Pack saved to { $path }
pack_export_failed = Pack export failed: { $error }
pack_import_title = Import pack
pack_import_subtitle = Review what the pack adds before anything is written
pack_import_items_label = Pack items
pack_import_item_count = { $count } items
pack_import_empty = This pack has no items
pack_import_pack_label = Pack
pack_import_author = by { $author }
pack_import_author_unknown = Author not given
pack_import_conflict_none = new
pack_import_conflict_id = same id
pack_import_conflict_name = same name
pack_import_conflict_archived = archived
pack_import_resolution_skip = Skip
pack_import_resolution_replace = Replace
pack_import_resolution_keep_both = Keep both
pack_import_warnings_label = Warnings
pack_import_warning_integration = Needs { $integration }, which this forge does not have
pack_import_warning_unknown_kind = Uses "{ $kind }", which this forge does not know
pack_import_warning_unresolved = Refers to { $namespace } "{ $value }", which the pack does not carry
pack_import_warning_external = Refers to { $namespace } "{ $value }" outside the pack
pack_import_warning_clip_silent = Sound clip "{ $clip }" comes without its sound file
pack_import_runs_code_notice = This pack contains content that runs code. Review it before you import:
pack_import_runs_script = Script "{ $name }"
pack_import_runs_http = HTTP step in action "{ $action }"
pack_import_runs_script_step = Script step in action "{ $action }"
pack_import_runs_overlay = Overlay "{ $overlay }": { $file }
pack_import_limit_names = Names inside scripts and text fields are not followed, so they may refer to things this pack does not carry
pack_import_snapshot_note = Some items replace existing ones, so a safety backup of your data is taken first
pack_import_confirm_btn = Import
pack_import_applying = Importing...
pack_import_done = Pack imported: { $written } written, { $skipped } skipped
pack_import_done_restart = Pack imported: { $written } written, { $skipped } skipped. Restart forge to apply the rest
pack_import_reveal_snapshot = Show backup
pack_import_stale = forge data changed since the preview was built. Review the updated preview and import again
pack_import_failed = Nothing was imported: { $error }
pack_import_rollback_incomplete = The import failed and could not be fully undone: { $error }. Restore the safety backup from { $folder }
pack_import_rollback_incomplete_no_snapshot = The import failed and could not be fully undone: { $error }
home_export_setup = Setup...
setup_export_title = Export setup
setup_export_subtitle = Move your configuration to another machine
setup_export_carries = A setup carries your whole configuration: actions, triggers, queues, scripts, globals, sound clips, overlays, media, speech settings, voice aliases, hotkeys and settings.
setup_export_never_logins = It never carries logins. Connect your accounts again on the new machine.
setup_export_never_history = It never carries history: events, runs, chat, viewers, donations and bans stay here.
setup_export_never_machine = It never carries machine-specific settings such as audio devices, folders, window placement, the network server and HTTP trust. Set them again on the new machine.
setup_export_continue = Choose file...
setup_export_dialog_filter = forge setups
setup_export_done = Setup saved to { $path }
setup_export_failed = Setup export failed: { $error }
setup_confirm_title = Import this setup?
setup_confirm_subtitle = Your configuration will be replaced by the setup
setup_confirm_part_voice_aliases = Voice aliases
setup_confirm_part_filter_rules = Speech filter rules
setup_confirm_part_clip_hotkeys = Clip hotkeys
setup_confirm_part_settings = Settings
setup_confirm_replaces = ALL current configuration is replaced. Items that are not in the setup are removed.
setup_confirm_keeps = Logins, history and machine-specific settings stay as they are on this machine.
setup_confirm_snapshot = A safety backup of your current data is taken first.
setup_confirm_btn = Replace configuration
setup_confirm_btn_busy = Importing...
setup_import_failed = Nothing was changed: { $error }
setup_import_done = Setup imported
setup_import_done_restart = Setup imported. Restart forge to apply all of it
setup_import_restart = Restart
setup_import_cleanup_failed = Setup imported, but { $count } leftover files could not be removed. See the log for details
setup_import_rollback_incomplete = The setup import failed and could not be fully undone: { $error }. Restore the safety backup { $snapshot }
home_import_dialog_filter = forge files
home_hero_new_action = New action
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

## Home - event loss card and footer badge

event_loss_title = Event loss
event_loss_scope_hint = since start · resets on restart
event_loss_none = No events lost since start
event_loss_footer_badge = { $count } dropped
footer_connected = { $connected }/{ $total } connected
footer_uptime = { $uptime } uptime
event_loss_tooltip_title = Events lost since start
event_loss_tooltip_row = { $consumer } ({ $tier }) - { $detail }
stay_awake_footer_label = Awake
stay_awake_tooltip_both = Stay awake: display and system kept awake
stay_awake_tooltip_display_only = Stay awake: display kept awake
stay_awake_tooltip_system_only = Stay awake: system kept awake
stay_awake_tooltip_starting = Stay awake: starting
stay_awake_tooltip_inactive = Stay awake: not active
stay_awake_tooltip_display_refused = Display: unavailable - { $reason }
stay_awake_tooltip_system_refused = System: unavailable - { $reason }
event_loss_tier_critical = lossless
event_loss_tier_observer = display
event_loss_part_priority = { $count ->
    [one] { $count } priority dropped
   *[other] { $count } priority dropped
}
event_loss_part_bulk = { $count ->
    [one] { $count } bulk dropped
   *[other] { $count } bulk dropped
}
event_loss_part_skipped = { $count ->
    [one] { $count } skipped
   *[other] { $count } skipped
}
event_loss_part_unwritten = { $count ->
    [one] { $count } not saved
   *[other] { $count } not saved
}
event_loss_consumer_trigger_evaluator = Trigger evaluation
event_loss_consumer_event_log = Event log
event_loss_consumer_chat_history = Chat history
event_loss_consumer_viewer_tracker = Viewer tracking
event_loss_consumer_latest_values = Latest values
event_loss_consumer_latest_overlays = Latest value overlays
event_loss_consumer_run_history = Run history
event_loss_consumer_event_trail = Event trail
event_loss_consumer_observer = Other views
event_loss_consumer_ui_events = App views
event_loss_consumer_ui_chat_feed = Chat feed
event_loss_consumer_ui_chat_display = Chat feed display

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
settings_appearance_accent_label = Accent color
settings_appearance_accent_hint = Primary highlight across the UI
settings_accent_persist_failed = Failed to save accent color
settings_appearance_font_interface = INTERFACE
settings_appearance_font_monospace = MONOSPACE
settings_appearance_font_search = Search installed fonts
settings_appearance_font_default_body = Default (Inter)
settings_appearance_font_default_mono = Default (JetBrains Mono)
settings_appearance_font_persist_failed = Failed to save font
settings_theme_desc_dark = dark
settings_theme_desc_storm = Storm
settings_theme_desc_light_mode = Light mode

## Settings → Language pane

settings_language_subtitle = Choose how Forge talks to you
settings_region_region = Region
settings_region_date_format = Date format
settings_region_time_format = Time format
settings_region_first_day = First day of week
settings_region_default = Region default ({ $value })
settings_region_persist_failed = Could not save the region setting

## Settings → navigation sidebar

settings_page_title = Settings
settings_nav_group_preferences = PREFERENCES
settings_nav_group_engine = ENGINE
settings_nav_group_about = ABOUT
settings_nav_appearance = Appearance
settings_nav_general = General
settings_stay_awake_label = Stay awake while forge is running
settings_stay_awake_hint = Keeps the display and the system from sleeping while forge runs
settings_nav_language_region = Language & region
settings_nav_shortcuts = Shortcuts
settings_nav_notifications = Notifications
settings_notifications_subtitle = What Forge tells you about
settings_notifications_action_failure_label = Show desktop toasts on errors
settings_notifications_action_failure_hint = OS-native notification on action failure
settings_notifications_connection_loss_label = Beep on disconnect
settings_notifications_connection_loss_hint = System sound when a platform/app loses connection
settings_notifications_scheduled_backup_failure_label = Notify when a scheduled backup fails
settings_notifications_scheduled_backup_failure_hint = OS-native notification on the first failure of a scheduled backup
settings_notifications_persist_failed = Could not save the notification setting
settings_nav_audio = Audio

## Settings → Diagnostics pane

settings_diagnostics_section_title = Logs & diagnostics
settings_diagnostics_subtitle = Tail Forge runtime logs and export for support
settings_diagnostics_open_log_dir = Open logs folder
settings_diagnostics_export_bundle = Export diagnostic bundle
settings_diagnostics_clear_logs = Clear logs
settings_diagnostics_tail_empty = No log lines captured yet.
settings_diagnostics_clear_confirm_title = Clear logs?
settings_diagnostics_clear_confirm_body = Archived log files are deleted and today's log is emptied. This cannot be undone.
settings_diagnostics_clear_confirm_action = Clear logs
settings_diagnostics_cleared = Logs cleared
settings_diagnostics_clear_failed = Could not clear logs: { $error }
settings_diagnostics_exported = Diagnostic bundle saved to { $path }
settings_diagnostics_export_failed = Could not export diagnostic bundle: { $error }
settings_diagnostics_level_label = Log level
settings_diagnostics_level_hint = Applies right away, no restart. Debug and trace grow the log fast on a busy stream.
settings_diagnostics_level_env_locked = The RUST_LOG environment variable is in charge for this run, so the level cannot be changed here.
settings_diagnostics_level_env_hatch = Logging starts at the default level until settings load, so the first moments of a launch are never captured at a raised level. To catch a startup bug, launch Forge with RUST_LOG=debug (or trace) set in the environment.
settings_diagnostics_level_targets = A trace reproduction stays readable if you raise one component instead of everything: RUST_LOG=info,forge::event=debug for the event trail, forge::trigger=debug for why a trigger did or did not fire, forge::command=trace for the full matched command line including what viewers typed.
settings_diagnostics_level_apply_failed = Could not switch the log level for this run.
settings_diagnostics_export_preview_title = Read this before you export
settings_diagnostics_export_preview_lead = This file is built to be attached to a public issue. Open it and read it before you post it anywhere.
settings_diagnostics_export_preview_includes = It contains the Forge version, your OS and architecture, which integrations are installed and whether they are connected, your settings, the last { $runs } action runs, and the log itself.
settings_diagnostics_export_preview_excludes = It withholds access tokens and keys, account and channel names, endpoints and file paths, and chat messages. Withheld values read as { $stamp } - that is deliberate, not a truncated read.
settings_diagnostics_export_preview_lengths = Settings and action runs carry lengths and kinds instead of values, so nothing that was typed can be read back out of them.
settings_diagnostics_export_preview_script = { $count } lines in this file were written by the log step in your own actions and scripts. Forge did not choose that text - it sits in its own section so you can read it.
settings_diagnostics_export_preview_script_none = No lines in this file came from the log step in your own actions and scripts.
settings_diagnostics_export_preview_trace = Trace logging was active: this file contains { $count } full command lines, including everything viewers typed as arguments to your commands. Read them before you publish it.
settings_diagnostics_export_preview_trace_none = The log level is trace. No command has matched yet, but at trace every matched command is written to the log in full, including everything viewers type as arguments.
settings_diagnostics_export_preview_size = About { $size } KB.
settings_diagnostics_export_preview_elided = { $size } KB of older log lines did not fit the size limit and were left out; the newest lines were kept.
settings_diagnostics_export_preview_confirm = Choose a file and export
settings_diagnostics_level_trace = Trace
settings_diagnostics_level_debug = Debug
settings_diagnostics_level_info = Info
settings_diagnostics_level_warn = Warn
settings_diagnostics_level_error = Error

## Settings → About pane

settings_about_title = About
settings_about_system_title = System info
settings_about_system_hint = Paste this into a bug report. It never includes host or user names, paths or network addresses.
settings_about_collecting = Collecting system info...
settings_about_unknown = unknown
settings_about_field_forge = forge
settings_about_field_build = Build
settings_about_field_os = OS
settings_about_field_kernel = Kernel
settings_about_field_cpu = CPU
settings_about_field_memory = Memory
settings_about_field_gpu = GPU
settings_about_field_gpu_backend = GPU backend
settings_about_field_displays = Displays
settings_about_field_audio_outputs = Audio outputs
settings_about_field_data_dir = Data folder size
settings_about_report_problem = Report a problem
crash_notice = forge closed unexpectedly last time.
crash_notice_report = Report
crash_notice_dismiss = Dismiss
problem_report_title = Report a problem
problem_report_crash_title = Report the crash
problem_report_lead = forge saves a diagnostic bundle to a folder and opens a new GitHub issue in your browser with your forge version and system info filled in. Nothing is sent until you submit the issue yourself.
problem_report_crash_lead = forge saves the crash details and a diagnostic bundle to a folder and opens a new GitHub issue in your browser with the crash summary filled in. Nothing is sent until you submit the issue yourself.
problem_report_files_note = The folder opens when you continue - drag its files into the issue. About the diagnostic bundle in it:
problem_report_issue_note = The issue page opens with this text filled in:
problem_report_confirm = Save files and open GitHub
problem_report_saved = Report files saved. Drag them from the opened folder into the GitHub issue.
problem_report_prepare_failed = Could not prepare the report: { $error }
problem_report_save_failed = Could not save the report files: { $error }
settings_version_license = Open-source · MIT OR Apache-2.0
settings_version_check_updates = Check for updates
settings_version_checking = Checking for updates...
settings_version_up_to_date = forge is up to date - checked at { $time }
settings_version_update_found = Update found - v{ $version }
settings_version_update_downloading = Update found - v{ $version }, downloading
settings_version_check_failed = Could not check for updates
settings_version_open_releases = Open releases page

## Update notification

app_menu_quit = Quit forge
update_toast_available = forge { $version } is available
update_toast_open_action = Download
update_toast_ready = forge { $version } is ready. It installs when you quit.
update_toast_restart_action = Restart to update
update_toast_preparing = Preparing the update to forge { $version }...
update_toast_waiting_for_transfer = Waiting for the running backup or transfer to finish - forge { $version } installs when it does. Close again to quit without updating.
update_toast_restarting = Restarting to install forge { $version }
update_toast_download_failed = forge { $version } could not be downloaded.
update_toast_unverified = forge { $version } could not be verified, so it was not installed.
update_toast_handoff_failed = forge { $version } could not be prepared for install.
update_toast_retry_action = Retry

system_notice_action_failed_title = Action failed
system_notice_action_failed_body = { $action } did not finish.
system_notice_action_failed_body_reason = { $action } - { $reason }
system_notice_backup_failed_title = Scheduled backup failed
system_notice_backup_failed_body = forge could not write a scheduled backup: { $reason }.
system_notice_backup_failed_open = Open settings
system_notice_actions_folded_title = More actions failed
system_notice_actions_folded_body = { $count ->
    [one] { $count } more action failed. See the event log.
   *[other] { $count } more actions failed. See the event log.
}
update_leftover_install = Another forge is installed for all users. Uninstall it so only one copy uses your data.
update_leftover_open_action = Open installed apps
update_report_updated = forge updated to { $version }
update_report_rolled_back = forge { $failed } { $reason }. Back on { $restored }.
update_report_rolled_back_data_restored = forge { $failed } { $reason }. Back on { $restored }, data restored.
update_report_rolled_back_data_failed = forge { $failed } { $reason }. Back on { $restored }, data not restored.
update_report_rollback_failed = forge { $failed } { $reason } and could not be undone. Reinstall forge.
update_report_not_applied = forge { $version } not installed: { $reason }.
update_reason_crashed = crashed on start
update_reason_no_health_signal = did not start in time
update_reason_launch_failed = could not be started
update_reason_parent_still_running = forge did not close in time
update_reason_installer_failed = the installer reported an error
update_reason_installer_timed_out = the installer did not finish in time
update_reason_malformed_payload = the downloaded update was damaged
update_reason_file_operation = forge files could not be replaced
update_reason_interrupted = the update was interrupted

## Settings → Storage pane

settings_storage_section_title = Storage & backups
settings_storage_db_path_label = Database
settings_storage_backup_label = Backup
settings_storage_backup_btn = Backup now
settings_storage_backup_hint = Saves the whole data folder - database, credentials key, media, overlays, voices and assets - as one passphrase-encrypted file in the backups folder.
settings_storage_transfer_busy_hint = Another export, import or restore is in progress. Finish or cancel it first.
settings_storage_backup_btn_busy = Backing up...
settings_storage_backup_done = Backup saved to { $path }
settings_storage_backup_failed = Backup failed: { $error }
settings_storage_backup_reveal = Reveal
settings_storage_passphrase_title = Encrypt backup
settings_storage_passphrase_subtitle = A new passphrase is asked for every backup
settings_storage_passphrase_warning = forge never stores this passphrase. If you forget it, nobody can recover it and this backup becomes unusable.
settings_storage_passphrase_label = Passphrase
settings_storage_passphrase_placeholder = Enter a passphrase
settings_storage_passphrase_hint = At least { $min } characters
settings_storage_passphrase_too_short = Use at least { $min } characters
settings_storage_passphrase_confirm_label = Confirm passphrase
settings_storage_passphrase_confirm_placeholder = Enter it again
settings_storage_passphrase_mismatch = Passphrases do not match
settings_storage_passphrase_confirm_btn = Create backup
settings_storage_restore_btn = Restore...
settings_storage_restore_btn_busy = Preparing...
settings_storage_backup_scheduled_hint = Writes a scheduled backup now with the schedule's contents and passphrase key - no passphrase is asked.
backup_schedule_label = Backup schedule
backup_schedule_change_btn = Change passphrase
backup_schedule_run_done = Backup saved to the backup folder
backup_schedule_status_off = Off - last backup: { $last }
backup_schedule_status_paused = Paused while a restore is pending
backup_schedule_status_running = Backing up now...
backup_schedule_status_scheduled = Last backup: { $last } - next at { $next }
backup_schedule_status_last = Last backup: { $last }
backup_schedule_status_failing = { $attempts ->
    [one] Failing since { $since }: { $reason }
   *[other] Failing since { $since }, { $attempts } attempts: { $reason }
}
backup_schedule_failure_destination_missing = the backup folder is missing
backup_schedule_failure_destination_not_writable = the backup folder is not writable
backup_schedule_failure_disk_full = the disk is full
backup_schedule_failure_inside_scope = the backup folder is inside the data folder
backup_schedule_failure_key_missing = the schedule passphrase key is missing
backup_schedule_failure_key_unreadable = the stored schedule passphrase key is damaged, set the passphrase again
backup_schedule_failure_snapshot_failed = the backup could not be written
backup_schedule_failure_state_not_saved = the backup was written but its state could not be saved
backup_schedule_failure_interrupted = the backup was interrupted
backup_schedule_storage_used_label = Storage used
backup_schedule_storage_used_hint = Size on disk of each part of the data folder.
backup_schedule_storage_used_unknown = -
backup_schedule_storage_used_database = Database
backup_schedule_storage_used_media = Media
backup_schedule_storage_used_voices = Voices
backup_schedule_storage_used_overlays = Overlay files
backup_schedule_storage_used_assets = Action files
backup_schedule_storage_used_logs = Logs
backup_schedule_storage_used_backups = Backups
backup_schedule_storage_used_destination = Backup folder
backup_schedule_interval_label = Interval
backup_schedule_interval_hint = Minutes between scheduled backups, { $min }-{ $max }.
backup_schedule_keep_newest_label = Backup retention
backup_schedule_keep_newest_hint = How many of the newest scheduled backups to keep, { $min }-{ $max }.
backup_schedule_keep_daily_label = Daily backups
backup_schedule_keep_daily_hint = Also keep one scheduled backup per day for this many days, { $min }-{ $max }; 0 turns this off.
backup_schedule_range_error = Enter a whole number from { $min } to { $max }
backup_schedule_unit_minutes = { $count ->
    [one] minute
   *[other] minutes
}
backup_schedule_unit_backups = { $count ->
    [one] backup
   *[other] backups
}
backup_schedule_unit_days = { $count ->
    [one] day
   *[other] days
}
backup_schedule_contents_label = Contents
backup_schedule_contents_hint = Unchecked parts are left out of scheduled backups; restoring one keeps them as they are on this machine.
backup_schedule_category_database = Database and credentials key
backup_schedule_category_database_reason = always included
backup_schedule_category_media = Media
backup_schedule_category_voices = Voices
backup_schedule_category_overlays = Overlay files
backup_schedule_category_assets = Action files
backup_schedule_folder_label = Backup folder
backup_schedule_folder_hint = Scheduled backups are written here. Only the default folder is created when missing.
backup_schedule_folder_choose = Choose...
backup_schedule_folder_reset = Use default
backup_schedule_folder_failed = Could not open the folder picker: { $error }
backup_schedule_persist_failed = Could not save the backup schedule setting
backup_schedule_key_failed = Could not set the schedule passphrase: { $error }
backup_schedule_setup_title = Protect scheduled backups
backup_schedule_setup_subtitle = Asked once - scheduled backups then run on their own
backup_schedule_setup_warning = forge forgets this passphrase and keeps only a locked restore key. Restoring a scheduled backup asks for it; if you forget it, nobody can recover those backups.
backup_schedule_setup_confirm_btn = Turn on schedule
backup_schedule_change_title = Change schedule passphrase
backup_schedule_change_subtitle = New scheduled backups use the new passphrase
backup_schedule_change_warning = Backups made before this change still open only with the old passphrase. Keep it while you still need those backups.
backup_schedule_change_confirm_btn = Change passphrase
backup_schedule_change_current_label = Current passphrase
backup_schedule_change_current_placeholder = Enter the current passphrase
backup_schedule_change_new_label = New passphrase
backup_schedule_change_wrong_current = The current passphrase is not correct
backup_schedule_change_done = Schedule passphrase changed
safety_copies_label = Safety copies
safety_copies_hint = Copies forge makes on its own before a Restore, a pack or setup import and an update, so you can go back.
safety_copies_mode_keep_newest = Keep the newest
safety_copies_mode_delete_after_days = Delete after
safety_copies_mode_never = Never delete
safety_copies_unit_copies = { $count ->
    [one] copy
   *[other] copies
}
safety_copies_unit_days = { $count ->
    [one] day
   *[other] days
}
safety_copies_usage_label = Stored safety copies
safety_copies_usage = { $count ->
    [one] { $count } copy - { $size }
   *[other] { $count } copies - { $size }
}
safety_copies_usage_unknown = -
safety_copies_clean_up_btn = Clean up now
safety_copies_clean_up_btn_busy = Cleaning up...
safety_copies_clean_up_confirm_title = Delete all safety copies?
safety_copies_clean_up_confirm_body = Every safety copy in the data folder is deleted, except ones still needed by an update in progress or an unfinished restore. Your backups are not touched. You will not be able to go back to the state before an earlier Restore, import or update.
safety_copies_clean_up_confirm_action = Delete safety copies
safety_copies_saved = Safety copy setting saved
safety_copies_nothing_to_remove = There are no safety copies to delete - copies still needed by an update in progress or an unfinished restore are kept
safety_copies_freed = { $count ->
    [one] Deleted { $count } safety copy - freed { $size }
   *[other] Deleted { $count } safety copies - freed { $size }
}
safety_copies_not_deleted = { $count ->
    [one] Could not delete a safety copy ({ $name }): { $reason }
   *[other] Could not delete { $count } safety copies, first { $name }: { $reason }
}
safety_copies_change_deferred = Setting saved - a backup or transfer is running, so safety copies will be trimmed the next time forge starts.
safety_copies_clean_up_deferred = A backup or restore is running - try again when it finishes.
safety_copies_change_failed = Could not apply the safety copy setting: { $error }
safety_copies_clean_up_failed = Could not clean up safety copies: { $error }
restore_dialog_filter = forge backups
restore_passphrase_title = Unlock backup
restore_passphrase_subtitle = Enter the passphrase this backup was created with
restore_passphrase_label = Passphrase
restore_passphrase_placeholder = Backup passphrase
restore_passphrase_hint = Nothing changes until the backup is unlocked and you confirm the restore.
restore_passphrase_submit = Unlock
restore_passphrase_submit_busy = Unlocking...
restore_confirm_title = Restore this backup?
restore_confirm_subtitle = All current data will be replaced by the backup
restore_confirm_created = Created
restore_confirm_forge_version = forge version
restore_confirm_schema = Database schema
restore_confirm_snapshot_kept = Your current data is kept as a pre-restore snapshot in the backups folder.
restore_confirm_closes = forge will close now and start again by itself to finish the restore.
restore_confirm_relogin = Accounts may ask you to log in again if their logins were refreshed after this backup was made.
restore_confirm_one_machine = Never run the same restored logins on two machines at once - it breaks the login chain on both.
restore_confirm_btn = Restore and restart
restore_confirm_btn_busy = Scheduling...
restore_confirm_subtitle_scoped = The parts this backup holds will be replaced
restore_confirm_scope_replaced = Replaced from the backup: { $parts }
restore_confirm_scope_kept = Not in this backup, left as they are: { $parts }
restore_close_cancelled = Restore cancelled - your current data was not changed.
restore_close_yielded_to_update = An update is being installed, so the restore was cancelled. Restart to finish the update, then restore.
restore_close_cancel_failed = Restore cancelled, but its files could not be cleaned up: { $error }
restore_problem_not_a_bundle = This file is not a forge backup.
restore_problem_unknown_kind = This file holds a kind of forge data this version does not know.
restore_problem_newer_format = This backup was made in bundle format { $found }; this forge reads up to format { $supported }. Update forge first.
restore_problem_newer_schema = This backup holds database schema { $found }; this forge supports up to schema { $supported }. Update forge first.
restore_problem_schema_too_old = This backup's database (legacy schema { $found }) is too old to restore; open it once with forge { $release } first.
restore_problem_unrecognized_database = This backup's database is not one forge can open.
restore_problem_unknown_encryption = This backup is encrypted in a way this forge does not know. Update forge first.
restore_problem_passphrase_required = This backup is encrypted. Enter its passphrase to restore it.
restore_problem_passphrase_too_short = The passphrase must be at least { $min } characters.
restore_problem_wrong_passphrase = Wrong passphrase
restore_problem_damaged = The file is damaged or incomplete and cannot be restored.
restore_problem_missing_key = This backup holds logins but not the key that decrypts them, so it cannot be restored.
restore_problem_update_in_progress = An update is ready or being installed. Restart forge to finish the update, then restore.
restore_problem_already_pending = A restore is already waiting for the next start.
restore_problem_unavailable = Restore cannot be scheduled right now. Try again in a moment.
restore_problem_failed = Something went wrong: { $error }
restore_problem_kind_mismatch = This file is a forge { $found }, not a { $expected }.
restore_problem_newer_documents = This file uses document format { $found }; this forge reads up to format { $supported }. Update forge first.
bundle_kind_full = full backup
bundle_kind_pack = pack
bundle_kind_setup = setup
import_busy_pack_title = Reading pack
import_busy_pack_detail = Checking the pack against your forge data.
import_busy_setup_title = Reading setup
import_busy_setup_detail = Unpacking the setup to show what it replaces.
import_busy_restore_title = Reading backup
import_busy_restore_detail = Checking the backup before it can be restored.
import_busy_already = Another import is still running. Wait for it to finish, then try again.
transfer_busy_already = Another backup, export, import or restore is still running. Wait for it to finish, then try again.
restore_boot_applied = Backup restored. Your previous data was kept in { $path }
restore_boot_rolled_back = The restore was not applied and your data was left as it was: { $reason }
restore_boot_deferred = A restore is still pending and was not applied: { $reason }
restore_rollback_interrupted = it was interrupted
restore_rollback_staging_missing = the prepared backup files were missing
restore_rollback_marker_unreadable = its bookkeeping file was unreadable
restore_deferral_update = an update is being installed first. It will be applied on a later start.
restore_deferral_instance_lock = another forge may be running. Close every forge window and start it again.
credentials_key_loss_toast =
    { $count ->
        [one] The credentials key was missing - { $count } saved login can't be read. Sign in again on the Platforms screens.
       *[other] The credentials key was missing - { $count } saved logins can't be read. Sign in again on the Platforms screens.
    }
settings_storage_per_viewer_limit_label = Chat history per viewer
settings_storage_per_viewer_limit_hint = How many of each viewer's latest chat messages to keep in the database.
settings_storage_per_viewer_limit_custom = Custom
settings_storage_per_viewer_limit_unlimited = Unlimited
settings_storage_per_viewer_limit_unlimited_hint = The database grows without limit.
settings_storage_display_limit_label = Chat history shown on open
settings_storage_display_limit_hint = How many recent messages the chat shows; older ones drop off. Takes effect after a restart.
settings_storage_retention_label = Event log retention
settings_storage_retention_hint = How many days of event log history to keep in the database.
settings_storage_history_retention_label = Action history retention
settings_storage_history_retention_hint = How many days of action run history to keep in the database - at least { $min }.

## Settings → Queues pane


## Settings → Shortcuts pane

settings_shortcuts_subtitle = These shortcuts work only while the forge window is focused. System-wide combinations live under Hotkeys.
settings_shortcuts_action_nav_home = Go to Hub
settings_shortcuts_action_nav_live_chat = Open Live Chat
settings_shortcuts_action_nav_actions = Open Actions
settings_shortcuts_action_nav_triggers = Open Triggers
settings_shortcuts_action_nav_twitch = Open Twitch
settings_shortcuts_action_nav_settings = Open Settings
settings_shortcuts_action_save = Save
settings_shortcuts_action_new_action = New action
settings_shortcuts_action_toggle_live_chat = Toggle Live Chat
settings_shortcuts_action_toggle_event_feed = Toggle Event Feed
settings_shortcuts_action_run_script = Run script
settings_shortcuts_unbound = Not bound
settings_shortcuts_capture_prompt = Press a shortcut... Esc to cancel
settings_shortcuts_rebind = Change
settings_shortcuts_reset = Reset
settings_shortcuts_reset_all = Reset all to defaults
settings_shortcuts_fixed_section = Fixed keys
settings_shortcuts_fixed_enter = Confirm a form or dialog
settings_shortcuts_fixed_escape = Close a modal or cancel capture
settings_shortcuts_fixed_note = These keys are built in and cannot be rebound.
settings_hold_ceiling_section = Global hotkeys
settings_hold_ceiling_label = Close a stuck hold after
settings_hold_ceiling_placeholder = seconds
settings_hold_ceiling_unit = seconds
settings_hold_ceiling_off = Never closed automatically
settings_hold_ceiling_hint = When the system swallows a key release, the held hotkey releases itself after this long. Off leaves a hold open until a release arrives.
settings_hold_ceiling_invalid = Enter a whole number of seconds between { $min } and { $max }.
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
settings_ws_badge_recommended = Recommended
settings_ws_badge_requires_confirmation = Requires confirmation
settings_ws_origins_section_title = Additional allowed origins
settings_ws_origins_subtitle = One per line. A page Forge serves itself is accepted automatically, including a browser source pointed at your machine's IP. Needed only for a page served from somewhere else - another host, or a domain name that reaches Forge through a proxy. Without a match the WebSocket handshake is rejected.
settings_ws_origins_placeholder = http://192.168.1.5:8081
settings_ws_origins_apply_btn = Apply
settings_ws_origins_invalid = Not a valid origin (scheme + host, optional port):
settings_ws_port_section_title = Port
settings_ws_port_subtitle = Default 8081 · range 1024-65535
settings_ws_token_section_title = Bearer token
settings_ws_token_clients_send = Clients send this in
settings_ws_auth_section_title = Authentication
settings_ws_auth_section_subtitle = Which clients need to authenticate
settings_ws_auth_require_ws_label = Require token for WebSocket clients
settings_ws_auth_require_ws_sublabel = Reject WS handshake without valid bearer token
settings_ws_auth_require_http_label = Require token for HTTP overlay files
settings_ws_auth_require_http_sublabel = Overlay URLs carry the overlay token as their first path segment: /overlays/<token>/...
settings_ws_overlay_token_title = Overlay token
settings_ws_overlay_token_subtitle = Read-only - lets a browser source load overlay files and nothing else
settings_ws_overlay_token_regen_title = Regenerate the overlay token?
settings_ws_overlay_token_regen_message = Every overlay URL changes and the old ones stop loading right away. Re-copy each overlay URL into its OBS browser source.
settings_ws_overlay_token_restore_failed = Regenerate failed and the previous overlay token could not be restored. The new token is now active - re-copy every overlay URL into its OBS browser source.
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

## Actions - page header / breadcrumb

actions_breadcrumb_automation = Automation
actions_breadcrumb_actions = Actions
actions_filter_all = All
actions_group_ungrouped = Ungrouped
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

actions_menu_rename = Rename...
actions_menu_duplicate = Duplicate
actions_menu_enable = Enable
actions_menu_disable = Disable
actions_menu_delete = Delete...

## Actions - footer

## Actions - ESC hint


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
actions_modal_concurrent_label = Bypass queue limit
actions_modal_concurrent_desc = Runs right away, as if its queue allowed parallel runs. Its own steps still run in order.
actions_modal_bypass_label = Bypass queue pause
actions_modal_bypass_desc = Always run even if queue is paused.
actions_modal_random_pick_label = Random pick
actions_modal_random_pick_desc = Run ONE random step per trigger instead of all.
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
sub_cat_overlay = Overlays
sub_cat_util = Utilities

## Actions - sub-action config sections

## Actions - trigger picker (side-sheet)

## Actions - trigger category display names (section headers)

## Actions - trigger kind labels

## Actions - trigger kind summaries

## Action editor - breadcrumb / tree pane / detail pane

action_editor_loading = Loading action...
action_editor_no_description = No description
action_editor_test_run = Test run
action_editor_duplicate = Duplicate
action_editor_export = Export as pack...
action_editor_edit = Edit
action_editor_menu_delete = Delete action
action_editor_add_trigger = Add trigger
action_editor_add_step = Add step
action_editor_health_unknown_var = Uses %{ $name }%, which no trigger provides and no earlier step produces
action_editor_health_produced_later = Uses %{ $name }%, but it is only produced by a later step
action_editor_health_isolated_sibling = Uses %{ $name }%, produced by a sibling step that runs in isolation and does not share it
action_editor_health_some_triggers = Uses %{ $name }%, which only some of this action's triggers provide
action_editor_health_last_run_failed = Last run failed: { $message }
action_editor_health_disabled = Integration disabled
action_editor_health_integration_disabled = { $name } is disabled
action_editor_step_disabled_prefix = { $name } is disabled - this step fails with{" "}
action_editor_health_failed = Integration failed
action_editor_health_integration_failed = { $name } failed to start
action_editor_health_date_format_not_datetime = The step formats %{ $name }% as a date, but this variable is not a date and time, so the pattern is ignored and the value shows as is
action_editor_health_invalid_date_pattern = The date pattern in %{ $name }:{ $pattern }% is invalid: { $reason }
action_editor_health_invalid_step_config = Step settings are invalid: { $message }
step_config_required = Required
step_config_not_a_number = Must be a number
step_config_between = Must be between { $min } and { $max }
step_config_at_least = Must be at least { $min }
step_config_at_most = Must be at most { $max }
step_config_length_between = Must be { $min } to { $max } characters long
step_config_length_at_least = Must be at least { $min } { $min ->
    [one] character
   *[other] characters
} long
step_config_length_at_most = Must be at most { $max } { $max ->
    [one] character
   *[other] characters
} long
step_config_out_of_range = Out of the allowed range
step_config_count_between = Must list { $min } to { $max } items
step_config_count_at_least = Must list at least { $min } { $min ->
    [one] item
   *[other] items
}
step_config_count_at_most = Must list at most { $max } { $max ->
    [one] item
   *[other] items
}
step_config_not_text = Must be text
step_config_any_of_required = Fill in at least one of: { $fields }
step_config_invalid_choice = Must be one of: { $choices }
step_config_invalid_format = Invalid format: { $detail }
step_config_field_problem = { $problem } ({ $field })
action_editor_step_failed_notice = { $name } failed to start - this step fails until it runs
action_editor_health_ok = Healthy: all references resolve, last run passed
action_editor_health_warn = Static warning
action_editor_health_error = Error
action_editor_overlay_order_warning = Overlay rows may arrive out of order
action_editor_overlay_order_warning_hint = This action feeds an order-sensitive overlay while its runs can overlap: its queue runs several actions at once or the action bypasses its queue limit. A queue with concurrency 1 and an action that keeps its queue limit preserve the order.
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
action_editor_sub_actions_count = { $count ->
    [one] { $count } step
   *[other] { $count } steps
}
actions_sub_file_browse = Browse
actions_sub_files_root_label = Files folder
actions_sub_files_root_hint = File steps only use paths relative to this folder - absolute paths and ".." are refused.
actions_sub_files_root_open = Open folder
actions_sub_files_root_create_failed = Could not create the files folder: { $reason }
actions_sub_datetime_pick = Pick
actions_sub_datetime_now = Now
actions_sub_datetime_set = Set
actions_var_picker_open = Variable
actions_var_picker_search = Search variables
actions_var_picker_empty = No variables are available at this step
actions_var_picker_no_format = No format
actions_var_picker_group_trigger = Trigger
actions_var_picker_group_some_triggers = Only some triggers
actions_var_picker_group_steps = Earlier steps
actions_var_picker_group_loop = Loop
action_editor_sub_count = { $count ->
    [one] { $count } step
   *[other] { $count } steps
}
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

action_editor_run_history = Run history...
action_editor_run_history_title = Run history
action_editor_run_history_loading = Loading run history...
action_editor_run_history_failed = Could not load run history: { $error }
action_editor_run_history_empty_title = No runs yet
action_editor_run_history_empty_hint = This action has not run yet
action_editor_run_history_duration_ms = { $count } ms
action_editor_run_history_outcome_success = Success
action_editor_run_history_outcome_failed = Failed
action_editor_run_history_outcome_cancelled = Cancelled
action_editor_run_history_step_ok = ok
action_editor_run_history_step_failed = failed
action_editor_run_history_integration_disabled = integration disabled · { $name }
action_editor_run_history_open_integration = Open { $name }
action_editor_run_history_step_skipped = skipped
action_editor_run_history_step_nested = ↳
action_editor_run_history_trigger_fallback = Trigger
action_editor_run_history_scheduled_by = Scheduled by { $name }
action_editor_run_history_scheduled_by_removed = Scheduled by a deleted action
action_editor_run_history_scheduled = Scheduled run
action_editor_run_history_step_args_in = @in
action_editor_run_history_step_produced = @out

## Action editor - step menu

action_editor_step_menu_edit = Edit step...
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
action_editor_test_run_empty = No steps to run.
action_editor_test_run_default_error = Step execution failed
action_editor_test_run_status_queued = queued
action_editor_test_run_status_running = running...
action_editor_test_run_status_failed = failed
action_editor_test_run_status_skipped = skipped
action_editor_step_wait_ms = { $ms } ms
action_editor_test_run_status_ms = { $ms } ms
action_editor_test_run_failed_banner = Run failed at step { $step } · { $name }
action_editor_test_run_completed = Completed { $count } steps · { $errors } errors
action_editor_test_run_notstarted = Action did not start · the queue may be paused
action_editor_test_run_foot_simulating = Simulating...
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
action_editor_picker_add_sub_title = Add step
action_editor_picker_inserting_into = Inserting into
action_editor_picker_sub_count = · { $count ->
    [one] { $count } step
   *[other] { $count } steps
}
action_editor_picker_footer_hint = Added with smart defaults - edit inline after
action_editor_picker_search = Search { $count } steps...
action_editor_picker_fires = Fires
action_editor_picker_available_count = · { $count } available
action_editor_trigger_picker_footer_hint = Creates a new trigger of the chosen kind and links it

## Action editor - sub-action card titles

action_editor_kind_send_chat = Send chat message
action_editor_kind_set_global = Set global
action_editor_kind_delay = Delay
action_editor_kind_log = Log
action_editor_kind_play_sound = Play sound
action_editor_clip_missing = Missing clip
action_editor_kind_speak = Speak
action_editor_kind_read_file = Read file
action_editor_kind_random_int = Random int
action_editor_kind_incr_global = Increment global
action_editor_kind_run_script = Run script
action_editor_persisted_note = (persisted)
action_editor_kind_sub_action = Step

## Triggers registry - page header / filters

triggers_breadcrumb_automation = Automation
triggers_breadcrumb_triggers = Triggers
triggers_search_placeholder = Search triggers...
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

triggers_menu_rename = Rename...
triggers_menu_template = Use as template
triggers_menu_delete = Delete...
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
triggers_sheet_section_permission = PERMISSION
triggers_sheet_permission_value = minimum rung
triggers_sheet_permission_helper = Higher rungs always pass. Where a platform cannot express a rung it collapses upward - stricter, never looser.
triggers_permission_meaning_everyone = Anyone in chat passes, badge or not.
triggers_permission_meaning_twitch_subscriber = Twitch: a subscriber or founder badge
triggers_permission_meaning_twitch_vip = Twitch: a VIP badge
triggers_permission_meaning_twitch_moderator = Twitch: a moderator badge
triggers_permission_meaning_twitch_broadcaster = Twitch: the channel owner only
triggers_permission_meaning_youtube_subscriber = YouTube: channel members (plain subscribers do not count)
triggers_permission_meaning_youtube_vip = YouTube: no VIP role - only moderators and the channel owner pass
triggers_permission_meaning_youtube_moderator = YouTube: channel moderators
triggers_permission_meaning_youtube_broadcaster = YouTube: the channel owner only
triggers_permission_meaning_kick_subscriber = Kick: a subscriber or founder badge
triggers_permission_meaning_kick_vip = Kick: a VIP badge
triggers_permission_meaning_kick_moderator = Kick: a moderator badge
triggers_permission_meaning_kick_broadcaster = Kick: the channel owner only
triggers_permission_rung_everyone = everyone
triggers_permission_rung_subscriber = subs+
triggers_permission_rung_vip = vip+
triggers_permission_rung_moderator = mods+
triggers_permission_rung_broadcaster = broadcaster
triggers_permission_suffix = { " · perm=" }{ $rung }
triggers_timer_every = every { $minutes } min
triggers_timer_live_only = only while live
triggers_timer_min_messages = ≥{ $count } { $count ->
    [one] message
   *[other] messages
}
triggers_timer_hint_interval = Interval: how often it fires, from 1 minute to 24 hours. The first fire comes one interval after saving or after the stream goes live.
triggers_timer_hint_live = Only while live: skip fires while no platform and no OBS stream output reports live.
triggers_timer_hint_messages = Min chat messages: skip a fire until at least this many viewer messages arrived since the last one. 0 = off. Bot and forge's own messages do not count.
triggers_sheet_section_used_in = USED IN
triggers_sheet_delete_btn = Delete
triggers_sheet_save_btn = Save
triggers_detail_loading = Loading trigger...
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
triggers_create_search_types = Search { $count } trigger types...
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

## Settings → Scripting pane

settings_scripting_title = Scripting & HTTP
settings_scripting_engine_section = Engine limits
settings_scripting_op_limit_label = Op-count limit
settings_scripting_op_limit_hint = Range 1 000 - 10 000 000 (default 100 000)
settings_scripting_engine_timeout_label = Timeout (ms)
settings_scripting_engine_timeout_hint = Range 50 - 10 000 (default 500)
settings_scripting_http_section = HTTP sandbox
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
settings_scripting_core_allow_local_label = Allow localhost / private IPs
settings_scripting_core_allow_local_description = Disables SSRF protections for the HTTP request action. Only enable for local development.
settings_scripting_ssrf_warning = WARNING - disables SSRF protections. Only enable for local development.
settings_scripting_http_action_section = HTTP action
settings_scripting_range_error = Allowed: { $min } - { $max }
settings_scripting_persist_failed = Could not save the Scripting & HTTP setting. It will revert on restart.

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
trigger_cat_donations = Donations
trigger_cat_goals = Goals
trigger_cat_clips = Clips
trigger_cat_streams = Streams
trigger_cat_users = Users
trigger_cat_core = Core
trigger_cat_other = Other

## Actions modals - placeholder literals

actions_name_placeholder = My automation
actions_group_placeholder = Examples
actions_description_placeholder = Plays a sound, shows overlay alert...

## Actions - list states, toasts, delete confirm

actions_tree_loading = Loading actions...
actions_loading_queues = Loading queues...
actions_no_queue = No queue available
actions_toast_error = Actions: { $message }
actions_error_missing_action = action not found
actions_error_missing_chain = step list not found
actions_duplicate_name_suffix = (copy)
actions_rename_taken = Name '{ $name }' is already taken
actions_deleted_toast = Deleted '{ $name }'
actions_delete_title = Delete action?
actions_delete_body = This will remove the action and all of its steps and triggers.
actions_delete_scheduled_runs = { $count ->
    [one] Its { $count } pending scheduled run will be cancelled too.
   *[other] Its { $count } pending scheduled runs will be cancelled too.
}
actions_delete_scheduled_runs_unknown = Any pending scheduled runs of this action will be cancelled too.

## Triggers registry - error messages

triggers_delete_reference_block = Remove this trigger from all actions before deleting.

## TTS - tab bar section labels

tts_tab_dashboard = Dashboard
tts_tab_engines = Engines
tts_tab_aliases = Voice aliases
tts_tab_filters = Filters

## TTS - breadcrumb

tts_breadcrumb_tts = Text-to-Speech

## TTS Dashboard - control strip

tts_dash_pause_btn = Pause queue
tts_dash_resume_btn = Resume
tts_dash_skip_btn = Skip
tts_dash_stop_all_btn = Stop all
tts_dash_voice_gate_held = Held by voice gate
tts_dash_stop_all_confirm_name = Stop all TTS
tts_dash_stop_all_confirm_hint = Currently speaking message will be cut off and all queued messages dropped. Engines remain ready to handle new messages.
tts_dash_test_placeholder = Type to test a voice...
tts_dash_speak_btn = Speak
tts_dash_test_speaker_name = Test

## TTS Dashboard - now speaking

tts_dash_now_speaking_header = NOW SPEAKING
tts_dash_no_speaking = -
tts_dash_last_drop = Last request dropped: { $reason }

## TTS Dashboard - queue

tts_dash_queue_header = UP NEXT
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
tts_filters_blocklist_censor_meta = replace with [beep]
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
tts_filters_output_strip_twitch_emotes = Strip Twitch emotes
tts_filters_output_strip_twitch_emotes_meta = remove Twitch emote names before speaking
tts_filters_output_strip_reward_emotes = Strip reward emotes
tts_filters_output_strip_reward_emotes_meta = remove emote names from channel point redemptions
tts_filters_output_max_duration_meta = after { $secs }s
tts_filters_output_lang_meta = pick voice per message language

## TTS Filters - rule list

tts_filters_badge_text = TEXT
tts_filters_badge_regex = REGEX
tts_filters_badge_text_inside_words = TEXT · INSIDE WORDS
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
tts_filters_preset_output_maxdur_label = MAX SECONDS
tts_filters_preset_output_maxdur_placeholder = 30
tts_filters_preset_output_maxdur_range = Allowed: { $min }-{ $max } seconds
tts_filters_preset_output_sanitize_hint = "!!!" → "!"

tts_filters_modal_blocklist_words_label = WORDS OR PHRASES
tts_filters_modal_blocklist_words_placeholder = one per line, or comma-separated...
tts_filters_modal_blocklist_note = Matching is case-insensitive and matches whole words only; punctuation around a word is ignored; multi-word phrases will not match as a unit.
tts_filters_modal_blocklist_when_matched_label = WHEN MATCHED
tts_filters_modal_blocklist_censor_row = Censor the word
tts_filters_modal_blocklist_censor_row_hint = replace with [beep]
tts_filters_modal_blocklist_skip_row = Skip the whole message
tts_filters_modal_blocklist_skip_row_hint = nothing is spoken

tts_filters_modal_replace_text_tab = TEXT
tts_filters_modal_replace_regex_tab = REGEX
tts_filters_modal_replace_find_label = FIND
tts_filters_modal_replace_match_label = MATCH PATTERN
tts_filters_modal_replace_find_placeholder = POG
tts_filters_modal_replace_replace_label = REPLACE WITH
tts_filters_modal_replace_replace_text_placeholder = respect
tts_filters_modal_replace_inside_words_label = Match inside words
tts_filters_modal_replace_inside_words_hint = Off: GG replaces GG and GG! but not POGGERS. Matching ignores case.
tts_filters_modal_replace_note = Leave replacement empty to strip matched text.

## TTS Filters - pipeline settings

## TTS Filters - preview column

tts_filters_preview_header = Live preview
tts_filters_preview_input_label = INPUT MESSAGE
tts_filters_preview_input_placeholder = Type a message to preview...
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
tts_cloud_testing_btn = Testing...
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

tts_aliases_search_placeholder = Search viewers...
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
tts_aliases_loading = Loading voice aliases...

## Voice Aliases - blocked row

tts_aliases_never_speak = Never speak

## TTS Voice aliases - role badges

tts_aliases_role_blocked = BLOCKED

## Voice Aliases - assign/edit modal

tts_aliases_form_title_assign = Assign a voice
tts_aliases_form_title_edit = Edit voice alias
tts_aliases_form_platform_label = PLATFORM
tts_aliases_platform_any = Any
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
tts_aliases_viewer_taken = This viewer already has a voice alias - edit that one instead.
common_delete = Delete
common_undo = Undo

## Voice Aliases - preview

tts_aliases_preview_text = This is a voice preview.

## Voice Aliases - footer caption

tts_aliases_footer_caption = Showing { $shown } of { $total } manual aliases · { $auto } viewers use auto-assignment

## Soundboard - breadcrumb

soundboard_breadcrumb_soundboard = Soundboard

## Soundboard - header / modal

soundboard_loading = Loading clips...
soundboard_empty_title = No clips yet
soundboard_playback_error_prefix = Playback error: { $error }
soundboard_error_source_missing = The source file for { $name } is gone.
soundboard_error_clip_gone = That sound is no longer in the soundboard.
soundboard_import_unsupported = { $file } is not a sound format forge can play.
soundboard_import_type_mismatch = { $file } has the extension { $named } but its content is { $detected }.
soundboard_import_too_large = { $file } is { $size }, over the { $limit } limit for sounds.

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

soundboard_search_placeholder = Search sounds...
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
soundboard_pad_playing = playing...
soundboard_pad_source_missing = file missing
soundboard_pad_source_missing_hint = The source file is gone. Edit this sound and pick the file again.
soundboard_pad_adopt_blocked = cannot be copied
soundboard_pad_not_in_library = not in library yet
soundboard_toast_clip_failed = { $clip }: { $error }
soundboard_toast_clip_failed_unnamed = Sound failed: { $error }
soundboard_persist_failed = Could not save the soundboard setting. It will revert on restart.
soundboard_no_matches = No sounds match your filter
soundboard_library_section = Library
soundboard_library_import = Import
soundboard_add_sound = Add sound
soundboard_modal_section_category = CATEGORY
soundboard_modal_section_playback = PLAYBACK
soundboard_modal_loop_label = Loop playback
soundboard_modal_loop_hint = Repeat the clip until you stop it.
soundboard_modal_ready = Ready to add
soundboard_modal_ready_edit = Ready to save
soundboard_modal_fill_required = Fill required fields
soundboard_modal_section_key = GLOBAL KEY
soundboard_modal_key_none = None
soundboard_modal_key_set = Set key
soundboard_modal_key_change = Change
soundboard_modal_key_clear = Clear
soundboard_modal_key_hint = Toggles the clip from anywhere, even while forge is in the background. Esc cancels.
soundboard_modal_key_disabled = { $name } is disabled - this key will not fire.
soundboard_hotkeys_disabled = { $name } is disabled - clip hotkeys will not fire. Pads and actions still play clips.
soundboard_toast_key_not_registered = { $name } is saved, but { $combo } could not be registered as a global key. It stays saved and is retried on the next change.
soundboard_toast_key_still_shared = { $name } is saved, but { $combo } could not be taken from { $holder }. Both stay bound to it and one press fires both.
soundboard_pad_key_not_live = { $combo } is saved but not active: the system refused it or another program holds it. It is retried on the next change.
soundboard_pad_key_engine_unavailable = Global hotkeys are unavailable on this system, so { $combo } does not fire.
soundboard_pad_key_shared = { $combo } is also bound to { $holder }. One press fires both.
soundboard_routing_section = Output routing
soundboard_routing_device = DEVICE
soundboard_routing_hint = Add this device as an Audio Input Capture in OBS.
soundboard_routing_volume = MASTER VOLUME · { $pct }%
soundboard_routing_headphones = Also play in headphones
soundboard_footer_left = { $sounds } sounds · { $categories } categories · { $size }
soundboard_footer_unadopted =
    { $count ->
        [one] { $count } sound still plays from its original folder
       *[other] { $count } sounds still play from their original folder
    }
soundboard_footer_adopt_action =
    { $count ->
        [one] Copy { $count } clip into the library
       *[other] Copy { $count } clips into the library
    }
soundboard_footer_adopt_busy = Copying...
soundboard_adopt_copied = { $count } copied
soundboard_adopt_refused = { $count } refused
soundboard_adopt_missing = { $count } missing
soundboard_adopt_unfinished = { $count } unfinished
soundboard_adopt_nothing = Nothing changed
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
queues_loading = Loading queues...
queues_search_placeholder = Search queues...
queues_no_filter_match = No queues match the active filter.
queues_filter_all = All
queues_filter_running = Running
queues_filter_paused = Paused
queues_filter_parallel = Parallel
queues_filter_sequential = Sequential
queues_configure_btn = Configure
queues_drain_btn = Drain
queues_pause_btn = Pause
queues_hold_btn = Hold
queues_free_btn = Free

## Queues - mode controls

queues_mode_pause_tooltip = Stop processing and skip every new action
queues_mode_drain_tooltip = Keep processing the backlog, skip every new action
queues_mode_hold_tooltip = Stop processing but keep queueing new actions
queues_mode_active_tooltip = Back to running
queues_free_tooltip = Drop everything waiting - the action in flight still finishes and the mode stays as it is
queues_free_feedback = Dropped { $count } waiting actions in “{ $name }”.
queues_free_failed = Could not free the queue: { $error }
queues_mode_change_failed = Could not change the queue mode: { $error }
queues_pause_all_failed = Could not pause every queue: { $error }
queues_delete_failed = Could not delete the queue: { $error }
queues_load_failed = Could not load the queues: { $error }
queues_mode_running_caption = Running - processing actions as they arrive
queues_mode_drain_caption = Draining - finishing the backlog, new actions are skipped
queues_mode_hold_caption = Held - processing stopped, new actions still queue up
queues_mode_pause_caption = Paused - processing stopped, new actions are skipped
queues_strip_counts = { $pending } waiting · { $in_flight } in flight
queues_overflow_badge = { $count } skipped over the { $cap } cap

## Queues - card menu

queues_menu_configure = Configure...
queues_menu_pause = Pause
queues_menu_resume = Resume
queues_menu_free = Free queue
queues_menu_delete = Delete...
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

## Queues - running panel

queues_running_now_header = RUNNING NOW
queues_no_actions_running = No actions running
queues_running_count = { $count } actions running
queues_running_label = running

## Queues - status badge

queues_status_paused = PAUSED
queues_status_running = RUNNING
queues_status_draining = DRAINING
queues_status_held = HELD

## Queues - live-membership divergence

queues_not_live_badge = NOT LIVE · APPLY
queues_not_live_tooltip = The scheduler is not running this queue's latest settings. Click to apply them now.
queues_apply_live_done = Queue settings applied live.
queues_apply_live_failed = Could not apply the queue live ({ $error }). Restart forge to pick up the change.
queues_apply_live_missing = the queue no longer exists
queues_apply_live_not_found = the scheduler does not know this queue

## Queues - overflow pill

queues_overflow_more = +{ $count } more

## Queues - scheduled runs

queues_scheduled_title = Scheduled
queues_scheduled_blurb = Action runs planned for later by a Schedule Action step or a script
queues_scheduled_count = { $count } pending
queues_scheduled_loading = Loading scheduled runs...
queues_scheduled_empty = Nothing scheduled - add a Schedule Action step to an action to plan a run
queues_scheduled_load_failed = Could not load scheduled runs: { $error }
queues_scheduled_due_now = due now
queues_scheduled_due_in = in { $span }
queues_scheduled_waiting_for_queue = waiting for its queue
queues_scheduled_from = from
queues_scheduled_from_removed = from a deleted action
queues_scheduled_from_unknown = scheduled outside an action run
queues_scheduled_run_now = Run now
queues_scheduled_cancel = Cancel
queues_scheduled_run_now_started = Started “{ $name }” now.
queues_scheduled_run_now_refused = “{ $name }” did not start: { $reason }
queues_scheduled_run_now_failed = Could not run it now: { $error }
queues_scheduled_cancel_failed = Could not cancel the scheduled run: { $error }
queues_scheduled_not_pending = This run already started or was cancelled.
queues_scheduled_recent = RECENT ({ $count })
queues_scheduled_outcome_ran = STARTED
queues_scheduled_outcome_cancelled = CANCELLED
queues_scheduled_outcome_skipped = SKIPPED
queues_scheduled_outcome_failed = FAILED
queues_scheduled_reason_missed = Missed - forge was not running at the due time
queues_scheduled_reason_missed_by = Missed by { $span } - forge was not running at the due time
queues_scheduled_reason_superseded = Replaced by a newer run with the same key
queues_scheduled_reason_cancelled = Cancelled before it was due
queues_scheduled_reason_action_removed = The action was deleted
queues_scheduled_reason_action_disabled = The action was disabled
queues_scheduled_reason_action_not_found = The action no longer exists
queues_scheduled_reason_actions_unavailable = Actions could not be read
queues_scheduled_reason_queues_closed = Queues were shutting down
queues_scheduled_reason_unreadable = The stored run could not be read
queues_scheduled_reason_queue_paused = Its queue was paused
queues_scheduled_reason_queue_draining = Its queue was draining
queues_scheduled_reason_queue_overflow = Its queue was full
queues_scheduled_reason_queue_not_found = Its queue no longer exists
scheduled_span_minutes = { $minutes } min
scheduled_span_hours = { $hours } h { $minutes } min
scheduled_span_days = { $days } d { $hours } h

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




## Platforms - feature chips


## Twitch panel

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
builtin_picker_transition = Choose a Transition
builtin_picker_profile = Choose a Profile
builtin_picker_scene_collection = Choose a Scene Collection
builtin_picker_model = Choose a Model
builtin_picker_item = Choose an Item
builtin_picker_item_instance = Choose a Loaded Item

## OAuth / local callback flow

oauth_connect_explainer_prefix = Forge uses
oauth_connect_explainer_emphasis = { " " }OAuth 2.0 Authorization Code with PKCE
oauth_connect_explainer_suffix = . Your browser opens { $name }, and the response returns to a local loopback address - no secret is ever stored.
oauth_status_authorizing = Authorizing
oauth_status_authorized = Authorized
oauth_step_open_title = Open { $name } in your browser
oauth_step_approve_title = Approve access on the platform
oauth_step_approve_caption = Forge is listening on the loopback redirect for the authorization code.
oauth_step_exchange_title = Exchange code for token (+ verifier)
oauth_step_exchange_caption = PKCE code_verifier proves the request; tokens are stored encrypted on this device.
oauth_step_connected_title = Connected
oauth_progress_launching = Launching your browser...
oauth_progress_waiting = Waiting for you to approve on { $name }...
oauth_progress_subline = loopback :{ $port } · scopes: { $scopes }
oauth_done_authorized = Authorized - finishing up...
oauth_footer_choose_different = Choose different platform
oauth_footer_signin = Sign in with { $name }
oauth_btn_retry = Retry
oauth_btn_cancel = Cancel
oauth_failed_title = Authorization failed
oauth_kick_hybrid_title = Hybrid chat transport

## Twitch device code flow

twitch_device_explainer_prefix = Twitch uses the
twitch_device_explainer_emphasis = { " " }OAuth 2.0 Device Code
twitch_device_explainer_suffix = { " " }grant. Open the link, enter the code, and Forge auto-detects when you're done - no secret is ever stored.
twitch_device_open_title = Open this URL in any browser
twitch_device_open_btn = Open
twitch_device_enter_title = Enter this code on the page
twitch_device_copy = Copy
twitch_device_copied = Copied
twitch_device_expires_in = Expires in
twitch_device_get_new_code = Get new code
twitch_device_waiting = Waiting for you to authorize on { $name }...
twitch_device_requesting = Requesting a code from { $name }...
twitch_device_polling_subline = polling every { $interval }s · scopes: { $scopes }
twitch_device_do_later = I'll do this later
twitch_device_expired_title = This code expired before you finished
twitch_device_denied_title = Authorization denied
twitch_device_failed_title = Could not start authorization
twitch_device_denied_detail = You declined access on Twitch. Retry to request a new code.

## Server screen

server_breadcrumb_server = WebSocket server
server_status_listening = Listening · { $clients ->
    [one] { $clients } client
   *[other] { $clients } clients
  }
server_status_stopped = Stopped
server_not_running = Not running
server_console_disabled_hint = Server is switched off
server_unavailable_banner = The server could not start: { $reason }. Enabling it will not help until this is fixed; check the log and restart forge.
server_console_disabled_hint_link = Enable in Settings -> WebSocket
server_bind_address = BIND ADDRESS
server_bind_address_loading = Reading address...
server_bearer_token = BEARER TOKEN
server_btn_restart = Restart
server_btn_restarting = Restarting...
server_stat_clients = CLIENTS
server_stat_clients_sub = +{ $count } last { $minutes } min
server_stat_events_rate = EVENTS / SEC
server_stat_events_sub = { $seconds }s avg
server_stat_http = HTTP REQUESTS
server_stat_http_sub = overlays served
server_stat_bandwidth = BANDWIDTH
server_stat_bandwidth_value = { $rate } KB/s
server_stat_bandwidth_sub = ↑ out
server_clients_header = Connected clients
server_clients_empty = No clients connected
server_col_client = CLIENT
server_col_subscriptions = SUBSCRIPTIONS
server_col_evs = EV/S
server_col_uptime = UPTIME
server_overlay_files_empty = No overlay files found
server_overlay_dir_files = { $count ->
    [one] { $count } file
   *[other] { $count } files
  }
server_disconnect_confirm_hint = Client at { $info } will be disconnected from the WebSocket server. Other clients are not affected.
server_disconnect_tooltip = Disconnect this client
server_btn_regenerate = Regenerate
server_regen_warning_title = Regenerating disconnects all clients
server_throughput_title = Throughput
server_throughput_meta = last { $seconds }s · max { $max } ev/s
server_overlay_host_title = Overlay host
server_overlay_browser_source_url = BROWSER SOURCE URL
server_open_overlay_folder = Open the overlay folder
server_open_overlay_folder_failed = Could not open the overlay folder
server_clients_live = live
server_footer_totals = Total sent: { $sent } · Total events: { $events }
server_footer_health_ok = Healthy
server_footer_health_degraded = Degraded ({ $dropped } dropped)
server_footer_endpoint_accepting = WebSocket :{ $port } · accepting connections
server_footer_endpoint_stopped = WebSocket · not running
server_disconnect_confirm_title = Disconnect client?
server_disconnect_esc_hint = to cancel
server_btn_disconnect = Disconnect

## Common status badges (shared across platform detail pages)

common_status_not_connected = Not connected
common_status_connected = Connected

## YouTube platform detail

twitch_description = Chat, subs, bits, raids, channel points, and EventSub.
youtube_description = Live chat, super chats, channel memberships, subscribers.

## Kick platform detail

kick_description = Chat, subs, hosts - hybrid: official OAuth API for send, community Pusher WS for receive. Not affiliated with Kick.com.

## VTube Studio platform detail

## Stream apps overview


## Live Chat - page header / filters

chat_badge_owner = OWNER
chat_badge_mod = MOD
chat_badge_vip = VIP
chat_badge_sub = SUB
chat_badge_bot = BOT
chat_badge_partner = PARTNER
chat_badge_prime = PRIME
chat_badge_founder = FOUNDER
chat_badge_turbo = TURBO
chat_badge_hype = HYPE
chat_badge_bits = BITS
chat_badge_bits_leader = BITS LEADER
chat_breadcrumb_audience = Audience
chat_breadcrumb_chat = Chat
chat_filter_all = All
chat_filter_events = Events only
chat_filter_hide_bots = Hide bots
chat_viewers_unit = viewers
chat_no_filter_matches = No messages match these filters.
chat_send_placeholder_connected = Send to chat...
chat_send_placeholder_to = Send to {$platform} chat...
chat_send_placeholder_offline = No chat platform connected
chat_send_failed = Send to { $platform } failed: { $error }
chat_send_no_response = { $platform } did not confirm the message
chat_send_no_platform_enabled = No chat platform is enabled
chat_send_no_platform_connected = No enabled chat platform is connected
chat_send_too_long = Message is too long: { $count } of { $limit } characters
chat_send_unavailable = Sending to chat is not available
chat_search_placeholder = Search messages...
chat_new_message = 1 new message
chat_new_messages = { $count } new messages
chat_gap_messages = { $count ->
    [one] { $count } message skipped here
   *[other] { $count } messages skipped here
}
chat_gap_events = { $count ->
    [one] { $count } event skipped here - some may have been messages
   *[other] { $count } events skipped here - some may have been messages
}
chat_gap_both = { $messages } messages and { $events } other events skipped here
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
chat_drawer_whisper_placeholder = Type a message...
chat_drawer_whisper_send = Send
chat_drawer_whisper_cancel = Cancel
chat_drawer_set_tts_voice = Set TTS voice...
chat_drawer_block_tts = Block from TTS
chat_drawer_timeout = Timeout 10 min
chat_drawer_ban = Ban from channel
chat_stat_messages = MESSAGES
chat_stat_sub = SUB
chat_stat_sub_yes = Yes
chat_stat_follow = FOLLOW
chat_follow_not_following = Not following
chat_follow_hidden = Hidden
chat_drawer_history_loading = Loading messages...
chat_drawer_history_empty = No saved messages
chat_drawer_history_failed = Could not load message history
chat_drawer_history_older_failed = Could not load older messages
chat_drawer_history_retry = Retry
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
chat_drawer_tts_voice_saved = TTS voice saved
chat_drawer_tts_voice_failed = Set TTS voice failed: { $error }
chat_ctx_timeout_10m = Timeout 10 min
chat_ctx_timeout_1h = Timeout 1 hour
chat_ctx_timeout_2w = Timeout 2 weeks
chat_ctx_ban = Ban
chat_ctx_timeout_sent = Timeout applied
chat_reply = Reply
chat_reply_title = Replying to @{ $recipient }
chat_reply_placeholder = Type a reply...
chat_reply_sent = Reply sent
chat_reply_failed = Reply failed: { $error }
chat_tab_feed = Feed
chat_tab_banned = Banned
chat_banned_col_viewer = Viewer
chat_banned_col_reason = Reason
chat_banned_col_moderator = Moderator
chat_banned_col_banned = Banned
chat_banned_col_expires = Expires
chat_banned_permanent = Permanent
chat_banned_until = until { $time }
chat_banned_unban = Unban
chat_banned_unban_failed = Unban failed: { $error }
chat_banned_outside_forge = Banned outside forge - lift it in YouTube Studio
chat_banned_loading = Loading bans...
chat_banned_empty = No active bans
chat_banned_load_more = Load more
chat_banned_retry = Retry
chat_banned_no_platform = No connected platform offers a ban list
chat_banned_not_connected = Not connected
chat_banned_missing_permission = Missing permission - reconnect to grant it
chat_banned_try_later = Could not load bans - try again later
chat_banned_seen_by_forge = Only bans forge issued or saw while connected - not the full platform list
chat_dispatch_cancelled = The action was cancelled before it finished

## Event Feed - page header / filters

event_feed_filter_all = All { $n }
event_feed_step_count = { $count ->
    [one] { $count } step
   *[other] { $count } steps
}
event_feed_filter_chat = Chat { $n }
event_feed_filter_subs = Subs { $n }
event_feed_filter_bits = Bits { $n }
event_feed_filter_timers = Timers { $n }
event_feed_filter_obs = OBS { $n }
event_feed_filter_errors = Errors { $n }
event_feed_export_success = Exported event feed to { $path }
event_feed_export_failed = Event feed export failed: { $error }
event_feed_replay_success = Event replayed
event_feed_replay_not_found = That event is no longer in the event log and cannot be replayed.
event_feed_replay_failed = Event replay failed: { $error }
event_feed_replay_badge = Replay
event_feed_no_events = No events yet - system activity appears here in real time.
event_feed_no_filter_match = No events match the active filter.
event_feed_inspector_title = Event inspector
event_feed_auto_scroll_on = Auto-scroll on
event_feed_auto_scroll_off = Auto-scroll off
event_feed_breadcrumb_automation = Automation
event_feed_events_live_stream = events · live stream
event_feed_status_live = Live
event_feed_status_paused = Paused
event_feed_search_placeholder = Search events...
event_feed_outcome_trigger = → trigger
event_feed_outcome_actions = { $count ->
    [one] → { $count } action
   *[other] → { $count } actions
}
event_feed_outcome_no_match = no match
event_feed_outcome_sent = sent
event_feed_duration_ms = { $ms }ms

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
script_editor_search_placeholder = Search scripts...
script_editor_new_script = New script
script_editor_no_scripts = No scripts yet
script_editor_group_action = Action scripts
script_editor_group_standalone = Standalone
script_editor_manual_run = manual run
script_editor_rename_action = Rename
script_editor_enable_action = Enable
script_editor_disable_action = Disable
script_editor_delete_action = Delete
script_editor_running = Running...
script_editor_run_modal_cancel = Cancel
script_editor_save_blocked = Save blocked - fix syntax errors first
script_editor_discard_title = Discard unsaved changes?
script_editor_discard_body = This script has unsaved edits. Continue and lose them, or stay to keep editing.
script_editor_discard_confirm = Discard
script_editor_discard_cancel = Keep editing
script_editor_discard_esc_hint = to keep editing
script_editor_sandbox_label = Sandbox:
script_editor_timeout_ms = Timeout: { $ms } ms
script_editor_sandbox_enabled = enabled
script_editor_problems_tab = Problems
script_editor_console_cleared = Console cleared.
script_editor_no_problems = No problems.
script_editor_inert_annotation = This annotation no longer declares anything - inputs are edited in the SIGNATURE panel.

## Script Editor - run modal

script_editor_health = { $ok }/{ $total } healthy
script_editor_type_check_passed = Type-check passed
script_editor_type_check_failed = Type-check failed
script_editor_run_modal_title = Run { $name }
script_editor_run_input_placeholder = Enter { $label } value...
script_editor_run_input_placeholder_example = Enter { $label } value, e.g. { $example }

## Action telemetry - stat column headers

## Action editor - validation errors

## Integration detail - OBS / quick-action

builtin_disconnect_confirm_hint = You will be disconnected and will need to reconnect manually. Live events from this integration stop arriving until then.
integration_disconnect_title = Disconnect integration
integration_settings_coming_soon = Settings coming soon
integration_control_failed = Control command failed
integration_quick_action_failed = Quick action failed
integration_row_action_failed = Could not run that row action: { $error }
integration_open_url_failed = Could not open the link in your browser
integration_quick_action_na = N/A
integration_quick_action_not_live = Unavailable in current state
integration_status_authenticated = Authenticated
integration_token_expires_in = token expires in { $time }
integration_viewers_delta = { $delta } last { $window }
integration_qa_filter_placeholder = Filter actions...
integration_qa_no_matches = No actions match your filter
integration_quick_action_ran = Ran "{ $label }"
integration_run_history = Run history
integration_run_history_failed = Could not load run history
integration_qa_modal_subtitle = Test this without writing an action
integration_qa_modal_confirm_body = Run "{ $label }" right now using current channel state?
integration_qa_modal_footer_immediate = Runs immediately - no action created
integration_qa_modal_footer_destructive = This affects your live channel immediately
integration_qa_modal_run = Run
integration_qa_modal_confirm = Confirm
integration_qa_field_loading = Loading options...
integration_qa_field_select = Select...
integration_qa_field_unavailable = Options are unavailable while this integration is disconnected
integration_qa_field_no_scene = No active scene to read sources from
integration_qa_scene_current_hint = Currently: { $scene } (LIVE)
integration_qa_source_visible = visible
integration_qa_source_hidden = hidden
collection_modal_subtitle = Changes apply directly - no action created
collection_form_creating = New item
collection_form_editing = Editing "{ $title }"
collection_form_footer = Saved directly to the integration
collection_new = New
collection_save = Save
collection_close = Close
collection_loading = Loading...
collection_empty = Nothing here yet
collection_read_only = READ-ONLY
collection_count = { $count } items
collection_count_of = { $count } of { $capacity } used
collection_count_full = Limit reached - { $capacity } of { $capacity }
collection_count_unknown = -
collection_saved = Saved "{ $title }"
collection_deleted = Deleted "{ $title }"
collection_delete_title = Delete item?
collection_delete_body = This removes it from the integration permanently.
collection_delete_confirm = Delete
collection_field_unsupported = This field can't be edited here
collection_field_max_chars = Up to { $max } characters
collection_failure_not_connected = Not connected - connect the integration first
collection_failure_unauthorized = Authorization expired - sign in again
collection_failure_not_owned = Created outside forge - it can't be changed here
collection_failure_capacity = The limit is reached - delete an item first
collection_failure_not_eligible = This channel isn't eligible for this feature - Twitch channel point rewards need Affiliate or Partner status
collection_failure_rate_limited = Rate limited - try again shortly
collection_failure_transport = Connection error - try again
collection_text_channel_point_rewards = Channel point rewards
collection_text_title = Title
collection_text_cost = Cost (Channel Points)
collection_text_prompt = Prompt
collection_text_optional = optional
collection_text_require_input = Require viewer input
collection_text_background_color = Background color (#RRGGBB)
collection_text_max_per_stream = Max per stream (0 = unlimited)
collection_text_max_per_viewer = Max per viewer per stream (0 = unlimited)
collection_text_global_cooldown = Global cooldown seconds (0 = off)
collection_text_enabled = Enabled
collection_text_paused = Paused
collection_text_created_outside = Created outside forge - edit it in the Twitch dashboard
collection_text_title_required = Title is required
collection_text_color_format = Background color must be empty or #RRGGBB
collection_text_zero_or_more = Must be 0 or more
collection_text_duplicate_title = A reward with this title already exists
collection_text_automod_rejected = Twitch AutoMod rejected the reward text
collection_text_settings_rejected = Twitch rejected the reward settings
collection_text_reward_gone = This reward no longer exists
collection_text_rejected_per_viewer = Twitch rejected the per-viewer limit
collection_text_rejected_per_stream = Twitch rejected the per-stream limit
collection_text_rejected_cooldown = Twitch rejected the cooldown
collection_text_rejected_color = Twitch rejected the background color
collection_text_rejected_prompt = Twitch rejected the prompt
collection_text_rejected_cost = Twitch rejected the cost
collection_text_rejected_title = Twitch rejected the title
collection_text_unknown_collection = Unknown collection
collection_text_unknown_toggle = Unknown reward toggle
collection_text_title_max = Title must be at most { $max } characters
collection_text_prompt_max = Prompt must be at most { $max } characters
collection_text_cost_min = Cost must be at least { $min }
integration_state_connecting_title = Connecting...
integration_state_connecting_detail = Establishing a session with this integration.
integration_state_reconnecting_title = Reconnecting...
integration_state_reconnecting_detail = The session dropped; forge is re-establishing it.
integration_state_disconnected_detail = Use Reconnect above to link this integration.
obs_state_password_rejected_title = OBS rejected the saved password
obs_state_password_rejected_detail = Enter the current WebSocket password in the OBS connection settings, then reconnect.

## OAuth / authentication errors

auth_error_credentials_missing_twitch = Twitch integration is not configured. Set FORGE_TWITCH_CLIENT_ID with your own registered application's client_id and restart the app.
auth_error_credentials_missing_youtube = YouTube OAuth client credentials are not configured
auth_error_credentials_missing_kick = Kick integration is not configured. Set FORGE_KICK_CLIENT_ID and FORGE_KICK_CLIENT_SECRET with your own registered application's credentials and restart the app.

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
widget_header_action_refresh_token = Refresh token
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
widget_builtin_chip_grid_empty = No items yet

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

## Widget - date time picker

## Widget - save indicator

widget_copied_toast = Copied to clipboard
widget_save_all_saved = All changes saved

## Widget - server bearer token

## Widget - server bind card

## Widget - picker modal

widget_picker_search_placeholder = Search...
widget_picker_loading = Loading...
widget_picker_no_results = No results.

## Widget - output device picker

widget_device_default_suffix = (default)
widget_device_test = Test

## Widget - quick actions panel

widget_quick_actions_title = Quick actions

## Widget - console (script output)

## Settings - audio output

settings_audio_scanning = Scanning devices...
settings_audio_title = Output
settings_audio_output_devices = Output device
settings_audio_test_playing = Playing...
settings_audio_test_error = Test tone error: { $error }
settings_audio_persist_error = Failed to save device selection: { $error }

## Settings - audio routing

settings_audio_routing_title = Routing
settings_audio_routing_loading = Reading routing settings
settings_audio_routing_load_error = Failed to read routing settings: { $error }
settings_audio_routing_speech_label = Speech plays on
settings_audio_routing_speech_hint = Where synthesized speech is heard.
settings_audio_routing_clips_label = Sound clips play on
settings_audio_routing_clips_hint = Where soundboard clips are heard.
settings_audio_route_local = Local device
settings_audio_route_overlay = Audio overlay
settings_audio_route_both = Both
settings_audio_routing_destination = Audio overlay
settings_audio_routing_destination_none = None
settings_audio_routing_no_overlays = Create an overlay on the Overlays screen first.
settings_audio_routing_test = Test on overlay
settings_audio_routing_testing = Playing
settings_audio_routing_test_error = Overlay test failed: { $error }
settings_audio_routing_persist_error = Failed to save routing: { $error }
settings_audio_routing_in_effect = In effect now
settings_audio_routing_in_effect_fallback = { $route } - { $reason }
settings_audio_routing_fallback_unchosen = no audio overlay is chosen
settings_audio_routing_fallback_server_off = the server that carries overlay audio is not running
settings_audio_routing_fallback_unreadable = the chosen audio overlay could not be looked up
settings_audio_routing_fallback_missing = the chosen audio overlay no longer exists
settings_audio_routing_pages_none = No page connected
settings_audio_routing_pages_connected =
    { $count ->
        [one] { $count } page connected
       *[other] { $count } pages connected
    }
settings_audio_routing_pages_duplicate = Every connected page plays the same audio, so the clip is heard { $count } times on stream.

## Settings - voice gate

settings_voice_gate_title = Voice gate
settings_voice_gate_enable_label = Hold speech while you talk
settings_voice_gate_enable_hint = Pauses the speak queue whenever your microphone picks up sound above the threshold.
settings_voice_gate_input_devices = Input device
settings_voice_gate_scanning = Scanning devices...
settings_voice_gate_threshold_label = Threshold
settings_voice_gate_threshold_hint = Peak input level that counts as speech. The meter shows the live level.
settings_voice_gate_hold_label = Hold
settings_voice_gate_hold_hint = How long the queue stays held after you stop talking, in milliseconds.
settings_voice_gate_hold_placeholder = 800
settings_voice_gate_hold_range = Allowed: { $min }-{ $max } ms
settings_voice_gate_state_off = Off
settings_voice_gate_state_inactive = Listening - no speech detected
settings_voice_gate_state_active = Speech detected - queue held
settings_voice_gate_state_unavailable = Microphone unavailable: { $error }
settings_voice_gate_retry = Retry
settings_voice_gate_persist_error = Failed to save voice gate settings: { $error }

## Script editor - API docs panel

script_editor_api_no_matches = No matches
script_editor_api_search_placeholder = Search modules...

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
script_editor_signature_add_input = Add input
script_editor_signature_no_inputs = No inputs declared
script_editor_signature_no_return = not declared
script_editor_input_name_empty = An input name cannot be empty
script_editor_input_name_invalid = "{ $name }" is not a valid identifier
script_editor_input_name_reserved = "{ $name }" is reserved by the engine
script_editor_input_name_duplicate = "{ $name }" is already declared
script_editor_run_stats_heading = RUN STATS
script_editor_stat_runs = RUNS
script_editor_stat_avg = AVG
script_editor_stat_runs_value = { $n } today
script_editor_stat_avg_value = { $n } ms

## Widget - layout chrome

## Widget - volume slider

## Locale-aware formatting - month abbreviations (en)

## Locale-aware formatting - relative time

## Storage error screen

## Unavailable integration
builtin_unavailable_metric_status = Status
builtin_unavailable_metric_uptime = Uptime
builtin_unavailable_metric_endpoint = Endpoint
builtin_unavailable_metric_version = Version

obs_connect_title = OBS Studio
obs_connect_subtitle = Connect to control scenes, sources, audio, filters, and recording
obs_connect_guide_title = Before you start
obs_connect_guide_intro = In OBS Studio, enable the built-in WebSocket server, then copy the settings here.
obs_connect_step_menu_prefix = In OBS:
obs_connect_step_menu_path = Tools → WebSocket Server Settings
obs_connect_step_enable_prefix = Check
obs_connect_step_enable_option = Enable WebSocket server
obs_connect_step_port_prefix = Note the
obs_connect_step_port_field = port
obs_connect_step_port_default_prefix = (default
obs_connect_step_reveal_prefix = Click
obs_connect_step_reveal_button = Show Connect Info
obs_connect_step_reveal_suffix = to reveal the password
obs_connect_settings_title = Connection settings
obs_connect_field_host = HOST
obs_connect_field_port = PORT
obs_connect_field_password = PASSWORD
obs_connect_password_note = stored encrypted
obs_connect_auto_reconnect_label = Auto-reconnect on disconnect
obs_connect_auto_reconnect_hint = Retry with exponential backoff
obs_connect_on_launch_label = Connect on app launch
obs_connect_on_launch_hint = Start connecting when Forge opens
obs_connect_btn_test = Test connection
obs_connect_btn_connect = Connect
obs_connect_btn_save_reconnect = Save & reconnect
obs_connect_testing = Testing connection
obs_connect_connecting = Connecting
obs_connect_test_successful = Test successful
obs_connect_test_detail = obs-websocket v{ $version } · { $scenes } scenes · { $rtt }ms RTT
obs_connect_test_failed = Test failed
obs_connect_failed = Connection failed
obs_connect_error_title = Invalid settings
obs_connect_error_unreachable = Nothing answered on that host and port. Is OBS running with its WebSocket server on?
obs_connect_error_auth = OBS rejected the password
obs_connect_error_timeout = OBS did not answer in time
obs_connect_error_unknown = OBS ended the connection attempt
obs_connect_error_invalid_port = Port must be a number between 1 and 65535
obs_connect_settings_save_failed = Could not save the OBS connection settings

vtube_connect_title = VTube Studio
vtube_connect_subtitle = Connect to control your avatar: expressions, hotkeys, and thrown items
vtube_connect_guide_title = Before you start
vtube_connect_guide_intro = VTube Studio must be running with its API enabled. You approve Forge as a plugin inside the app.
vtube_connect_step_open_prefix = In VTube Studio: open
vtube_connect_step_open_path = Settings → API
vtube_connect_step_enable_prefix = Enable
vtube_connect_step_enable_option = Start API (allow plugins)
vtube_connect_step_port_prefix = Note the
vtube_connect_step_port_field = port
vtube_connect_step_port_default_prefix = (default
vtube_connect_step_approve_prefix = Click
vtube_connect_step_approve_button = Connect
vtube_connect_step_approve_suffix = here, then Allow the popup in VTube Studio
vtube_connect_settings_title = Connection settings
vtube_connect_field_host = HOST
vtube_connect_field_port = PORT
vtube_connect_field_plugin = PLUGIN NAME
vtube_connect_auto_reconnect_label = Auto-reconnect on disconnect
vtube_connect_auto_reconnect_hint = Retry with exponential backoff
vtube_connect_on_launch_label = Connect on app launch
vtube_connect_on_launch_hint = Start connecting when Forge opens
vtube_connect_idle_hint = Not connected yet - start VTube Studio, then connect.
vtube_connect_awaiting_title = Waiting for approval in VTube Studio...
vtube_connect_awaiting_hint = click "Allow" on the plugin popup
vtube_connect_btn_test = Test connection
vtube_connect_btn_connect = Connect
vtube_connect_btn_authorizing = Authorizing...
vtube_connect_testing = Testing connection
vtube_connect_connecting = Connecting
vtube_connect_test_successful = Test successful
vtube_connect_test_detail = VTube Studio { $version } · { $rtt }ms RTT · { $auth }
vtube_connect_test_authorized = already authorized
vtube_connect_test_unauthorized = approval needed
vtube_connect_test_failed = Test failed
vtube_connect_failed = Connection failed
vtube_connect_error_title = Invalid settings
vtube_connect_error_invalid_port = Port must be a number between 1 and 65535
vtube_connect_error_unreachable = Nothing answered on that host and port. Is VTube Studio running with its API on?
vtube_connect_error_token_rejected = The stored plugin token was rejected. Approve Forge again.
vtube_connect_error_denied = Approval was declined in VTube Studio
vtube_connect_error_timeout = The approval popup was not answered in time
vtube_connect_error_auth = Plugin authentication failed
vtube_connect_error_subscribe = Authorized, but subscribing to VTube Studio events failed
vtube_connect_error_socket_closed = VTube Studio closed the connection
vtube_connect_error_unresponsive = VTube Studio stopped answering requests
vtube_connect_error_unknown = VTube Studio ended the connection attempt
vtube_connect_settings_save_failed = Could not save the VTube Studio connection settings
obs_settings_disconnect_hint = The WebSocket session closes. Scene switches and source toggles stop working until you reconnect.

## Common - row actions

common_duplicate = Duplicate

## MIDI - breadcrumb + header

midi_header_summary = { $devices ->
    [one] { $devices } device
   *[other] { $devices } devices
    } · { $mappings ->
    [one] { $mappings } mapping
   *[other] { $mappings } mappings
    }

## MIDI - hero

midi_hero_blurb = Bind notes, control changes, and faders from any MIDI controller to Forge actions.
midi_hero_enabled = Enabled
midi_hero_disabled = Disabled
midi_toggle_failed = Could not change the MIDI input state

## MIDI - devices

midi_section_devices = Devices
midi_device_port = in · port { $index }
midi_device_offline = offline
midi_device_maps = { $count } maps
midi_devices_empty = No MIDI devices seen yet
midi_rescan_ports = Rescan ports

## MIDI - input monitor

midi_section_monitor = Input monitor
midi_monitor_empty = Waiting for MIDI input...
midi_monitor_disabled = MIDI input is off - enable it above to see traffic

## MIDI - mappings

midi_section_mappings = Mappings
midi_bindings_count = { $count ->
    [one] { $count } binding
   *[other] { $count } bindings
    }
midi_value_any = any
midi_unassigned = Unassigned - click to pick an action
midi_mappings_empty = No MIDI mappings yet
midi_mappings_empty_device = No mappings for this device yet
midi_mapping_any_device = all devices
midi_menu_edit = Edit mapping...
midi_add_learn = MIDI Learn
midi_add_learn_kbd = listen
midi_learn_prompt = Move a control or press a key...
midi_learn_input_disabled = MIDI input is off - enable it above to capture
midi_toast_error = MIDI: { $message }

## MIDI - mapping modal

midi_modal_title_add = Assign MIDI mapping
midi_modal_title_edit = Edit MIDI mapping
midi_modal_subtitle_captured = Signal captured - pick what it should do
midi_modal_section_signal = Signal
midi_modal_section_action = Action to run
midi_modal_input = INPUT
midi_modal_listening = listening...
midi_modal_type = TYPE
midi_modal_channel = CHANNEL
midi_modal_channel_any = any
midi_modal_device = DEVICE
midi_modal_device_any = Any device
midi_modal_relearn = Re-learn
midi_modal_input_disabled = MIDI input is off - re-learn is unavailable until you enable it.
midi_modal_filter_actions = Filter actions...
midi_modal_actions_loading = Loading actions...
midi_modal_actions_empty = No actions match the filter
midi_modal_actions_none = No actions exist yet - create one first
midi_modal_hint_ready = Ready to save
midi_modal_hint_pick_action = Pick an action to continue
midi_modal_add_mapping = Add mapping
midi_modal_save_changes = Save changes

## MIDI - footer

midi_footer_left = { $devices ->
    [one] { $devices } device connected
   *[other] { $devices } devices connected
    } · { $mappings ->
    [one] { $mappings } active mapping
   *[other] { $mappings } active mappings
    }
midi_engine_running = MIDI engine running
midi_engine_stopped = MIDI engine stopped

## Hotkeys - hero

hotkeys_hero_title = Hotkeys
hotkeys_hero_blurb = System-wide keyboard shortcuts that fire actions even when Forge is in the background.
hotkeys_hero_enabled = Enabled
hotkeys_hero_disabled = Disabled
hotkeys_header_summary = { $count } active · global
hotkeys_toggle_failed = Could not change the hotkey engine state
hotkeys_toggle_binding_failed = Could not change the binding state
hotkeys_toast_error = Hotkey operation failed: { $message }
hotkeys_toast_enable_partial = { $count } combinations could not be re-registered with the system
hotkeys_toast_bare_key_global = { $combo } without a modifier is captured system-wide: while forge runs, other apps no longer receive this key

## Hotkeys - stats

hotkeys_stat_bindings = Bindings
hotkeys_stat_bindings_hint = { $count } enabled
hotkeys_stat_global = Global
hotkeys_stat_global_hint = OS-wide
hotkeys_stat_conflicts = Conflicts
hotkeys_stat_conflicts_none = no clashes
hotkeys_stat_conflicts_hint = since startup
hotkeys_stat_last_fired = Last fired
hotkeys_stat_last_fired_none = nothing yet

## Hotkeys - bindings

hotkeys_section_bindings = Bindings
hotkeys_section_hint = double-click a combo to rebind
hotkeys_unassigned = Unassigned - pick an action
hotkeys_scope_global = Global
hotkeys_scope_app = App
hotkeys_scope_unregistered = Not registered
hotkeys_menu_edit = Edit...
hotkeys_menu_edit_press = Edit press half...
hotkeys_menu_edit_release = Edit release half...
hotkeys_menu_add_press = Add press half...
hotkeys_menu_add_release = Add release half...
hotkeys_menu_remove_press = Remove press half
hotkeys_menu_remove_release = Remove release half
hotkeys_badge_hold = HOLD
hotkeys_badge_release_edge = ON RELEASE
hotkeys_row_stop = stop: { $action }
hotkeys_row_stop_unassigned = stop: unassigned
hotkeys_menu_unbind = Unbind
hotkeys_menu_reset_default = Reset to default
hotkeys_add_binding = Add hotkey
hotkeys_add_binding_kbd = capture
hotkeys_capture_prompt = Press the key combination...

## Hotkeys - conflict

hotkeys_confirm_delete_title = Delete hotkey?
hotkeys_confirm_delete_body = { $action } will no longer run on this combination. This cannot be undone.
hotkeys_confirm_delete_body_unassigned = This combination is not linked to an action. This cannot be undone.
hotkeys_confirm_delete_body_pair = Both the press and the release half of this combination will be removed. This cannot be undone.
hotkeys_confirm_delete_press_half = Only the press half is removed; the release half keeps the combination registered.
hotkeys_confirm_delete_release_half = Only the release half is removed, so the hold will start but never stop.

hotkeys_conflict_title = Combination already bound
hotkeys_conflict_body = is already bound to { $holder }. Replace the existing binding or cancel?
hotkeys_conflict_holder_unassigned = an unassigned binding
hotkeys_conflict_holder_clip = Soundboard: { $name }
hotkeys_conflict_replace = Replace

## Hotkeys - modal

hotkeys_modal_title_add = Bind hotkey
hotkeys_modal_title_edit = Edit hotkey
hotkeys_app_modal_subtitle = In-app shortcut
hotkeys_modal_subtitle_captured = Captured combination
hotkeys_modal_section_combo = Combination
hotkeys_modal_section_action = Action
hotkeys_modal_section_edge = Fires on
hotkeys_modal_edge_press = On press
hotkeys_modal_edge_release = On release
hotkeys_modal_edge_locked_press = The press half of this combination is already bound.
hotkeys_modal_edge_locked_release = The release half of this combination is already bound.
hotkeys_modal_edge_queue_hint = Put the two halves on different queues - a slow press action blocks the release on a shared serial queue.
hotkeys_modal_recapture = Re-capture
hotkeys_modal_filter_actions = Filter actions...
hotkeys_modal_actions_loading = Loading actions...
hotkeys_modal_actions_empty = No actions match the filter
hotkeys_modal_actions_none = No actions exist yet - create one first
hotkeys_modal_hint_ready = Ready to save
hotkeys_modal_hint_pick_action = Pick an action to continue
hotkeys_modal_add_binding = Add hotkey
hotkeys_modal_save_changes = Save changes

## Hotkeys - footer

hotkeys_footer_bindings = { $count } bindings
hotkeys_footer_listening = global listener active
hotkeys_footer_stopped = global listener stopped
hotkeys_footer_no_conflicts = No conflicts
hotkeys_footer_conflicts = { $count } conflicts
hotkeys_footer_hold_closed = hold force-closed { $when }

## Discord - breadcrumb + hero

discord_hero_title = Discord
discord_hero_blurb = Post to your server channels through incoming webhooks. Each binding is one channel endpoint an action can write to.
discord_hero_webhooks = { $count } webhooks
discord_hero_webhooks_sub = configured
discord_header_summary = { $webhooks } webhooks · { $actions } actions
discord_toast_error = Webhook operation failed: { $message }
discord_toast_saved = Webhook saved
discord_toast_deleted = Webhook deleted
discord_toast_name_taken = A webhook named { $name } already exists
discord_toast_test_sent = Test message posted
discord_toast_test_failed = Test post failed: { $message }
discord_test_content = Test message from forge.

## Discord - stats

discord_stat_latency = Webhook latency
discord_stat_latency_value = { $ms } ms
discord_stat_latency_hint = p50 · last { $count }
discord_stat_no_sends = nothing sent yet
discord_stat_budget = Rate limit
discord_stat_budget_value = { $used } / { $total }
discord_stat_budget_hint = bucket used
discord_stat_budget_unknown = no bucket data yet
discord_stat_send = Last send
discord_stat_send_ok = Delivered
discord_stat_send_failed = Failed
discord_stat_send_none = no sends yet
discord_stat_errors = Errors
discord_stat_errors_hint = last 60 min

## Discord - channel bindings

discord_section_bindings = Channel bindings
discord_section_bindings_count = { $count } bound
discord_bindings_empty = No channel bound yet.
discord_binding_no_actions = no action posts here
discord_binding_action_count = { $count } actions post here
discord_add_binding = Bind channel
discord_menu_edit = Edit...
discord_menu_test = Test

## Discord - recent posts

discord_section_posts = Recent posts
discord_posts_empty = Nothing posted yet.
discord_posts_never = never
discord_post_kind_embed = embed
discord_post_kind_message = message

## Discord - webhook modal

discord_modal_title_add = Bind channel
discord_modal_title_edit = Edit binding
discord_modal_subtitle = Webhook endpoints are stored encrypted and never logged.
discord_modal_name_label = Name
discord_modal_name_placeholder = go-live
discord_modal_name_locked = Actions reference this webhook by name, so it cannot be renamed here.
discord_modal_url_label = Webhook URL
discord_modal_url_placeholder = https://discord.com/api/webhooks/...
discord_modal_url_hint = Copy it from Channel settings - Integrations - Webhooks.
discord_modal_url_invalid = This is not a Discord webhook URL.
discord_modal_test = Test post
discord_modal_testing = Posting...
discord_modal_add = Bind channel
discord_modal_save_changes = Save changes

## Discord - delete + footer

discord_confirm_delete_title = Delete webhook?
discord_confirm_delete_body = The endpoint will be removed from storage. This cannot be undone.
discord_confirm_delete_body_linked = { $count } actions post to this webhook and will start failing. This cannot be undone.
discord_footer_webhooks = { $count } webhooks
discord_footer_linked = { $count } linked actions
discord_footer_healthy = Sending
discord_footer_failing = Last send failed
discord_footer_idle = Nothing sent yet

## Overlays - registry

overlays_breadcrumb_overlays = Overlays
overlays_header_summary = { $enabled }/{ $total } active - served on :{ $port }
overlays_header_summary_stopped = { $enabled }/{ $total } active - server stopped
overlays_pane_title = Overlays
overlays_pane_loading = Loading overlays...
overlays_pane_empty = No overlays yet.
overlays_add_overlay = New overlay
overlays_menu_rename = Rename...
overlays_menu_copy_url = Copy URL
overlays_type_unavailable = Unavailable type
overlays_type_unavailable_notice = This overlay uses the type { $kind }, which this build does not have. The record is kept untouched.

## Overlays - selection + URL

overlays_url_copy = Copy
overlays_url_not_served = The server is stopped, so this overlay has no address yet.
overlays_stage_select = Select an overlay to work on it.
overlays_stage_empty = Create an overlay to get a browser-source address for OBS.

## Overlays - add / rename form

overlays_form_title_create = New overlay
overlays_form_title_rename = Rename overlay
overlays_form_subtitle_create = The address OBS uses is minted from this name once and never changes afterwards.
overlays_form_subtitle_rename = Only the display name changes - the address already in your OBS scenes stays the same.
overlays_form_name_label = Name
overlays_form_name_placeholder = Sub alerts
overlays_form_type_label = Overlay type
overlays_form_type_locked = The overlay type is fixed once the overlay exists.
overlays_form_create = Create overlay
overlays_form_save = Save name

## Overlays - toasts + delete

overlays_toast_created = Overlay created
overlays_toast_renamed = Overlay renamed
overlays_toast_deleted = Overlay deleted
overlays_toast_missing = That overlay is no longer in storage.
overlays_toast_unknown_type = This build does not have that overlay type.
overlays_toast_token_shaped_name = That name looks like an overlay token. Choose a different name.
overlays_toast_url_unavailable = The server is stopped, so there is no address to copy.
overlays_toast_build_failed_pages =
    { $count ->
        [one] { $count } overlay page could not be rebuilt - its browser source may show an old version. Details are in the log.
       *[other] { $count } overlay pages could not be rebuilt - their browser sources may show an old version. Details are in the log.
    }
overlays_toast_build_failed_pass = Overlay pages could not be rebuilt: { $reason }
overlays_toast_speech_failed = { $overlay } was shown without speech: { $error }. Pick an installed voice in its Voice field.
overlays_confirm_delete_title = Delete overlay?
overlays_confirm_delete_body = Any OBS browser source pointing at this overlay will stop loading. This cannot be undone.

overlays_mode_design = Design
overlays_mode_code = Code
overlays_code_placeholder = This file has not been written yet.
overlays_code_loading = Reading the file from the overlay folder...
overlays_code_meta = { $lines } lines - { $state }
overlays_code_state_saved = saved
overlays_code_state_unsaved = unsaved
overlays_code_state_saving = saving
overlays_code_save = Save
overlays_code_revert = Revert
overlays_code_restore = Restore
overlays_code_hint_generated = forge writes { $file } for you. Save an edit and the file becomes yours: it stops being regenerated, and design controls that need generated markup or styling stop reaching it.
overlays_code_hint_owned = You own { $file }. forge never rewrites it, so design controls that need generated markup or styling no longer reach it.
overlays_code_hint_predates_box = You own { $file } and it predates the browser-source sizing. Its old position or padding rules may still apply; Revert brings back the generated file.
overlays_code_hint_missing = The record claims { $file } as yours, but the file is gone from the overlay folder. Revert to let forge write it again.
overlays_code_footer_reload = Saving reloads every browser source showing this overlay. Served at
overlays_code_schema_notice = This overlay type changed in a newer build. Your files were left exactly as you wrote them: { $files }. Configuration still reaches whatever bindings they kept.
overlays_code_revert_title = Revert this file?
overlays_code_revert_body = forge writes the shipped version over your copy and the browser sources reload onto it. Your edits are gone.
overlays_code_revert_confirm = Revert file
overlays_code_discard_title = Discard unsaved edits?
overlays_code_discard_body = This file has edits that were never saved. Continue and lose them, or stay to keep editing.
overlays_code_discard_confirm = Discard
overlays_code_discard_cancel = Keep editing
overlays_preview_label = Live preview - { $width }x{ $height }
overlays_preview_scale_true = To scale
overlays_preview_scale_zoom = Zoom
overlays_preview_open = Open in browser
overlays_preview_open_failed = forge could not open this overlay page in your browser.
overlays_preview_browser_counts = A preview tab still receives the sample, but it does not count as a browser source.
overlays_preview_canvas_note = transparent - OBS browser source
overlays_preview_approximate = Approximate preview. The real overlay is drawn by the OBS browser source on a transparent background.
overlays_preview_unavailable = This build has no overlay type for this record, so there is nothing to preview.
overlays_preview_audio_silent = Draws nothing in OBS - it only plays audio there.
overlays_preview_audio_needs_source = Add it as an OBS browser source - an ordinary tab blocks autoplay.
overlays_test_send = Preview look
overlays_test_send_hint = Only draws the look in the preview and on connected pages - no queue, sound or speech.
overlays_test_sending = Building a sample for the bound event...
overlays_test_delivered = { $count ->
    [one] Sample delivered to { $count } browser source
   *[other] Sample delivered to { $count } browser sources
}
overlays_test_preview_only = { $count ->
    [one] Only a preview tab is connected - open the overlay URL in OBS to see it there
   *[other] Only { $count } preview tabs are connected - open the overlay URL in OBS to see it there
}
overlays_test_undelivered = The server is stopped, so no browser source received this sample. The preview above ran on its own.
overlays_test_no_browser_source = The server is running, but no browser source is connected to this overlay. Check the overlay URL in OBS and refresh the source.
overlays_bindings_heading = Data bindings
overlays_bindings_intro = Mark an element { $marker } and { $call } writes text into it. The content callback receives the fields below by name - hand each one to { $writer } to fill its element.
overlays_bindings_api_heading = Page API
overlays_bindings_fn_ready = Runs once the page has loaded, with this overlay's settings.
overlays_bindings_fn_content = Runs on every delivery with the content fields. durationMs is set only when the delivery overrides the duration.
overlays_bindings_fn_set = Writes text into every element whose data-bind equals name.
overlays_bindings_fn_show = Reveals the elements matching target. With ms, hides them again after ms, or once the speech ends if that is later.
overlays_bindings_fn_sound = Plays a file from this overlay's folder.
overlays_bindings_content_heading = Content fields
overlays_bindings_content_empty = This look declares no content fields. The content callback still runs on every delivery.
overlays_bindings_runtime_heading = Applied by forge
overlays_bindings_runtime_accent = Accent color
overlays_bindings_runtime_font = Font family
overlays_bindings_runtime_text_size = Text size in px, without a unit
overlays_bindings_runtime_position = Position: top or bottom
overlays_bindings_config_heading = Look settings
overlays_bindings_config_empty = This look has no settings of its own.
overlays_bindings_look_unavailable = This build has no overlay type for this record.
overlays_bindings_field_headline = Headline
overlays_bindings_field_subline = Subline
overlays_bindings_field_author = Author
overlays_bindings_field_author_color = Name color
overlays_bindings_field_badges = Badges
overlays_bindings_field_message = Message
overlays_bindings_field_label = Label
overlays_bindings_field_value = Current value
overlays_bindings_field_target = Target
overlays_bindings_field_placeholder = Empty text
overlays_bindings_field_icon = Icon
overlays_bindings_field_slot = Slot
overlays_bindings_field_platform = Platform
overlays_panel_section_content = Content
overlays_panel_section_style = Style
overlays_panel_section_source = Browser source
overlays_source_width = Width
overlays_source_height = Height
overlays_source_size_hint = Pixels. Give the OBS browser source exactly this size.
overlays_source_margin_top = Margin top
overlays_source_margin_right = Margin right
overlays_source_margin_bottom = Margin bottom
overlays_source_margin_left = Margin left
overlays_source_margin_hint = Animation room, in percent of the box. Content stays inside the margins; motion may use them.
overlays_sizing_notice_title = Sizing changed
overlays_sizing_notice_body = This overlay now fills its OBS browser source. Position was removed and the old padding became percent margins.
overlays_sizing_notice_size = Set your OBS browser source to { $width } x { $height }.
overlays_sizing_notice_ack = Got it
overlays_obs_title = OBS browser source
overlays_obs_url = URL
overlays_obs_width = Width
overlays_obs_height = Height
overlays_obs_shutdown = Shutdown source when not visible
overlays_obs_refresh = Refresh browser when scene becomes active
overlays_obs_custom_css = Custom CSS
overlays_obs_custom_css_value = OBS default
overlays_obs_audio = Control audio via OBS
overlays_obs_on = On
overlays_obs_off = Off
overlays_panel_section_behavior = Behavior
overlays_panel_section_look = Look
overlays_panel_section_audio = Audio
overlays_panel_section_display = Display
overlays_panel_section_receiver = Receiver
overlays_look_hint = Changing the look keeps the audio, display, style and browser source settings.
overlays_base_sound = Sound
overlays_base_speech = Speech text
overlays_base_speech_hint = Read aloud when the show appears. Event variables like %message% work here.
overlays_base_voice = Voice
overlays_base_voice_hint = An installed engine voice; Default voice uses the viewer's alias or the default voice.
overlays_voice_default = Default voice
overlays_voice_not_installed = { $voice } - not installed
overlays_base_duration = Duration
overlays_base_duration_hint = Stays up this long, or until its speech ends if that is later.
overlays_display_draws_nothing = Shows nothing on screen - it only plays its sound and speech.
overlays_display_replace = Stays on screen until the next update replaces it.
overlays_display_append = Each update is added to what is already on screen.
overlays_display_own_hide = The custom overlay.js hides the show on its own timer, so the duration and the hold for speech may not apply.
overlays_motion_entrance = Entrance
overlays_motion_text = Text
overlays_motion_on_screen = On screen
overlays_motion_exit = Exit
overlays_motion_total = Total show
overlays_motion_ms = { $value } ms
overlays_motion_secs = { $value } s
overlays_motion_unit_letter = by letter
overlays_motion_unit_word = by word
overlays_motion_text_by = { $effect } { $unit }
overlays_motion_text_staggered = { $effect } { $unit }, { $stagger } apart
overlays_motion_replay = Replay in browser
overlays_motion_field_entrance = Entrance
overlays_motion_field_entrance_ms = Entrance duration
overlays_motion_field_text_effect = Text effect
overlays_motion_field_text_effect_hint = Plays on the text during the entrance.
overlays_motion_field_text_unit = Text effect by
overlays_motion_field_stagger_custom = Custom text stagger
overlays_motion_field_stagger = Text stagger
overlays_motion_field_exit = Exit
overlays_motion_field_exit_ms = Exit duration
overlays_motion_field_exit_hint = Plays after the duration ends; the next show waits for it.
overlays_motion_field_intensity = Motion intensity
overlays_motion_opt_none = None
overlays_motion_opt_fade = Fade
overlays_motion_opt_slide_up = Slide up
overlays_motion_opt_slide_down = Slide down
overlays_motion_opt_slide_left = Slide left
overlays_motion_opt_slide_right = Slide right
overlays_motion_opt_pop = Pop
overlays_motion_opt_wipe = Wipe
overlays_motion_opt_sparks = Sparks
overlays_motion_opt_assemble = Assemble
overlays_motion_opt_glow_burst = Glow burst
overlays_motion_opt_dissolve = Dissolve
overlays_motion_opt_smoke = Smoke
overlays_motion_opt_shatter = Shatter
overlays_motion_opt_dust = Dust
overlays_motion_opt_burst = Burst
overlays_motion_opt_typewriter = Typewriter
overlays_motion_opt_fly_in = Fly in
overlays_motion_opt_spin = Spin
overlays_motion_opt_wave = Wave
overlays_motion_opt_bounce = Bounce
overlays_motion_opt_letter = Letter
overlays_motion_opt_word = Word
overlays_motion_opt_low = Low
overlays_motion_opt_medium = Medium
overlays_motion_opt_high = High
overlays_motion_reveal_target_missing = Your { $file } has no element with id "{ $target }" - this preset will not play.
overlays_motion_text_targets_missing = Your { $file } has no data-bind text - the text effect will not play.
overlays_motion_reveal_call_missing = Your { $file } never calls forge.show - presets will not play.
overlays_motion_retired_rules = Your { $file } still styles data-animation - those rules no longer apply.
overlays_queue_label = Show queue
overlays_queue_waiting = { $count } waiting
overlays_queue_clear = Clear queue
overlays_show_fire = Play full show
overlays_show_fire_hint = Queues a sample show with its sound and speech, the way an action would.
overlays_show_fire_hint_arrival = Sends sample content with its sound and speech, the way an action would.
overlays_show_fire_hint_latest = Shows sample values for 5 seconds, then the overlay returns to the current value.
overlays_latest_section = Current value
overlays_latest_from = from { $platform }
overlays_latest_empty = Nothing yet
overlays_latest_empty_hint = The overlay shows its placeholder until a value arrives.
overlays_latest_reset = Reset
overlays_latest_resetting = Resetting...
overlays_latest_reset_hint = Clears this slot for every platform. The overlay shows its placeholder until the next value.
overlays_latest_confirm_title = Reset the latest value?
overlays_latest_confirm_body = The stored value is removed for every platform and every overlay bound to this slot shows its placeholder. This cannot be undone.
latest_slot_donation = Last donation
latest_slot_now_playing = Now playing
latest_platform_all = All platforms
overlays_show_sending = Sending the show...
overlays_show_delivered = { $count ->
    [one] Show sent to { $count } browser source
   *[other] Show sent to { $count } browser sources
}
overlays_show_preview_only = { $count ->
    [one] Show sent, but only a preview tab is connected
   *[other] Show sent, but only { $count } preview tabs are connected
}
overlays_show_no_page = Show sent, but no page is connected - nothing is seen or heard.
overlays_show_ended = The show has ended.
overlays_show_cleared = The show was cleared from the queue before it played.
overlays_show_withdrawn = The show was withdrawn before it played.
overlays_receiver_toggle = Receives forge TTS and soundboard
overlays_receiver_hint_on = Speech and soundboard audio routed to an overlay play here. Turning it on elsewhere turns it off here.
overlays_receiver_hint_off = Turning it on sends forge speech and soundboard audio here and switches Settings - Audio - Routing to the overlay. Only one overlay receives it.
overlays_receiver_save_failed = The audio receiver could not be saved: { $reason }
overlays_panel_unavailable = This build has no overlay type for this record, so there is nothing to configure.
overlays_panel_choice_empty = No choices available.
overlays_sound_none = No sound
overlays_sound_custom_file = Custom file: { $name }
overlays_sound_adopting = Copying the sound into the library...
overlays_sound_issue_unknown_clip = The chosen sound is no longer in the soundboard, so this overlay plays nothing.
overlays_sound_issue_outside_library = The chosen sound has not been copied into the library yet, so this overlay plays nothing. Open the Soundboard and add it to the library.
overlays_sound_issue_bytes_missing = The library copy of the chosen sound is gone, so this overlay plays nothing.
overlays_sound_issue_kind_mismatch = The chosen sound is stored as { $format }, which is not audio, so this overlay plays nothing.
overlays_sound_issue_lookup_failed = forge could not read what this field points at: { $reason }
overlays_icon_issue_unknown_glyph = The chosen icon is not in this build's icon set, so this overlay shows no icon.
overlays_icon_issue_unknown_image = The chosen image is no longer in the library, so this overlay shows no icon.
overlays_icon_issue_bytes_missing = The library copy of the chosen image is gone, so this overlay shows no icon.
overlays_icon_issue_kind_mismatch = The chosen icon is stored as { $format }, which is not an image, so this overlay shows no icon.
overlays_icon_none = No icon
overlays_icon_none_desc = The alert shows text only.
overlays_icon_group_none = No icon
overlays_icon_group_imported = Imported
overlays_icon_import = Import image...
overlays_icon_import_desc = Pick a file from this machine ({ $formats }).
overlays_icon_import_not_image = { $file } is stored as { $format }, which is not an image, so it cannot be an icon.
overlays_icon_importing = Importing the image into the library...
overlays_icon_file_filter = Images
overlays_icon_picker_title = Choose an icon
overlays_icon_picker_subtitle = A curated glyph takes the overlay accent; an import is shown as it is.
overlays_icon_picker_hint = Type to search, arrows to move, Enter to choose, Esc to close.
overlays_icon_picker_search = Search icons
overlays_icon_category_support = Support
overlays_icon_category_celebration = Celebration
overlays_icon_category_community = Community
overlays_icon_category_stream = Stream
overlays_icon_category_play = Play
overlays_icon_category_goals = Goals
overlays_icon_category_nature = Nature
overlays_panel_override_notice = You own these files now, so they are never regenerated: { $files }. Design changes that need generated markup or styling no longer reach them.
config_form_choice_placeholder = Not set
config_form_auto_placeholder = auto
config_form_choice_any = Any
config_form_choice_search = Search or type a value...
config_form_choice_empty = Nothing to pick - type a value
config_form_choice_custom = use typed value
config_form_duration_unit_ms = milliseconds
config_form_duration_unit_seconds = seconds
config_form_duration_unit_minutes = minutes
config_form_duration_unit_hours = hours
config_form_duration_unit_days = days
field_label_allow_everyone_pings = Allow @everyone / @here
field_label_allow_role_pings = Allow role pings
field_label_ban_type_filter = Ban type
field_label_case_sensitive = Case sensitive
field_label_core__action_id = Action
field_label_core__add_amount = Amount (negative to subtract)
field_label_core__amount = Amount
field_label_core__array_source = Array variable
field_label_core__base = Base date-time (or %var%)
field_label_core__body = Body
field_label_core__captures_into_var = Captures variable (array)
field_label_core__cases = Cases
field_label_core__condition = Condition
field_label_core__content = Content
field_label_core__content_type = Content type
field_label_core__count = Count
field_label_core__create_parent_dirs = Create parent folders
field_label_core__custom_format_string = Custom date pattern (e.g. HH:mm dd.MM.yyyy, d MMMM yyyy, EEEE)
field_label_core__default_chain = Default
field_label_core__default_value = Default value
field_label_core__delay_amount = Run after
field_label_core__due_at = Date-time (or %var%)
field_label_core__due_at_into_var = Due time variable
field_label_core__else_chain = Else
field_label_core__encoding = Encoding
field_label_core__end_index = End index (-1 = to the end, exclusive)
field_label_core__expression = Expression
field_label_core__follow_redirects = Follow redirects
field_label_core__format = Date pattern (e.g. dd.MM.yyyy HH:mm, yyyy-MM-dd'T'HH:mm:ssxxx)
field_label_core__format_string = Date pattern (e.g. HH:mm dd.MM.yyyy, d MMMM yyyy, EEEE)
field_label_core__from = From (date-time or %var%)
field_label_core__headers = Headers (JSON object)
field_label_core__icon_path = Icon path
field_label_core__id_into_var = Scheduled run ID variable
field_label_core__ignore_missing = Ignore missing file
field_label_core__include_dirs = Include folders
field_label_core__inherit_args = Pass current arguments
field_label_core__into_arg = Output variable
field_label_core__into_var = Output variable
field_label_core__is_regex = Use regex
field_label_core__items = Items (one per line)
field_label_core__keep_current = Keep the running action
field_label_core__key = Variable name
field_label_core__late_tolerance_minutes = Skip when late by more than
field_label_core__level = Level
field_label_core__mark_as = Mark as
field_label_core__max = Maximum
field_label_core__max_iterations = Max iterations
field_label_core__max_length = Max length (0 = unlimited)
field_label_core__max_parts = Max parts (0 = unlimited)
field_label_core__message = Message
field_label_core__min = Minimum
field_label_core__mode = Mode
field_label_core__ms = Duration
field_label_core__name = Variable name
field_label_core__override_outputs = Synthetic outputs (JSON object)
field_label_core__parse_response_as = Parse response as
field_label_core__parts = Parts (one per line)
field_label_core__path = File path (relative to the files folder)
field_label_core__pattern = Glob pattern
field_label_core__persisted = Persist across restarts
field_label_core__platform = Platform or service (empty = most recent of all)
field_label_core__poll_interval_ms = Poll interval
field_label_core__probability_true = Probability of true (0..1)
field_label_core__query_params = Query parameters (JSON object)
field_label_core__queue_id = Queue
field_label_core__read_as = Read as
field_label_core__reason = Reason
field_label_core__recursive = Recursive
field_label_core__remove_all = Remove all matches
field_label_core__replace_with = Replace with
field_label_core__result_type = Result type
field_label_core__search = Search for
field_label_core__separator = Separator
field_label_core__slot = Slot
field_label_core__source = Source
field_label_core__start_index = Start index (0-based, characters)
field_label_core__target_var = Output variable
field_label_core__template = Template
field_label_core__text = Text
field_label_core__then_chain = Then
field_label_core__timeout_ms = Timeout
field_label_core__title = Title
field_label_core__to = To (date-time or %var%)
field_label_core__treat_undefined_as_false = Treat undefined as false
field_label_core__trigger_instance_id = Trigger
field_label_core__trim_each = Trim each part
field_label_core__unit = Unit
field_label_core__urgency = Urgency (Linux only)
field_label_core__url = URL
field_label_core__user_login = User login
field_label_core__utc = Use UTC instead of local time
field_label_core__value = Value
field_label_core__var_name = Variable name
field_label_core__weights = Weights (one per line, empty = uniform)
field_label_core__while_condition = While condition
field_label_core_action_cancel_scheduled__into_var = Output variable (bool)
field_label_core_action_cancel_scheduled__key = Key (exact match)
field_label_core_action_schedule__inherit_args = Pass current variables
field_label_core_action_schedule__key = Key (optional, replaces a pending run with the same key)
field_label_core_file_list__path = Folder path (relative to the files folder)
field_label_core_file_write__mode = Write mode
field_label_core_globals_array_append__value = Value to append
field_label_core_globals_array_remove__value = Value to remove
field_label_core_logic_loop__body = Body
field_label_core_notify_show__body = Body
field_label_core_string_regex_match__into_var = Output variable (bool)
field_label_core_string_regex_match__pattern = Pattern
field_label_core_test_fire_trigger__trigger_instance_id = Trigger instance
field_label_core_time_format__source = Source (date-time or %var%)
field_label_core_time_now__format = Format
field_label_core_time_parse__source = Source text
field_label_core_time_parse__utc = Read text without an offset as UTC instead of local time
field_label_donation__provider = Donation service
field_label_donations__amount = Amount
field_label_donations__currency = Currency
field_label_donations__donor_name = Donor name
field_label_donations__message = Message
field_label_donations__provider = Donation service
field_label_event_name = Event name
field_label_first_chatters_only = Only first-time chatters
field_label_hotkey__combo = Hotkey combo
field_label_ignore_bot_messages = Ignore the bot's messages
field_label_midi__channel = Channel
field_label_midi__controller = Controller
field_label_midi__device = Device
field_label_midi__direction = Direction
field_label_midi__note = Note
field_label_midi__program = Program
field_label_midi_send__channel = Channel (0-15)
field_label_midi_send__controller = Controller (CC)
field_label_midi_send__note = Note (NoteOn/NoteOff)
field_label_obs__scene = Scene
field_label_obs__source_name = Source name
field_label_phrase = Command phrase
field_label_reward_id = Reward
field_label_kick__reward_id = Reward ID
field_label_script__body = Script body
field_label_script__payload = Payload (JSON object)
field_label_script__script_name = Script
field_label_script__target_var = Output variable
field_label_timer__interval_minutes = Interval
field_label_timer__min_chat_messages = Min chat messages
field_label_timer__only_while_live = Only while live
field_label_twitch__color = Color
field_label_twitch__from_user = From user (login)
field_label_twitch__match_is_regex = Use regex
field_label_twitch__match_text = Match text
field_label_twitch__max_bits = Max bits (-1 = no limit)
field_label_twitch__min_amount_cents = Min donation (cents)
field_label_twitch__min_bits = Min bits
field_label_twitch__min_level = Min level (1-5)
field_label_twitch__reward_title = Reward title (if no reward chosen)
field_label_twitch__sender = Send as
field_label_twitch__state_filter = Guest state (empty = any)
field_label_twitch_automod_message_updated__status_filter = Decision status
field_label_twitch_channel_points_redemption__reward_id = Reward (empty = any)
field_label_twitch_channel_points_redemption_updated__status_filter = Redemption status
field_label_twitch__title = Title
field_label_twitch_poll_start__title = Question
field_label_twitch_prediction_start__title = Question
field_label_twitch__message = Message
field_label_twitch__target_user_login = Target Username
field_label_twitch__to_user_login = Recipient Username
field_label_twitch_guest_star_invite__target_user_login = Guest Login
field_label_twitch_guest_star_assign_slot__target_user_login = Guest Login
field_label_twitch_guest_star_remove_guest__target_user_login = Guest Login
field_label_twitch__session_id = Session ID
field_label_twitch__slot_id = Slot ID
field_label_twitch__message_id = Message ID
field_label_twitch__parent_message_id = Parent Message ID
field_label_twitch__prediction_id = Prediction ID
field_label_twitch__poll_id = Poll ID
field_label_twitch__redemption_id = Redemption ID
field_label_twitch__term_id = Blocked Term ID
field_label_twitch_automod_add_blocked_term__text = Term to block
field_label_twitch__duration_seconds = Duration
field_label_twitch__reason = Reason (optional, max 500 chars)
field_label_twitch_moderation_warn_user__reason = Reason (required, max 500 chars)
field_label_twitch__target = Target Platform
field_label_twitch__to_broadcaster_login = Target Channel
field_label_twitch__description = Description (optional)
field_label_twitch__category_id = Category ID
field_label_twitch__category_name = Category Name (display only)
field_label_twitch__tags = Tags (one per line or comma-separated)
field_label_twitch__choices = Choices (one per line or comma-separated, 2-5)
field_label_twitch__channel_points_voting_enabled = Enable Channel Points Voting
field_label_twitch__channel_points_per_vote = Channel Points Per Vote (0 = default; active only when voting enabled)
field_label_twitch__status = End Status
field_label_twitch__outcomes = Outcomes (one per line or comma-separated, 2-10)
field_label_twitch__prediction_window_seconds = Prediction Window
field_label_twitch__winning_outcome_id = Winning Outcome ID
field_label_twitch__cost = Cost (points)
field_label_twitch__prompt = Prompt
field_label_twitch_channel_points_create_reward__prompt = Prompt (optional)
field_label_twitch__background_color_hex = Background Color (#RRGGBB)
field_label_twitch_channel_points_create_reward__background_color_hex = Background Color (#RRGGBB or empty)
field_label_twitch__max_per_stream = Max Per Stream (0 = unlimited)
field_label_twitch__max_per_user_per_stream = Max Per User Per Stream (0 = unlimited)
field_label_twitch__global_cooldown_seconds = Global Cooldown (0 = disabled)
field_label_twitch__is_enabled = Enabled
field_label_twitch__requires_user_input = Require User Input
field_label_twitch__should_redemptions_skip_request_queue = Skip Redemption Queue
field_label_twitch__is_paused = Paused
field_label_twitch__audio_enabled = Audio Enabled
field_label_twitch__video_enabled = Video Enabled
field_label_twitch__volume = Volume (0-100)
field_label_twitch__emote_only = Emote-Only Mode
field_label_twitch__follower_mode = Follower-Only Mode
field_label_twitch__follower_mode_min_minutes = Follower Min. Duration
field_label_twitch__slow_mode = Slow Mode
field_label_twitch__slow_mode_wait_seconds = Slow Mode Wait
field_label_twitch__subscriber_mode = Subscriber-Only Mode
field_label_twitch__unique_chat_mode = Unique Chat Mode
field_label_twitch__overall_level = Overall Level (overrides individual categories)
field_label_twitch__aggression = Aggression
field_label_twitch__bullying = Bullying
field_label_twitch__disability = Disability
field_label_twitch__misogyny = Misogyny
field_label_twitch__race_ethnicity_or_religion = Race, Ethnicity, or Religion
field_label_twitch__sex_based_terms = Sex-Based Terms
field_label_twitch__sexuality_sex_or_gender = Sexuality, Sex, or Gender
field_label_twitch__swearing = Swearing
field_label_youtube__channel_id = Channel ID
field_label_youtube__message_id = Message ID
field_label_youtube__message = Message
field_label_youtube__title = Title
field_label_youtube__description = Description
field_label_youtube__category_id = Category ID
field_label_youtube__image_path = Image Path
field_label_youtube__duration_seconds = Duration
field_label_youtube__duration_secs = Length
field_label_youtube__privacy_status = Privacy
field_label_youtube__identifier = Channel Handle or ID
field_label_youtube__question = Question
field_label_youtube__options = Options (one per line, 2-4)
field_label_kick__user_id = Target User ID
field_label_kick__reason = Reason (optional, max 100 chars)
field_label_kick_reward_create__title = Reward Title
field_label_kick_reward_update__title = New Title (optional)
field_label_kick_channel_update_info__title = Stream Title
field_label_kick_reward_create__cost = Cost (channel points)
field_label_kick_reward_update__cost = New Cost (optional)
field_label_kick_reward_create__description = Description (optional)
field_label_kick_reward_update__description = New Description (optional)
field_label_kick__category_id = Category ID
field_label_kick__tags = Tags (comma-separated, max 10)
field_label_kick__message_id = Message ID
field_label_kick__message = Message
field_label_kick__as_bot = Send as bot
field_label_kick__redemption_ids = Redemption ID(s)
field_label_kick__query = Search Query
field_label_kick__slug = Channel Slug
field_label_kick__duration_minutes = Duration
field_label_obs__caption_text = Caption Text
field_label_obs__on = Active
field_label_obs_studio_set_enabled__on = Enabled
field_label_vtube__model_id = Model
field_label_vtube_item_pin__model_id = Model ID
field_label_soundboard__clip_id = Clip
field_label_vtube__color_r = Red
field_label_vtube__color_g = Green
field_label_vtube__color_b = Blue
field_label_vtube__color_a = Alpha
field_label_vtube__mix_with_scene_lighting = Mix With Scene Lighting
field_label_obs__source = Source
field_label_obs_media_restart__source = Media Input
field_label_obs_browser_refresh__source = Browser Source
field_label_obs_audio_set_volume__source = Audio Input
field_label_obs_audio_set_mute__source = Input
field_label_obs_sources_set_input_settings__source = Input Source
field_label_obs_sources_get_input_settings__source = Input name
field_label_discord__webhook_name = Webhook
field_label_discord__content = Caption
field_label_discord_webhook_update_message__content = New Content
field_label_discord_webhook_send_message__content = Message
field_label_discord__file_path = File Path
field_label_vtube__x = X Position
field_label_vtube__y = Y Position
field_label_vtube__rotation = Rotation
field_label_vtube__size = Size
field_label_vtube__duration = Duration (s)
field_label_vtube_item_throw__duration = Throw Duration (s)
field_label_discord__message_id = Message ID
field_label_tts__alias_name = Alias name
field_label_tts__engine_id = Engine ID
field_label_tts__voice_id = Voice ID
field_label_discord__embed_title = Title
field_label_discord__embed_description = Description
field_label_obs__transition = Transition Name
field_label_obs__name = Collection Name
field_label_obs_profile_switch__name = Profile Name
field_label_obs__volume_db = Volume (dB)
field_label_vtube__hotkey_id = Hotkey
field_label_tts__text = Text
field_label_tts__voice = Voice
field_label_tts__wait_for_completion = Wait for completion
field_label_obs__muted = Muted
field_label_midi__port = Output Port
field_label_midi__message_kind = Message Kind
field_label_midi__velocity = Velocity (NoteOn/NoteOff)
field_label_midi__value = Value (CC)
field_label_obs__settings_json = Settings (JSON)
field_label_obs__overlay = Overlay (merge with existing settings)
field_label_tts__keep_current = Let current item finish
field_label_vtube__param_id = Parameter ID
field_label_vtube__value = Value
field_label_vtube__hold_secs = Hold (seconds, 0 = one-shot)
field_label_obs__visible = Visible
field_label_vtube__enabled = Physics Enabled
field_label_vtube__item_instance_id = Item Instance ID
field_label_vtube__pin = Pin (off to unpin)
field_label_vtube__vertex_pin_type = Pin Position
field_label_vtube__angle_relative_to = Angle Relative To
field_label_vtube__size_relative_to = Size Relative To
field_label_vtube__art_mesh_id = Art Mesh ID
field_label_vtube__angle = Angle
field_label_soundboard__volume_db = Volume (dB)
field_label_obs__path = Directory
field_label_obs_capture_screenshot__path = Save To
field_label_vtube__order = Order
field_label_vtube__fade_mode = Fade Mode
field_label_obs__locked = Locked
field_label_vtube__file_name = Item File Name
field_label_vtube__fade_time = Fade Time (s)
field_label_vtube__unload_on_disconnect = Unload When Plugin Disconnects
field_label_server__payload = Payload (JSON object)
field_label_server__client_filter = Client IDs (JSON array, empty = all)
field_label_server__client_type_filter = Client Types (JSON array: overlay/dashboard/remote)
field_label_overlay__overlay_id = Overlay
field_label_overlay__duration_secs = Override duration
field_label_overlay__wait_for_show = Wait until the show ends
field_label_obs__filter = Filter Name
field_label_obs__enabled = Enabled
field_label_vtube__expression_file = Expression
field_label_vtube__active = Active
field_label_vtube__from_x = From X
field_label_vtube__from_y = From Y
field_label_vtube__to_x = To X
field_label_vtube__to_y = To Y
field_optional_core__skip_if_late = Skip instead of running late if forge was closed
field_optional_core__use_due_at = Run at a date-time instead
field_optional_donation__provider = Filter by donation service
field_optional_midi__channel = Filter by channel
field_optional_midi__controller = Filter by controller
field_optional_midi__device = Filter by device
field_optional_midi__direction = Filter by direction
field_optional_midi__note = Filter by note
field_optional_midi__program = Filter by program
field_optional_obs__scene = Filter by scene
field_optional_obs__source_name = Filter by source
field_optional_twitch__title = Title
field_optional_twitch__cost = Cost (points)
field_optional_twitch__prompt = Prompt
field_optional_twitch__background_color_hex = Background Color
field_optional_twitch__max_per_stream = Max Per Stream
field_optional_twitch__max_per_user_per_stream = Max Per User Per Stream
field_optional_twitch__global_cooldown_seconds = Global Cooldown Seconds
field_optional_twitch__volume = Volume (0-100)
field_optional_vtube__mix_with_scene_lighting = Mix With Scene Lighting
field_optional_vtube__x = X Position
field_optional_vtube__y = Y Position
field_optional_vtube__rotation = Rotation
field_optional_vtube__size = Size
field_optional_vtube__duration = Duration (s)
field_optional_vtube_item_throw__duration = Throw Duration (s)
field_optional_tts__voice = Voice (leave empty for the default voice)
field_optional_vtube__model_id = Model ID
field_optional_vtube__art_mesh_id = Art Mesh ID
field_optional_vtube__angle = Angle
field_optional_vtube__order = Order
field_optional_vtube__fade_time = Fade Time (s)
field_optional_overlay__duration_secs = Override duration
field_optional_vtube__from_x = From X
field_optional_vtube__from_y = From Y
field_optional_vtube__to_x = To X
field_optional_vtube__to_y = To Y
field_placeholder_core_args_set__value = hello
field_placeholder_core_logic_stop__reason = why the chain stopped
field_placeholder_core_notify_show__title = Stream starting
field_placeholder_core_string_length__source = Hello
field_placeholder_core_string_lowercase__source = Hello World
field_placeholder_core_string_regex_match__source = Hello World
field_placeholder_core_string_replace__search = World
field_placeholder_core_string_replace__source = Hello World
field_placeholder_core_string_substring__source = Hello World
field_placeholder_core_string_titlecase__source = hello world
field_placeholder_core_string_uppercase__source = Hello World
field_placeholder_twitch_guest_star_guest_updated__state_filter = e.g. live
field_placeholder_twitch_chat_whisper__from_user = any
field_placeholder_twitch_chat_whisper__match_text = leave empty to match all
field_placeholder_twitch_poll_start__title = What should we play next?
field_placeholder_twitch_automod_add_blocked_term__text = word or phrase (2-500 characters)
field_placeholder_twitch_channel_update_category__category_id = e.g. 509658
field_placeholder_twitch_channel_update_category__category_name = e.g. Just Chatting
field_placeholder_twitch_channel_points_create_reward__title = My Reward
field_placeholder_twitch_channel_points_update_reward__title = My Reward
field_placeholder_twitch_channel_points_create_reward__prompt = Enter your message
field_placeholder_twitch_channel_points_update_reward__prompt = Enter your message
field_placeholder_twitch_prediction_start__title = Will I win this game?
field_placeholder_youtube_chat_create_poll__question = What next?
field_placeholder_youtube_stream_update_title__title = New stream title
field_placeholder_youtube_stream_update_category__category_id = 20 (Gaming), 24 (Entertainment)...
field_placeholder_youtube_lookup_viewer__identifier = @handle or UC...
field_placeholder_youtube_moderation_add_moderator__channel_id = UC... or %user_id%
field_placeholder_youtube_moderation_ban_user__channel_id = UC... or %user_id%
field_placeholder_youtube_moderation_remove_moderator__channel_id = UC... or %user_id%
field_placeholder_youtube_moderation_timeout_user__channel_id = UC... or %user_id%
field_placeholder_youtube_moderation_unban_user__channel_id = UC... or %user_id%
field_placeholder_kick_reward_create__title = e.g. Hydrate
field_placeholder_kick_reward_create__cost = e.g. 500
field_placeholder_kick_reward_create__description = Leave empty to omit
field_placeholder_kick_reward_update__title = Leave empty to keep current
field_placeholder_kick_reward_update__cost = Leave empty to keep current
field_placeholder_kick_reward_update__description = Leave empty to keep current
field_placeholder_kick_channel_update_info__title = Leave empty to keep current
field_placeholder_kick_channel_update_info__category_id = Leave empty to keep current
field_placeholder_kick_channel_update_info__tags = Leave empty to keep current
field_placeholder_kick_reward_redemption_accept__redemption_ids = %redemption_id% or id1,id2,id3 (max 25)
field_placeholder_kick_reward_redemption_reject__redemption_ids = %redemption_id% or id1,id2,id3 (max 25)
field_placeholder_obs_stream_send_caption__caption_text = Caption text to send over stream
field_placeholder_vtube_model_tint__mix_with_scene_lighting = 0.0 to 1.0
field_placeholder_vtube_model_move__x = -1.0 to 1.0
field_placeholder_vtube_model_move__y = -1.0 to 1.0
field_placeholder_vtube_model_move__rotation = -360 to 360
field_placeholder_vtube_model_move__size = -100 to 100
field_placeholder_tts_alias_set__alias_name = e.g. %user% or viewer id
field_placeholder_discord_webhook_send_embed__embed_title = Stream is live
field_placeholder_obs_scenes_set_transition__transition = e.g. Fade
field_placeholder_obs_scene_collection_switch__name = e.g. Main Collection
field_placeholder_obs_audio_set_volume__volume_db = e.g. -6.0
field_placeholder_obs_sources_set_input_settings__source = e.g. Webcam
field_placeholder_vtube_model_set_physics__duration = 0.5 to 5
field_placeholder_vtube_item_pin__item_instance_id = item instance id from VTS
field_placeholder_vtube_item_pin__model_id = empty = current model
field_placeholder_vtube_item_pin__art_mesh_id = empty = random art mesh
field_placeholder_vtube_item_pin__size = 0.0 to 1.0
field_placeholder_obs_profile_switch__name = e.g. Streaming
field_placeholder_vtube_item_move__item_instance_id = item instance id from VTS
field_placeholder_vtube_item_move__x = -1.0 to 1.0
field_placeholder_vtube_item_move__y = -1.0 to 1.0
field_placeholder_vtube_item_move__size = 0.0 to 1.0
field_placeholder_vtube_item_move__rotation = -360 to 360
field_placeholder_tts_alias_switch_active__alias_name = e.g. %user% or viewer id
field_placeholder_vtube_item_load__x = -1.0 to 1.0
field_placeholder_vtube_item_load__y = -1.0 to 1.0
field_placeholder_vtube_item_load__size = 0.0 to 1.0
field_placeholder_vtube_item_load__rotation = -360 to 360
field_placeholder_obs_sources_get_input_settings__source = e.g. Webcam
field_placeholder_obs_filter_set_enabled__filter = e.g. Color Correction
field_placeholder_vtube_item_throw__from_x = -1.0 to 1.0
field_placeholder_vtube_item_throw__from_y = -1.0 to 1.0
field_placeholder_vtube_item_throw__to_x = -1.0 to 1.0
field_placeholder_vtube_item_throw__to_y = -1.0 to 1.0
field_placeholder_vtube_item_throw__size = 0.0 to 1.0

## Overlays - event wiring

overlays_wire_open = Show on event...
overlays_wire_empty = Nothing sends to this overlay yet.
overlays_wire_picker_title = Show this overlay on an event
overlays_wire_picker_lead = for
overlays_wire_picker_count = { $count ->
    [one] { $count } event
   *[other] { $count } events
}
overlays_wire_picker_search = Search events
overlays_wire_picker_hint = Type to search - arrows to move - Enter to pick
overlays_wire_scope_suggested = Suggested
overlays_wire_confirm_title = Show { $overlay } on this event
overlays_wire_checking = Checking what already sends to this overlay...
overlays_wire_section_creates = Creates
overlays_wire_section_wording = On stream it reads
overlays_wire_record_action = Action
overlays_wire_record_trigger = Trigger
overlays_wire_record_queue = Queue
overlays_wire_wording_empty = This overlay type has no wording to fill in.
overlays_wire_create = Create
overlays_wire_back = Back
overlays_wire_open_action = Open action
overlays_wire_already_body = This overlay is already shown on that event. Open the action instead of creating a second one.
overlays_wire_refused_overlay = This overlay type does not take event wiring.
overlays_wire_refused_trigger = This event declares nothing an overlay could show.
overlays_wire_toast_created = Overlay wired to the event
overlays_wire_toast_already = This overlay is already shown on that event.
overlays_wire_toast_incomplete = Wiring stopped part-way. Written so far: { $landed }.
overlays_wire_toast_missing = This build no longer has that overlay type or that event.
overlays_wire_landed_action = the action
overlays_wire_landed_trigger = the trigger
overlays_wire_landed_link = the link between them
overlays_wire_landed_nothing = nothing
overlays_wire_queue_not_live = The queue { $queue } is not running yet. Restart forge so this alert plays.
overlays_wire_fires_on = Fires on { $triggers }
overlays_wire_fires_untriggered = Actions send to this overlay, but no event drives them yet.
overlays_wire_action_disabled = disabled
overlays_wire_overlay_off = This overlay is off, so nothing is displayed while it stays off.
overlays_confirm_delete_feeds = { $count ->
    [one] { $count } action sends to it.
   *[other] { $count } actions send to it.
}
nav_item_integrations = Integrations
integrations_breadcrumb = Integrations
integrations_title = Integrations
integrations_subtitle = Turn on what you use. A disabled integration does not start at all - no connections, no background work. Sign-ins and settings are kept.
integration_enabled_of_total = {" "}of { $total } enabled
integration_category_streaming = Streaming
integration_category_streaming_blurb = Chat and events from the platforms you stream on
integration_category_donations = Donations
integration_category_donations_blurb = Support from your viewers through donation services
integration_category_apps = Apps
integration_category_apps_blurb = Desktop apps Forge controls over a local WebSocket
integration_category_community = Community
integration_category_community_blurb = Post to your community outside the stream
integration_category_audio = Audio
integration_category_audio_blurb = Speech and sound, built into Forge
integration_category_controls = Controls
integration_category_controls_blurb = Physical and system inputs that fire triggers
integration_category_tools = Tools
integration_category_tools_blurb = Browser sources and the local API, built into Forge
integration_category_on_count = { $on } of { $total } on
integration_twitch_description = Chat, EventSub, channel points, bits, raids
integration_youtube_description = Live chat, super chats, memberships
integration_kick_description = Chat, subs, channel events
integration_donatello_description = Donations through the Donatello panel API
integration_monobank_description = Top-ups of a monobank jar
integration_obs_description = Scenes, sources, audio, recording
integration_vtube_description = Model hotkeys, expressions, items
integration_discord_description = Go-live posts and event mirrors via webhooks
integration_midi_description = Notes, CC and faders from controllers
integration_hotkey_description = OS-wide shortcuts, even when Forge is hidden
integration_core_tts_description = Voices, aliases, speak queue
integration_core_soundboard_description = Clips from pads, hotkeys, actions
integration_core_overlays_description = Browser sources for alerts and widgets
integration_core_server_description = Local API for overlays and tools
integration_core_open = Open
integration_core_always_on = Always on - part of Forge
integration_status_disabled = Disabled
integration_status_starting = Starting
integration_status_stopping = Stopping
integration_status_failed = Failed to start
integration_status_active = Active
integration_status_connected = Connected
integration_status_not_connected = Not connected
integration_status_built_in = Built in
integration_refs_none = Not used yet
integration_refs_actions = { $count ->
    [one] { $count } action
   *[other] { $count } actions
}
integration_refs_triggers = { $count ->
    [one] { $count } trigger
   *[other] { $count } triggers
}
integration_refs_used_by = used by { $parts }
integration_retry = Retry
integration_retry_failed = Could not restart { $name }
integration_connect_failed = Could not connect { $name }
integration_disconnect_failed = Could not disconnect { $name }
integration_failed_title = { $name } could not start
integration_failed_lead = Retry starts it again. Sign-in and settings are kept.
integration_failed_retrying = Starting again
integration_failed_sign_in = Sign in again
integration_connect = Connect
integration_disconnect = Disconnect
integration_settings_tooltip = { $name } settings
integration_kick_disclaimer_short = Chat via unofficial WebSocket - may break
integration_kick_disclaimer = Chat is read through Kick's unofficial WebSocket and may break without notice.
integration_activity_discord = { $count ->
    [one] { $count } webhook ready
   *[other] { $count } webhooks ready
}
integration_activity_midi = { $count ->
    [one] { $count } device
   *[other] { $count } devices
}
integration_activity_hotkey = { $count ->
    [one] { $count } binding
   *[other] { $count } bindings
}
integration_disable_title = Disable { $name }?
integration_disable_subtitle = Stops it now. Sign-in and settings are kept.
integration_disable_lead_prefix = { $name } is used by{" "}
integration_disable_lead_and = {" "}and{" "}
integration_disable_lead_suffix = . While it is disabled:
integration_disable_fails_prefix = { $name } steps fail with{" "}
integration_disable_reason = integration disabled
integration_disable_fails_suffix = {" "}in run history
integration_disable_triggers_inactive = Its triggers are inactive and will not fire
integration_disable_kept = Your sign-in and settings stay - turn it back on any time
integration_disable_used_by = Used by
integration_disable_more = +{ $count } more
integration_disable_confirm = Disable { $name }
integration_disable_esc_hint = Esc to cancel
integration_welcome_title = Welcome to Forge
integration_welcome_lead = Turn on what you stream with. Everything you leave off stays off - no connections, no background work.
integration_welcome_lead_later = You can change this any time in Integrations.
integration_welcome_nothing_on = Nothing turned on yet
integration_welcome_some_on = { $count } on - { $names }
integration_welcome_progress = { $on } / { $total }
integration_welcome_built_in = Text-to-Speech, Soundboard, Overlays and the WebSocket server are built in - always available.
integration_welcome_off = Off - turn on to set up
integration_welcome_on = On
integration_welcome_skip = Skip for now
integration_welcome_enter = Enter Forge
integration_welcome_turn_one_on = Turn one on to continue
integration_disabled_title = { $name } is disabled
integration_disabled_lead = It is not running - no connection, no background work. Sign-in and settings are kept.
integration_disabled_all = All integrations
integration_disabled_enable = Enable { $name }
integration_disabled_affected = Affected while disabled
integration_disabled_unused = Nothing uses { $name } yet.
integration_disabled_steps_fail = { $name } steps will fail
integration_disabled_triggers_idle = { $count ->
    [one] { $count } trigger will not fire
   *[other] { $count } triggers will not fire
}
integration_disabled_kept = Kept for when you turn it back on
integration_disabled_nothing_kept = Nothing set up yet.
integration_kept_sign_in = Sign-in saved
integration_kept_credentials = Credentials saved
integration_inactive_short = inactive
integration_inactive_badge = inactive · { $name } disabled
integration_inactive_category_badge = inactive · no { $category } integration enabled
donation_anonymous_donor = Anonymous
donation_settings_title = { $service } token
donation_token_label = API token
donation_token_placeholder = Paste the token here
donation_token_status_saved = Token saved
donation_token_status_missing = No token saved
donation_token_status_unknown = Checking stored token
donation_token_hint_new = Copy the token from your { $service } account and paste it here.
donation_token_hint_replace = A token is saved. Paste a new one to replace it.
donation_token_page_link = Open token page
donation_token_encrypted_note = Stored encrypted on this computer
donation_token_check = Check token
donation_token_checking = Checking...
donation_token_save = Save token
donation_token_saving = Saving...
donation_token_remove = Remove token
donation_token_remove_title = Remove the token?
donation_token_remove_body = Donations stop arriving until you paste a token again.
donation_token_remove_body_monobank = Donations stop arriving and the jar choice is cleared until you paste a token again.
donation_token_works = The token works.
donation_token_works_as = The token works. Account: { $name }
donation_token_saved = Token saved. Polling for donations.
donation_token_saved_as = Token saved for { $name }. Polling for donations.
donation_token_removed = Token removed.
donation_jar_label = Jar
donation_jar_hint = Load your jars, then pick the one that receives donations.
donation_jar_loading = Loading jars...
donation_jar_load = Load jars
donation_jar_loading_button = Loading...
donation_jar_save = Use this jar
donation_jar_saved = Saved. Watching the jar { $jar }.
donation_jar_none = This account has no jars. Create a jar in the monobank app first.
donation_jar_untitled = Untitled jar
donation_jar_goal = goal { $goal }
donation_poll_label = Check for new donations every
donation_poll_hint = forge asks Donatello for new donations every { $seconds } s.
donation_poll_seconds = { $seconds } s
donation_poll_minutes = { $minutes } min
donation_token_problem_missing = Paste a token first.
donation_token_problem_malformed = This does not look like a token. Copy it again without extra characters.
donation_token_problem_rejected = { $service } rejected the token. Create a new one and paste it here.
donation_token_problem_profile_incomplete = Your Donatello profile setup is not finished. Complete it in the Donatello panel, then try again.
donation_token_problem_no_jar = Pick a jar first.
donation_token_problem_jar_not_found = This jar is not among the jars of this token. Load the jars again.
donation_token_problem_cooling_down = { $service } allows one request per minute. Try again in { $seconds } s.
donation_token_problem_cooling_down_soon = { $service } asked to slow down. Try again in a minute.
donation_token_problem_unreachable = Could not reach { $service }. Check the internet connection.
donation_token_problem_service_error = { $service } answered with an error (HTTP { $status }). Try again later.
donation_token_problem_unreadable = { $service } sent an answer forge could not read. Try again later.
donation_token_problem_storage = The token could not be stored on this computer.
donation_token_problem_local = Something went wrong inside forge. Restart it and try again.

## Shell - leaving a screen with unsaved changes
shell_unsaved_leave_title = Leave with unsaved changes?
shell_unsaved_leave_body = This screen has edits that were never saved. Save them before leaving, discard them, or stay to keep editing.
shell_unsaved_leave_cancel = Keep editing
shell_unsaved_leave_save = Save and leave
shell_unsaved_leave_discard = Discard

twitch_bot_section_title = Bot account
twitch_bot_header_signing_in = signing in
twitch_bot_header_reauth = sign-in needed
twitch_bot_intro = Send chat as a second Twitch account. Events, moderation and rewards stay on your streamer account.
twitch_bot_connect_btn = Connect bot account
twitch_bot_private_window_hint = Open the link in a private browser window signed in as the bot account, then enter the code. Do not use a window where your streamer account is signed in.
twitch_bot_activation_link = Activation link
twitch_bot_activation_code = Code
twitch_bot_requesting = Requesting a sign-in code...
twitch_bot_waiting = Waiting for the bot account to authorize...
twitch_bot_try_again_btn = Try again
twitch_bot_disconnect_btn = Disconnect
twitch_bot_disconnect_failed = Could not disconnect the bot: { $error }
twitch_bot_reauth_detail = The bot account needs to sign in again.
twitch_bot_fallback_active = Default messages are going out as @{ $login } until then.
twitch_bot_reauth_btn = Sign in again
twitch_bot_not_moderator_note = The bot is not a moderator of your channel. Announcements and shoutouts need it, and non-moderators are limited to 20 messages per 30 seconds.
twitch_bot_make_moderator_btn = Make the bot a moderator
twitch_bot_sent_as_label = Sent as
twitch_bot_sent_as_role_bot = bot account
twitch_bot_sent_as_role_streamer = streamer account
twitch_bot_error_same_account = This is your streamer account. Sign in as the bot account in a private window.
twitch_bot_error_denied = The bot sign-in was denied on Twitch.
twitch_bot_error_expired = The sign-in code expired. Try again for a new code.
twitch_bot_error_failed = Bot sign-in failed: { $error }
twitch_bot_mod_error_not_connected = No bot account is signed in.
twitch_bot_mod_error_streamer_reauth = Sign in to your streamer account again to allow moderator changes.
twitch_bot_mod_error_banned = The bot is banned in your channel. Unban it first.
twitch_bot_mod_error_vip = The bot is a VIP in your channel. Remove its VIP status first.
twitch_bot_mod_error_refused = Twitch refused: { $reason }
twitch_bot_mod_error_unavailable = Could not change moderators: { $reason }
chat_send_as_me = Me
chat_send_as_bot = Bot
sender_option_default = Default (bot when connected)
sender_option_streamer = Streamer
sender_option_bot = Bot only
sender_field_twitch_only_helper = Twitch only; ignored on other platforms
event_feed_sent_as = sent as @{ $login }
announcement_color_primary = Primary (channel color)
announcement_color_blue = Blue
announcement_color_green = Green
announcement_color_orange = Orange
announcement_color_purple = Purple

step_option_unchanged = Leave unchanged
step_option_on = On
step_option_off = Off
step_option_level_0 = No filtering (0)
step_option_level_1 = Less filtering (1)
step_option_level_2 = Some filtering (2)
step_option_level_3 = More filtering (3)
step_option_level_4 = Maximum filtering (4)
step_option_poll_terminated = End, keep results visible
step_option_poll_archived = End and archive (hide results)
step_option_ease_linear = Linear
step_option_ease_in = Ease in
step_option_ease_out = Ease out
step_option_ease_both = Ease in and out
step_option_ease_overshoot = Overshoot
step_option_ease_zip = Zip
step_option_pin_center = Item center
step_option_pin_random = Random point
step_option_rel_world = World
step_option_rel_item_rotation = Current item rotation
step_option_rel_model = Model
step_option_rel_pin_position = Pin position
step_option_rel_item_size = Current item size
step_option_midi_note_on = Note on
step_option_midi_note_off = Note off
step_option_midi_cc = Control change (CC)
step_option_midi_raw = Raw bytes
step_option_direction_input = Input
step_option_direction_output = Output
step_option_mark_completed = Completed
step_option_mark_failed = Failed
step_option_log_trace = Trace
step_option_log_debug = Debug
step_option_log_info = Info
step_option_log_warn = Warning
step_option_log_error = Error
step_option_len_chars = Characters
step_option_len_bytes = Bytes
step_option_trim_both = Both ends
step_option_trim_left = Left side
step_option_trim_right = Right side
step_option_loop_count = Fixed count
step_option_loop_foreach = For each array item
step_option_loop_while = While the condition holds
step_option_write_overwrite = Overwrite
step_option_write_append = Append
step_option_write_create_new = Create new only
step_option_result_auto = Auto
step_option_result_int = Integer
step_option_result_float = Decimal
step_option_urgency_low = Low
step_option_urgency_normal = Normal
step_option_urgency_critical = Critical
step_option_parse_text = Text
step_option_parse_json = JSON
step_option_parse_ignore = Ignore
step_option_unit_seconds = Seconds
step_option_unit_minutes = Minutes
step_option_unit_hours = Hours
step_option_unit_days = Days
step_option_unit_months = Months
step_option_unit_years = Years
step_option_enc_utf8 = UTF-8
step_option_enc_latin1 = Latin-1
step_option_enc_raw_base64 = Raw (base64)
step_option_read_lines = Lines array
step_option_read_whole = Whole file
step_option_read_json = JSON
step_option_fmt_iso8601 = ISO 8601
step_option_fmt_unix_seconds = Unix seconds
step_option_fmt_unix_millis = Unix milliseconds
step_option_fmt_custom = Custom format
step_option_privacy_public = Public
step_option_privacy_unlisted = Unlisted
step_option_privacy_private = Private
step_option_ban_any = Any
step_option_ban_permanent = Permanent
step_option_ban_temporary = Temporary
step_option_status_any = Any
step_option_status_fulfilled = Fulfilled
step_option_status_canceled = Canceled
step_option_status_approved = Approved
step_option_status_denied = Denied
step_option_status_expired = Expired
step_option_position_top = Top
step_option_position_bottom = Bottom
step_option_seconds_value = { $seconds } s

## Step and trigger names and descriptions
kind_name_core_action_cancel = Cancel Action
kind_summary_core_action_cancel = Stop in-flight runs of an action (or all running actions)
kind_name_core_action_cancel_scheduled = Cancel Scheduled Action
kind_summary_core_action_cancel_scheduled = Cancel the pending scheduled run with this exact key
kind_name_core_action_disable = Disable Action
kind_summary_core_action_disable = Prevent an action from running when triggered
kind_name_core_action_enable = Enable Action
kind_summary_core_action_enable = Allow an action to run when triggered
kind_name_core_action_run = Run Action
kind_summary_core_action_run = Run another action's steps from inside this chain
kind_name_core_action_schedule = Schedule Action
kind_summary_core_action_schedule = Run an action later, even after a restart
kind_name_core_action_toggle = Toggle Action
kind_summary_core_action_toggle = Flip an action between enabled and disabled
kind_name_core_args_set = Set Local Arg
kind_summary_core_args_set = Set a variable in the current execution's ArgStack
kind_name_core_clipboard_copy = Copy to Clipboard
kind_summary_core_clipboard_copy = Copy text to the system clipboard
kind_name_core_clipboard_read = Read Clipboard
kind_summary_core_clipboard_read = Read the system clipboard into a variable
kind_name_core_file_delete = Delete File
kind_summary_core_file_delete = Remove a sandboxed file; never removes directories
kind_name_core_file_list = List Directory
kind_summary_core_file_list = Enumerate a sandboxed directory; stores entry names in a variable array
kind_name_core_file_read = Read File
kind_summary_core_file_read = Read a text file into a variable
kind_name_core_file_write = Write File
kind_summary_core_file_write = Write content to a sandboxed file; stores bytes written in `file.bytes_written`
kind_name_core_globals_array_append = Append to Global Array
kind_summary_core_globals_array_append = Append a value to a global array variable
kind_name_core_globals_array_remove = Remove from Global Array
kind_summary_core_globals_array_remove = Remove matching items from a global array variable
kind_name_core_globals_decrement = Decrement Global
kind_summary_core_globals_decrement = Decrement a numeric global variable by an amount
kind_name_core_globals_delete = Delete Global
kind_summary_core_globals_delete = Remove a global variable
kind_name_core_globals_get = Get Global
kind_summary_core_globals_get = Read a global variable into an argument
kind_name_core_globals_increment = Increment Global
kind_summary_core_globals_increment = Increment a numeric global variable by an amount
kind_name_core_globals_set = Set Global
kind_summary_core_globals_set = Set a global variable to a value
kind_name_core_globals_toggle = Toggle Global
kind_summary_core_globals_toggle = Flip a boolean global variable between true and false
kind_name_core_http_delete = HTTP DELETE
kind_summary_core_http_delete = Send a DELETE request; stores response in `http.*`
kind_name_core_http_get = HTTP GET
kind_summary_core_http_get = Fetch a URL; stores status, body and headers in `http.*`
kind_name_core_http_patch = HTTP PATCH
kind_summary_core_http_patch = Send a PATCH request with a body; stores response in `http.*`
kind_name_core_http_post = HTTP POST
kind_summary_core_http_post = Send a POST request with a body; stores response in `http.*`
kind_name_core_http_put = HTTP PUT
kind_summary_core_http_put = Send a PUT request with a body; stores response in `http.*`
kind_name_core_latest_get = Get Latest Value
kind_summary_core_latest_get = Read the latest recorded value of a widget slot (such as the last donation) into an argument
kind_name_core_log_write = Write Log
kind_summary_core_log_write = Emit a log message at a chosen severity level
kind_name_core_logic_break_loop = Break Loop
kind_summary_core_logic_break_loop = Exit the innermost enclosing loop
kind_name_core_logic_continue_loop = Continue Loop
kind_summary_core_logic_continue_loop = Skip to the next iteration of the innermost enclosing loop
kind_name_core_logic_if_then_else = If / Then / Else
kind_summary_core_logic_if_then_else = Run one of two sub-chains depending on a condition
kind_name_core_logic_loop = Loop
kind_summary_core_logic_loop = Repeat a sub-chain a fixed count, over an array, or while a condition holds
kind_name_core_logic_stop = Stop Action
kind_summary_core_logic_stop = Halt the current action chain immediately
kind_name_core_logic_switch_case = Switch / Case
kind_summary_core_logic_switch_case = Run the sub-chain whose case matches an expression
kind_name_core_logic_wait = Wait
kind_summary_core_logic_wait = Pause execution for a number of milliseconds (capped at 60s)
kind_name_core_logic_wait_until = Wait Until
kind_summary_core_logic_wait_until = Poll a condition until it holds or a timeout elapses
kind_name_core_math_evaluate = Math Expression
kind_summary_core_math_evaluate = Evaluate an arithmetic expression and store the result as an argument
kind_name_core_notify_show = Show Desktop Notification
kind_summary_core_notify_show = Show an OS-level notification
kind_name_core_queue_clear = Clear Queue
kind_summary_core_queue_clear = Drop a queue's pending actions, optionally stopping the running one
kind_name_core_queue_pause = Pause Queue
kind_summary_core_queue_pause = Stop a queue from starting new actions until resumed
kind_name_core_queue_resume = Resume Queue
kind_summary_core_queue_resume = Let a paused queue start running actions again
kind_name_core_random_bool = Random Bool (Weighted)
kind_summary_core_random_bool = Generate a weighted random boolean and store it in a variable
kind_name_core_random_float = Random Float
kind_summary_core_random_float = Generate a random float in [min, max] and store it in a variable
kind_name_core_random_int = Random Integer
kind_summary_core_random_int = Generate a random integer in [min, max] and store it in a variable
kind_name_core_random_pick = Random Pick
kind_summary_core_random_pick = Pick a random element from a list and store it in a variable
kind_name_core_string_concat = String - Concat
kind_summary_core_string_concat = Join a list of strings with an optional separator
kind_name_core_string_format = String - Format Template
kind_summary_core_string_format = Compose a string from a template with %variable% placeholders
kind_name_core_string_length = String - Length
kind_summary_core_string_length = Get the length of a string in bytes or Unicode scalar values (chars)
kind_name_core_string_lowercase = String - Lowercase
kind_summary_core_string_lowercase = Convert a string to lowercase
kind_name_core_string_regex_match = String - Regex Match
kind_summary_core_string_regex_match = Test a string against a regex pattern and optionally capture groups
kind_name_core_string_replace = String - Replace
kind_summary_core_string_replace = Replace all occurrences of a literal or regex pattern in a string
kind_name_core_string_split = String - Split
kind_summary_core_string_split = Split a string into an array of parts by a separator
kind_name_core_string_substring = String - Substring
kind_summary_core_string_substring = Extract a slice of a string by char index range
kind_name_core_string_titlecase = String - Titlecase
kind_summary_core_string_titlecase = Capitalize the first character of each whitespace-delimited word
kind_name_core_string_trim = String - Trim Whitespace
kind_summary_core_string_trim = Remove leading and/or trailing whitespace from a string
kind_name_core_string_uppercase = String - Uppercase
kind_summary_core_string_uppercase = Convert a string to uppercase
kind_name_core_test_fire_trigger = Test Trigger
kind_summary_core_test_fire_trigger = Fire one trigger instance with synthetic outputs to run its actions
kind_name_core_time_add = Time - Date/Time Arithmetic
kind_summary_core_time_add = Add or subtract an amount from a UTC datetime; month/year addition clamps to the last valid day on overflow
kind_name_core_time_diff = Time - Time Difference
kind_summary_core_time_diff = Compute `to − from` as a signed float in the given unit (seconds, minutes, hours, or days)
kind_name_core_time_format = Time - Format Datetime
kind_summary_core_time_format = Format a datetime as text with a date pattern (e.g. HH:mm dd.MM.yyyy or d MMMM yyyy) in local time, or in UTC when the toggle is on
kind_name_core_time_now = Time - Get Current Time
kind_summary_core_time_now = Capture the current time; writes the datetime to `into_var`, a formatted string to `time.formatted` (custom: a date pattern such as HH:mm dd.MM.yyyy, local time or UTC), and unix seconds to `time.unix_seconds`
kind_name_core_time_parse = Time - Parse Time String
kind_summary_core_time_parse = Read text into a datetime with a date pattern (e.g. dd.MM.yyyy HH:mm); text without an offset is read as local time, or as UTC when the toggle is on
kind_name_core_trigger_disable = Disable Trigger
kind_summary_core_trigger_disable = Prevent a trigger instance from firing actions
kind_name_core_trigger_enable = Enable Trigger
kind_summary_core_trigger_enable = Allow a trigger instance to fire actions
kind_name_core_trigger_toggle = Toggle Trigger
kind_summary_core_trigger_toggle = Flip a trigger instance between enabled and disabled
kind_name_core_url_open = Open URL
kind_summary_core_url_open = Open a URL in the default browser
kind_name_core_users_get_var = Get User Variable
kind_summary_core_users_get_var = Read a per-user variable into an argument
kind_name_core_users_increment_var = Increment User Variable
kind_summary_core_users_increment_var = Increment a numeric per-user variable by an amount
kind_name_core_users_set_var = Set User Variable
kind_summary_core_users_set_var = Set a per-user variable to a value
kind_name_discord_webhook_delete_message = Delete Webhook Message
kind_summary_discord_webhook_delete_message = Deletes a previously posted Discord webhook message.
kind_name_discord_webhook_send_embed = Post Embed
kind_summary_discord_webhook_send_embed = Posts a rich embed message to a Discord webhook.
kind_name_discord_webhook_send_file = Send File Attachment
kind_summary_discord_webhook_send_file = Uploads a file from disk to a Discord webhook with an optional caption.
kind_name_discord_webhook_send_message = Post Text
kind_summary_discord_webhook_send_message = Posts a text message to a Discord webhook.
kind_name_discord_webhook_update_message = Edit Message
kind_summary_discord_webhook_update_message = Edits a previously posted Discord webhook message.
kind_name_donation_received = Donation received
kind_summary_donation_received = Fires when a donation arrives through a connected donation service
kind_name_donations_test_donation = Send test donation
kind_summary_donations_test_donation = Announce a fake donation that fires donation triggers marked as a test
kind_name_hotkey_global_pressed = Hotkey Pressed
kind_summary_hotkey_global_pressed = Fires when a registered global hotkey combo is pressed.
kind_name_hotkey_global_released = Hotkey Released
kind_summary_hotkey_global_released = Fires when a held global hotkey combo is released.
kind_name_midi_device_connected = MIDI Device Connected
kind_summary_midi_device_connected = Fires when a MIDI port appears (device plugged in).
kind_name_midi_device_disconnected = MIDI Device Disconnected
kind_summary_midi_device_disconnected = Fires when a MIDI port disappears (device unplugged).
kind_name_midi_input_control_change = MIDI Control Change
kind_summary_midi_input_control_change = Fires when a MIDI Control Change (CC) message is received.
kind_name_midi_input_note_off = MIDI Note Off
kind_summary_midi_input_note_off = Fires when a MIDI Note Off message is received (including velocity-0 Note On).
kind_name_midi_input_note_on = MIDI Note On
kind_summary_midi_input_note_on = Fires when a MIDI Note On message is received.
kind_name_midi_input_pitch_bend = MIDI Pitch Bend
kind_summary_midi_input_pitch_bend = Fires when a MIDI Pitch Bend message is received.
kind_name_midi_input_program_change = MIDI Program Change
kind_summary_midi_input_program_change = Fires when a MIDI Program Change message is received.
kind_name_midi_send = Send MIDI Message
kind_summary_midi_send = Sends a MIDI message (Note On, Note Off, CC, or raw) to an output port.
kind_name_overlay_send = Send to Overlay
kind_summary_overlay_send = Send content to a browser-source overlay
kind_name_script_emit_event = Emit Custom Event
kind_summary_script_emit_event = Publish a named custom event to the bus
kind_name_script_event_custom = Custom Script Event
kind_summary_script_event_custom = Fires when a rhai script publishes a named custom event
kind_name_script_run_inline = Run Inline Script
kind_summary_script_run_inline = Execute a rhai script snippet embedded directly in the config
kind_name_script_run_named = Run Script (Named)
kind_summary_script_run_named = Execute a saved script by name
kind_name_server_broadcast = Broadcast to Overlay Clients
kind_summary_server_broadcast = Push a JSON message to all connected WebSocket clients
kind_name_soundboard_sound_play = Play Sound
kind_summary_soundboard_sound_play = Play a soundboard clip on the configured output device
kind_name_soundboard_sound_stop = Stop Sound
kind_summary_soundboard_sound_stop = Stop a soundboard clip (leave the clip empty to stop everything)
kind_name_soundboard_sound_stop_all = Stop All Sounds
kind_summary_soundboard_sound_stop_all = Stop every soundboard clip currently playing
kind_name_soundboard_volume_set_master = Set Master Volume
kind_summary_soundboard_volume_set_master = Set the soundboard master volume applied to every clip
kind_name_timer_tick = Interval timer
kind_summary_timer_tick = Fires every N minutes, optionally only while live and after enough chat activity
kind_name_tts_alias_set = Set Voice Alias
kind_summary_tts_alias_set = Assign a specific engine and voice to a viewer alias
kind_name_tts_alias_switch_active = Switch Active Voice Alias
kind_summary_tts_alias_switch_active = Repoint an existing viewer alias to a different engine and voice; no-op if the viewer has no alias
kind_name_tts_queue_clear = Clear TTS Queue
kind_summary_tts_queue_clear = Drop queued TTS items, optionally also stopping the current utterance
kind_name_tts_queue_pause = Pause TTS Queue
kind_summary_tts_queue_pause = Pause processing of the TTS speak queue
kind_name_tts_queue_resume = Resume TTS Queue
kind_summary_tts_queue_resume = Resume processing of the TTS speak queue after a pause
kind_name_tts_queue_skip_current = Skip Current TTS
kind_summary_tts_queue_skip_current = Skip the current TTS item and advance to the next in the queue
kind_name_tts_speak_stop = Stop Speaking
kind_summary_tts_speak_stop = Interrupt the current TTS utterance
kind_name_tts_speak_text = Speak Text
kind_summary_tts_speak_text = Send text to the TTS speak queue, optionally with a chosen engine voice
kind_name_tts_speak_text_with_engine = Speak with Engine Override
kind_summary_tts_speak_text_with_engine = Queue a TTS utterance using a specific engine
kind_name_twitch_automod_add_blocked_term = Add Blocked Term
kind_summary_twitch_automod_add_blocked_term = Adds a word or phrase to the AutoMod blocked terms list.
kind_name_twitch_automod_approve_message = Approve AutoMod Message
kind_summary_twitch_automod_approve_message = Allows a message held by AutoMod to appear in chat.
kind_name_twitch_automod_deny_message = Deny AutoMod Message
kind_summary_twitch_automod_deny_message = Denies a message held by AutoMod so it is removed from the queue.
kind_name_twitch_automod_message_held = AutoMod message held
kind_summary_twitch_automod_message_held = Fires when AutoMod holds a chat message for moderator review
kind_name_twitch_automod_message_updated = AutoMod message decision updated
kind_summary_twitch_automod_message_updated = Fires when a moderator approves, denies, or allows a held AutoMod message to expire
kind_name_twitch_automod_remove_blocked_term = Remove Blocked Term
kind_summary_twitch_automod_remove_blocked_term = Removes a term from the AutoMod blocked terms list by its ID.
kind_name_twitch_automod_settings_updated = AutoMod settings updated
kind_summary_twitch_automod_settings_updated = Fires when a moderator changes the channel AutoMod filter settings
kind_name_twitch_automod_terms_updated = AutoMod terms updated
kind_summary_twitch_automod_terms_updated = Fires when a moderator adds or removes permitted or blocked AutoMod terms
kind_name_twitch_automod_update_settings = Update AutoMod Settings
kind_summary_twitch_automod_update_settings = Sets the overall AutoMod level or individual filter category levels.
kind_name_twitch_channel_ban = User banned (permanent)
kind_summary_twitch_channel_ban = Fires when a viewer is permanently banned from the channel
kind_name_twitch_channel_cancel_raid = Cancel Raid
kind_summary_twitch_channel_cancel_raid = Cancels a pending raid initiated from this channel.
kind_name_twitch_channel_chat_settings_updated = Chat settings updated
kind_summary_twitch_channel_chat_settings_updated = Fires when a broadcaster's chat settings change (emote-only, follower-only, slow mode, sub-only, unique-chat)
kind_name_twitch_channel_create_marker = Create Stream Marker
kind_summary_twitch_channel_create_marker = Places a bookmark at the current live stream position.
kind_name_twitch_channel_follow = Follow
kind_summary_twitch_channel_follow = Fires when a viewer follows your channel
kind_name_twitch_channel_moderator_added = Moderator added
kind_summary_twitch_channel_moderator_added = Fires when a viewer is granted moderator status in the channel
kind_name_twitch_channel_moderator_removed = Moderator removed
kind_summary_twitch_channel_moderator_removed = Fires when moderator status is revoked from a viewer
kind_name_twitch_channel_raid_received = Raid received
kind_summary_twitch_channel_raid_received = Fires when another streamer raids your channel
kind_name_twitch_channel_raid_sent = Raid sent
kind_summary_twitch_channel_raid_sent = Fires when you raid another streamer's channel
kind_name_twitch_channel_run_ad = Run Ad
kind_summary_twitch_channel_run_ad = Starts a commercial break in the broadcaster's channel.
kind_name_twitch_channel_send_shoutout = Send Shoutout
kind_summary_twitch_channel_send_shoutout = Sends a shoutout to another broadcaster in the channel chat.
kind_name_twitch_channel_shield_mode_ended = Shield mode ended
kind_summary_twitch_channel_shield_mode_ended = Fires when a moderator deactivates shield mode in the channel
kind_name_twitch_channel_shield_mode_started = Shield mode started
kind_summary_twitch_channel_shield_mode_started = Fires when a moderator activates shield mode in the channel
kind_name_twitch_channel_shoutout_received = Shoutout received
kind_summary_twitch_channel_shoutout_received = Fires when another broadcaster gives the channel a shoutout
kind_name_twitch_channel_shoutout_sent = Shoutout sent
kind_summary_twitch_channel_shoutout_sent = Fires when the broadcaster or a moderator gives a shoutout to another channel
kind_name_twitch_channel_snooze_ad = Snooze Next Ad
kind_summary_twitch_channel_snooze_ad = Delays the next scheduled ad break by 5 minutes.
kind_name_twitch_channel_start_raid = Start Raid
kind_summary_twitch_channel_start_raid = Starts a raid to another broadcaster's channel.
kind_name_twitch_channel_suspicious_user_message = Suspicious user message
kind_summary_twitch_channel_suspicious_user_message = Fires when a user flagged as suspicious sends a message in the channel
kind_name_twitch_channel_timeout = User timed out
kind_summary_twitch_channel_timeout = Fires when a viewer is timed out from the channel
kind_name_twitch_channel_unban = User unbanned
kind_summary_twitch_channel_unban = Fires when a viewer's ban or timeout is lifted
kind_name_twitch_channel_unban_request_created = Unban request created
kind_summary_twitch_channel_unban_request_created = Fires when a banned viewer submits an unban request
kind_name_twitch_channel_unban_request_resolved = Unban request resolved
kind_summary_twitch_channel_unban_request_resolved = Fires when a moderator approves, denies, or cancels an unban request
kind_name_twitch_channel_update = Channel updated
kind_summary_twitch_channel_update = Fires when the broadcaster updates their channel title, category, or language
kind_name_twitch_channel_update_category = Update Stream Category
kind_summary_twitch_channel_update_category = Changes the broadcaster's game/category by its Helix game_id.
kind_name_twitch_channel_update_tags = Update Stream Tags
kind_summary_twitch_channel_update_tags = Replaces the broadcaster's stream tags (max 10, each ≤25 chars). Empty clears all tags.
kind_name_twitch_channel_update_title = Update Stream Title
kind_summary_twitch_channel_update_title = Updates the broadcaster's stream title.
kind_name_twitch_channel_vip_added = VIP added
kind_summary_twitch_channel_vip_added = Fires when a viewer is granted VIP status in the channel
kind_name_twitch_channel_vip_removed = VIP removed
kind_summary_twitch_channel_vip_removed = Fires when VIP status is revoked from a viewer
kind_name_twitch_channel_warning_acknowledged = Warning acknowledged
kind_summary_twitch_channel_warning_acknowledged = Fires when a user acknowledges a warning issued by a moderator
kind_name_twitch_channel_warning_sent = Warning sent
kind_summary_twitch_channel_warning_sent = Fires when a moderator issues a warning to a user
kind_name_twitch_channel_points_automatic_reward_redeemed = Automatic reward redeemed
kind_summary_twitch_channel_points_automatic_reward_redeemed = Fires when a viewer redeems a built-in automatic channel point reward
kind_name_twitch_channel_points_cancel_redemption = Cancel Channel Point Redemption
kind_summary_twitch_channel_points_cancel_redemption = Cancels a channel point redemption and refunds the viewer's points.
kind_name_twitch_channel_points_create_reward = Create Channel Point Reward
kind_summary_twitch_channel_points_create_reward = Creates a new custom channel point reward on the broadcaster's channel.
kind_name_twitch_channel_points_delete_reward = Delete Channel Point Reward
kind_summary_twitch_channel_points_delete_reward = Permanently deletes a custom channel point reward.
kind_name_twitch_channel_points_disable_reward = Disable Channel Point Reward
kind_summary_twitch_channel_points_disable_reward = Disables a custom channel point reward so viewers cannot redeem it.
kind_name_twitch_channel_points_enable_reward = Enable Channel Point Reward
kind_summary_twitch_channel_points_enable_reward = Enables a custom channel point reward so viewers can redeem it.
kind_name_twitch_channel_points_fulfill_redemption = Fulfill Channel Point Redemption
kind_summary_twitch_channel_points_fulfill_redemption = Marks a channel point redemption as fulfilled.
kind_name_twitch_channel_points_pause_reward = Pause Channel Point Reward
kind_summary_twitch_channel_points_pause_reward = Pauses a custom channel point reward, queueing new redemptions without fulfilling them.
kind_name_twitch_channel_points_redemption = Channel Point Redemption
kind_summary_twitch_channel_points_redemption = Fires when a viewer redeems a channel point reward
kind_name_twitch_channel_points_redemption_updated = Channel point redemption updated
kind_summary_twitch_channel_points_redemption_updated = Fires when a channel point redemption is fulfilled or canceled
kind_name_twitch_channel_points_resume_reward = Resume Channel Point Reward
kind_summary_twitch_channel_points_resume_reward = Resumes a paused custom channel point reward so redemptions are fulfilled normally.
kind_name_twitch_channel_points_reward_added = Channel point reward added
kind_summary_twitch_channel_points_reward_added = Fires when a new channel point reward is created on the broadcaster's channel
kind_name_twitch_channel_points_reward_removed = Channel point reward removed
kind_summary_twitch_channel_points_reward_removed = Fires when a channel point reward is deleted from the broadcaster's channel
kind_name_twitch_channel_points_reward_updated = Channel point reward updated
kind_summary_twitch_channel_points_reward_updated = Fires when an existing channel point reward is modified on the broadcaster's channel
kind_name_twitch_channel_points_update_reward = Update Channel Point Reward
kind_summary_twitch_channel_points_update_reward = Updates selected fields of an existing custom channel point reward.
kind_name_twitch_chat_cheer_message = Cheer message
kind_summary_twitch_chat_cheer_message = Fires when a chat message contains a bits cheer
kind_name_twitch_chat_clear = Clear Chat
kind_summary_twitch_chat_clear = Clears all messages from Twitch chat.
kind_name_twitch_chat_cleared = Chat Cleared
kind_summary_twitch_chat_cleared = Fires when all messages are cleared from the chat room
kind_name_twitch_chat_command = Chat command
kind_summary_twitch_chat_command = Fires when a chat message matches a command phrase
kind_name_twitch_chat_delete_message = Delete Message
kind_summary_twitch_chat_delete_message = Deletes a specific message from Twitch chat.
kind_name_twitch_chat_message = Chat message
kind_summary_twitch_chat_message = Fires for every message posted in chat
kind_name_twitch_chat_message_deleted = Chat Message Deleted
kind_summary_twitch_chat_message_deleted = Fires when a chat message is deleted by a moderator or bot
kind_name_twitch_chat_reply = Reply to Message
kind_summary_twitch_chat_reply = Sends a reply to a specific chat message.
kind_name_twitch_chat_send_announcement = Send Announcement
kind_summary_twitch_chat_send_announcement = Posts a highlighted announcement in the Twitch chat.
kind_name_twitch_chat_send_message = Send Chat Message
kind_summary_twitch_chat_send_message = Send a message to a platform chat channel
kind_name_twitch_chat_send_whisper = Send Whisper
kind_summary_twitch_chat_send_whisper = Sends a private whisper to another user.
kind_name_twitch_chat_set_mode = Set Chat Mode
kind_summary_twitch_chat_set_mode = Configures emote-only, follower, slow, subscriber, or unique-chat mode.
kind_name_twitch_chat_whisper = Whisper received
kind_summary_twitch_chat_whisper = Fires when the bot account receives a whisper
kind_name_twitch_goal_ended = Goal ended
kind_summary_twitch_goal_ended = Fires when a creator goal ends, whether achieved or not
kind_name_twitch_goal_get_current = Get Current Goal
kind_summary_twitch_goal_get_current = Reads the broadcaster's active creator goal and exposes its details as variables.
kind_name_twitch_goal_progress = Goal progress
kind_summary_twitch_goal_progress = Fires when a creator goal receives a progress update
kind_name_twitch_goal_started = Goal started
kind_summary_twitch_goal_started = Fires when a creator goal begins on the broadcaster's channel
kind_name_twitch_guest_star_assign_slot = Assign Guest Star Slot
kind_summary_twitch_guest_star_assign_slot = Assigns an invited guest to a numbered slot in the Guest Star session.
kind_name_twitch_guest_star_end_session = End Guest Star Session
kind_summary_twitch_guest_star_end_session = Ends the active Guest Star session for the broadcaster's channel.
kind_name_twitch_guest_star_guest_updated = Guest Star guest updated
kind_summary_twitch_guest_star_guest_updated = Fires when a Guest Star guest or slot changes state in the session
kind_name_twitch_guest_star_invite = Send Guest Star Invite
kind_summary_twitch_guest_star_invite = Invites a viewer to join the active Guest Star session.
kind_name_twitch_guest_star_remove_guest = Remove Guest Star Guest
kind_summary_twitch_guest_star_remove_guest = Removes a guest from their slot in the active Guest Star session.
kind_name_twitch_guest_star_session_began = Guest Star session began
kind_summary_twitch_guest_star_session_began = Fires when the broadcaster starts a new Guest Star session
kind_name_twitch_guest_star_session_ended = Guest Star session ended
kind_summary_twitch_guest_star_session_ended = Fires when a running Guest Star session ends
kind_name_twitch_guest_star_settings_updated = Guest Star settings updated
kind_summary_twitch_guest_star_settings_updated = Fires when the broadcaster's Guest Star preferences change
kind_name_twitch_guest_star_update_slot = Update Guest Star Slot Settings
kind_summary_twitch_guest_star_update_slot = Updates audio, video, or volume settings for a slot in the active Guest Star session.
kind_name_twitch_moderation_add_moderator = Add Moderator
kind_summary_twitch_moderation_add_moderator = Grants moderator status to a user in the channel.
kind_name_twitch_moderation_add_vip = Add VIP
kind_summary_twitch_moderation_add_vip = Grants VIP status to a user in the channel.
kind_name_twitch_moderation_ban_user = Ban User
kind_summary_twitch_moderation_ban_user = Permanently bans a user from the channel.
kind_name_twitch_moderation_remove_moderator = Remove Moderator
kind_summary_twitch_moderation_remove_moderator = Revokes moderator status from a user in the channel.
kind_name_twitch_moderation_remove_vip = Remove VIP
kind_summary_twitch_moderation_remove_vip = Revokes VIP status from a user in the channel.
kind_name_twitch_moderation_shield_mode_off = Disable Shield Mode
kind_summary_twitch_moderation_shield_mode_off = Deactivates Shield Mode on the broadcaster's channel.
kind_name_twitch_moderation_shield_mode_on = Enable Shield Mode
kind_summary_twitch_moderation_shield_mode_on = Activates Shield Mode on the broadcaster's channel.
kind_name_twitch_moderation_timeout_user = Timeout User
kind_summary_twitch_moderation_timeout_user = Temporarily times out a user from the channel chat.
kind_name_twitch_moderation_unban_user = Unban User
kind_summary_twitch_moderation_unban_user = Removes a ban or active timeout from a user.
kind_name_twitch_moderation_warn_user = Warn User
kind_summary_twitch_moderation_warn_user = Sends an official Twitch warning to a user in the channel.
kind_name_twitch_poll_end = End Poll
kind_summary_twitch_poll_end = Ends an active poll immediately. Use 'terminated' to stop with results visible or 'archived' to hide it.
kind_name_twitch_poll_ended = Poll ended
kind_summary_twitch_poll_ended = Fires when a poll ends (completed, archived, or terminated)
kind_name_twitch_poll_progress = Poll progress
kind_summary_twitch_poll_progress = Fires when vote counts update on an active poll
kind_name_twitch_poll_start = Start Poll
kind_summary_twitch_poll_start = Creates a new Twitch poll. Outputs poll.id for chaining with End Poll.
kind_name_twitch_poll_started = Poll started
kind_summary_twitch_poll_started = Fires when a poll begins on the broadcaster's channel
kind_name_twitch_prediction_cancel = Cancel Prediction
kind_summary_twitch_prediction_cancel = Cancels an active prediction and refunds all channel points to voters.
kind_name_twitch_prediction_ended = Prediction ended
kind_summary_twitch_prediction_ended = Fires when a prediction resolves or is canceled
kind_name_twitch_prediction_lock = Lock Prediction
kind_summary_twitch_prediction_lock = Locks an active prediction, preventing new votes. Viewers can no longer change their outcome.
kind_name_twitch_prediction_locked = Prediction locked
kind_summary_twitch_prediction_locked = Fires when a prediction is locked and no more points can be placed
kind_name_twitch_prediction_progress = Prediction progress
kind_summary_twitch_prediction_progress = Fires when point totals update on an active prediction
kind_name_twitch_prediction_resolve = Resolve Prediction
kind_summary_twitch_prediction_resolve = Resolves a prediction by declaring a winning outcome and distributing points to winners.
kind_name_twitch_prediction_start = Start Prediction
kind_summary_twitch_prediction_start = Creates a new Twitch prediction. Outputs prediction.id for chaining.
kind_name_twitch_prediction_started = Prediction started
kind_summary_twitch_prediction_started = Fires when a prediction begins on the broadcaster's channel
kind_name_twitch_shared_chat_message_received = Shared chat message
kind_summary_twitch_shared_chat_message_received = Fires when a message arrives via a Shared Chat session from another channel
kind_name_twitch_shared_chat_session_began = Shared Chat session began
kind_summary_twitch_shared_chat_session_began = Fires when the broadcaster's channel joins an active shared chat session
kind_name_twitch_shared_chat_session_ended = Shared Chat session ended
kind_summary_twitch_shared_chat_session_ended = Fires when the broadcaster's channel leaves a shared chat session or the session ends
kind_name_twitch_shared_chat_session_updated = Shared Chat session updated
kind_summary_twitch_shared_chat_session_updated = Fires when the active shared chat session the broadcaster's channel is in changes
kind_name_twitch_stream_ad_break_started = Ad break started
kind_summary_twitch_stream_ad_break_started = Fires when an ad break begins on the broadcaster's channel
kind_name_twitch_stream_offline = Stream offline
kind_summary_twitch_stream_offline = Fires when your stream ends
kind_name_twitch_stream_online = Stream online
kind_summary_twitch_stream_online = Fires when your stream goes live
kind_name_twitch_support_charity_donation = Charity donation
kind_summary_twitch_support_charity_donation = Fires when a viewer donates to the active charity campaign
kind_name_twitch_support_charity_progress = Charity campaign progress
kind_summary_twitch_support_charity_progress = Fires on each charity campaign progress update
kind_name_twitch_support_charity_started = Charity campaign started
kind_summary_twitch_support_charity_started = Fires when the broadcaster starts a charity campaign
kind_name_twitch_support_charity_stopped = Charity campaign stopped
kind_summary_twitch_support_charity_stopped = Fires when the broadcaster ends a charity campaign
kind_name_twitch_support_cheer = Cheer
kind_summary_twitch_support_cheer = Fires on a bits cheer event
kind_name_twitch_support_gift_sub = Gift subscription
kind_summary_twitch_support_gift_sub = Fires when a user gifts one or more subscriptions
kind_name_twitch_support_hype_train_ended = Hype Train ended
kind_summary_twitch_support_hype_train_ended = Fires when a Hype Train concludes on the channel
kind_name_twitch_support_hype_train_progress = Hype Train progress
kind_summary_twitch_support_hype_train_progress = Fires on each Hype Train progress update at or above the configured level
kind_name_twitch_support_hype_train_started = Hype Train started
kind_summary_twitch_support_hype_train_started = Fires when a Hype Train begins on the channel
kind_name_twitch_support_resubscriber = Re-subscriber
kind_summary_twitch_support_resubscriber = Fires on subscription renewals
kind_name_twitch_support_subscriber = New subscriber
kind_summary_twitch_support_subscriber = Fires on a first-time channel subscription
kind_name_twitch_user_update = User profile updated
kind_summary_twitch_user_update = Fires when the authenticated user changes their profile description or display name
kind_name_youtube_channel_member = New member
kind_summary_youtube_channel_member = Fires when a viewer joins as a new YouTube channel member
kind_name_youtube_channel_member_gift = Memberships gifted
kind_summary_youtube_channel_member_gift = Fires when a viewer gifts a batch of YouTube channel memberships
kind_name_youtube_channel_member_gift_received = Gift membership received
kind_summary_youtube_channel_member_gift_received = Fires when a viewer receives a gifted YouTube channel membership
kind_name_youtube_channel_member_milestone = Member milestone
kind_summary_youtube_channel_member_milestone = Fires when a YouTube channel member reaches a membership milestone
kind_name_youtube_channel_user_banned = User banned
kind_summary_youtube_channel_user_banned = Fires when a user is banned (permanent or temporary) from YouTube live chat
kind_name_youtube_chat_command = Chat command
kind_summary_youtube_chat_command = Fires when a YouTube chat message matches a command phrase
kind_name_youtube_chat_create_poll = Create Live Poll
kind_summary_youtube_chat_create_poll = Posts a poll to the active YouTube live chat.
kind_name_youtube_chat_delete_message = Delete Message
kind_summary_youtube_chat_delete_message = Removes a message from the active YouTube live chat by its resource id.
kind_name_youtube_chat_message = Chat message
kind_summary_youtube_chat_message = Fires for every message posted in YouTube live chat
kind_name_youtube_chat_message_deleted = Message deleted
kind_summary_youtube_chat_message_deleted = Fires when a moderator deletes a message from YouTube live chat
kind_name_youtube_chat_send_message = Send Message
kind_summary_youtube_chat_send_message = Posts a message in the active YouTube live chat.
kind_name_youtube_chat_super_chat = Super Chat
kind_summary_youtube_chat_super_chat = Fires when a viewer sends a Super Chat in YouTube live chat
kind_name_youtube_chat_super_sticker = Super Sticker
kind_summary_youtube_chat_super_sticker = Fires when a viewer sends a Super Sticker in YouTube live chat
kind_name_youtube_lookup_stream_stats = Stream Stats
kind_summary_youtube_lookup_stream_stats = Fetches live-streaming details for the active YouTube broadcast.
kind_name_youtube_lookup_viewer = Lookup Viewer
kind_summary_youtube_lookup_viewer = Looks up a YouTube channel by handle or channel id.
kind_name_youtube_moderation_add_moderator = Add Moderator
kind_summary_youtube_moderation_add_moderator = Grants a user moderator status in the active YouTube live chat.
kind_name_youtube_moderation_ban_user = Ban User
kind_summary_youtube_moderation_ban_user = Permanently bans a user from the active YouTube live chat.
kind_name_youtube_moderation_remove_moderator = Remove Moderator
kind_summary_youtube_moderation_remove_moderator = Revokes a user's moderator status in the active YouTube live chat.
kind_name_youtube_moderation_timeout_user = Timeout User
kind_summary_youtube_moderation_timeout_user = Temporarily bans a user from the active YouTube live chat.
kind_name_youtube_moderation_unban_user = Unban User
kind_summary_youtube_moderation_unban_user = Lifts a ban or timeout issued earlier in this session.
kind_name_youtube_stream_insert_ad_break = Insert Ad Break
kind_summary_youtube_stream_insert_ad_break = Triggers a mid-roll ad cuepoint on the active YouTube broadcast.
kind_name_youtube_stream_offline = Live broadcast ended
kind_summary_youtube_stream_offline = Fires when a YouTube live broadcast ends
kind_name_youtube_stream_online = Live broadcast started
kind_summary_youtube_stream_online = Fires when a YouTube live broadcast becomes active
kind_name_youtube_stream_set_thumbnail = Set Thumbnail
kind_summary_youtube_stream_set_thumbnail = Uploads a custom thumbnail image for the active YouTube broadcast.
kind_name_youtube_stream_title_changed = Live broadcast title changed
kind_summary_youtube_stream_title_changed = Fires when the title of an active YouTube live broadcast is edited
kind_name_youtube_stream_update_category = Update Stream Category
kind_summary_youtube_stream_update_category = Sets the video category of the active YouTube broadcast.
kind_name_youtube_stream_update_description = Update Stream Description
kind_summary_youtube_stream_update_description = Sets the description of the active YouTube broadcast.
kind_name_youtube_stream_update_privacy = Update Stream Privacy
kind_summary_youtube_stream_update_privacy = Sets the privacy status of the active YouTube broadcast.
kind_name_youtube_stream_update_title = Update Stream Title
kind_summary_youtube_stream_update_title = Sets the title of the active YouTube broadcast.
kind_name_kick_channel_hosted = Channel hosted
kind_summary_kick_channel_hosted = Fires when another streamer hosts this Kick channel
kind_name_kick_channel_reward_redemption_updated = Reward redeemed
kind_summary_kick_channel_reward_redemption_updated = Fires when a viewer redeems a Kick channel reward
kind_name_kick_channel_subscribed = Subscription
kind_summary_kick_channel_subscribed = Fires when a viewer subscribes to the Kick channel
kind_name_kick_channel_subscription_gifts = Gifted subscriptions
kind_summary_kick_channel_subscription_gifts = Fires when a viewer gifts subscriptions to the Kick channel
kind_name_kick_channel_update_info = Update Channel Info
kind_summary_kick_channel_update_info = Updates stream title, category, or tags on the Kick channel. Requires channel:write scope.
kind_name_kick_chat_command = Chat command
kind_summary_kick_chat_command = Fires when a Kick chat message starts with a command phrase
kind_name_kick_chat_delete_message = Delete Message
kind_summary_kick_chat_delete_message = Removes a chat message from the Kick channel by its message id. Requires moderation:chat_message:manage scope.
kind_name_kick_chat_message_deleted = Message deleted
kind_summary_kick_chat_message_deleted = Fires when a moderator deletes a chat message in Kick
kind_name_kick_chat_message_sent = Chat message
kind_summary_kick_chat_message_sent = Fires for every message posted in Kick live chat
kind_name_kick_chat_send_message = Send Message
kind_summary_kick_chat_send_message = Posts a message to the Kick channel chat.
kind_name_kick_livestream_metadata_updated = Livestream metadata
kind_summary_kick_livestream_metadata_updated = Fires when the Kick channel stream title or category changes
kind_name_kick_livestream_status_updated = Livestream status
kind_summary_kick_livestream_status_updated = Fires when the Kick channel livestream status changes (live or offline)
kind_name_kick_lookup_category = Lookup Category
kind_summary_kick_lookup_category = Searches Kick categories by name and returns the matching numeric ids.
kind_name_kick_lookup_stream_stats = Stream Stats
kind_summary_kick_lookup_stream_stats = Fetches live stream details for your own Kick channel.
kind_name_kick_lookup_user = Lookup User
kind_summary_kick_lookup_user = Looks up a Kick channel by slug and reports its identity and live state.
kind_name_kick_moderation_ban = Ban User
kind_summary_kick_moderation_ban = Permanently bans a user from the Kick channel. Requires moderation:ban scope.
kind_name_kick_moderation_banned = User banned
kind_summary_kick_moderation_banned = Fires when a moderator bans a user in Kick chat
kind_name_kick_moderation_timeout = Timeout User
kind_summary_kick_moderation_timeout = Temporarily bans a user from the Kick channel. Duration 1-10080 minutes. Requires moderation:ban scope.
kind_name_kick_moderation_unban = Unban User
kind_summary_kick_moderation_unban = Lifts an active ban or timeout for a user on the Kick channel. Requires moderation:ban scope.
kind_name_kick_reward_create = Create Channel Reward
kind_summary_kick_reward_create = Creates a new channel point reward on Kick. Requires channel:rewards:write scope.
kind_name_kick_reward_delete = Delete Channel Reward
kind_summary_kick_reward_delete = Permanently deletes a Kick channel reward. Requires channel:rewards:write scope and reward ownership.
kind_name_kick_reward_redemption_accept = Accept Reward Redemption(s)
kind_summary_kick_reward_redemption_accept = Accepts one or more pending Kick reward redemptions. Requires channel:rewards:write scope.
kind_name_kick_reward_redemption_reject = Reject Reward Redemption(s)
kind_summary_kick_reward_redemption_reject = Rejects one or more pending Kick reward redemptions. Requires channel:rewards:write scope.
kind_name_kick_reward_update = Update Channel Reward
kind_summary_kick_reward_update = Updates an existing Kick channel reward. Requires channel:rewards:write scope and reward ownership.
kind_name_vtube_expression_set = Set Expression
kind_summary_vtube_expression_set = Activates or deactivates a VTube Studio expression.
kind_name_vtube_expression_state_changed = VTube Studio expression state changed
kind_summary_vtube_expression_state_changed = Fires when a VTube Studio expression is activated or deactivated.
kind_name_vtube_hotkey_trigger = Trigger Hotkey
kind_summary_vtube_hotkey_trigger = Triggers a VTube Studio hotkey by its ID.
kind_name_vtube_hotkey_triggered = VTube Studio hotkey triggered
kind_summary_vtube_hotkey_triggered = Fires when a VTube Studio hotkey is activated.
kind_name_vtube_item_added = VTube Studio item added
kind_summary_vtube_item_added = Fires when an item is loaded into the VTube Studio scene.
kind_name_vtube_item_load = Load Item
kind_summary_vtube_item_load = Loads an item file into the VTube Studio scene.
kind_name_vtube_item_move = Move Item
kind_summary_vtube_item_move = Moves, resizes, or rotates a loaded VTube Studio item on screen.
kind_name_vtube_item_pin = Pin Item
kind_summary_vtube_item_pin = Pins or unpins a loaded VTube Studio item to the current model.
kind_name_vtube_item_removed = VTube Studio item removed
kind_summary_vtube_item_removed = Fires when an item is removed from the VTube Studio scene.
kind_name_vtube_item_throw = Throw Item
kind_summary_vtube_item_throw = Spawns an item and animates it moving across the VTube Studio scene.
kind_name_vtube_item_unload_all = Remove All Items
kind_summary_vtube_item_unload_all = Removes every item currently loaded in the VTube Studio scene.
kind_name_vtube_lookup_current_model = Get Current Model
kind_summary_vtube_lookup_current_model = Queries VTube Studio for the currently loaded model.
kind_name_vtube_lookup_expressions = Get Expressions
kind_summary_vtube_lookup_expressions = Queries VTube Studio for the expressions in the current model.
kind_name_vtube_lookup_hotkeys = Get Hotkeys
kind_summary_vtube_lookup_hotkeys = Queries VTube Studio for the hotkeys in the current model.
kind_name_vtube_lookup_items = Get Items
kind_summary_vtube_lookup_items = Queries VTube Studio for the item instances currently in the scene.
kind_name_vtube_lookup_parameters = Get Parameters
kind_summary_vtube_lookup_parameters = Queries VTube Studio for the input and Live2D parameters of the current model.
kind_name_vtube_model_config_changed = VTube Studio model config changed
kind_summary_vtube_model_config_changed = Fires when the configuration of the active VTube Studio model changes.
kind_name_vtube_model_load = Load Model
kind_summary_vtube_model_load = Loads a VTube Studio model by its ID.
kind_name_vtube_model_loaded = VTube Studio model loaded
kind_summary_vtube_model_loaded = Fires when a VTube Studio model is loaded.
kind_name_vtube_model_move = Move Model
kind_summary_vtube_model_move = Moves or rotates the VTube Studio model on screen.
kind_name_vtube_model_set_physics = Toggle Physics
kind_summary_vtube_model_set_physics = Temporarily overrides the VTube Studio model's physics strength.
kind_name_vtube_model_tint = Tint Model
kind_summary_vtube_model_tint = Applies a color tint over every ArtMesh on the VTube Studio model.
kind_name_vtube_model_unloaded = VTube Studio model unloaded
kind_summary_vtube_model_unloaded = Fires when a VTube Studio model is unloaded.
kind_name_vtube_param_set = Set Parameter
kind_summary_vtube_param_set = Injects a value into a VTube Studio parameter, optionally held for a duration.
kind_name_vtube_params_reset = Reset to Idle
kind_summary_vtube_params_reset = Deactivates every currently active expression, returning the model to idle.
kind_name_vtube_tracking_face_found = VTube Studio face found
kind_summary_vtube_tracking_face_found = Fires when VTube Studio starts tracking a face.
kind_name_vtube_tracking_face_lost = VTube Studio face lost
kind_summary_vtube_tracking_face_lost = Fires when VTube Studio stops tracking a face.
kind_name_obs_audio_set_mute = Set Input Mute
kind_summary_obs_audio_set_mute = Mutes or unmutes an OBS audio input.
kind_name_obs_audio_set_volume = Set Source Volume
kind_summary_obs_audio_set_volume = Sets the volume of an audio input in dB.
kind_name_obs_audio_source_balance_changed = OBS audio source balance changed
kind_summary_obs_audio_source_balance_changed = Fires when the audio balance of an OBS input source changes.
kind_name_obs_audio_source_mute_changed = OBS audio source mute changed
kind_summary_obs_audio_source_mute_changed = Fires when an OBS input source is muted or unmuted.
kind_name_obs_audio_source_sync_offset_changed = OBS audio source sync offset changed
kind_summary_obs_audio_source_sync_offset_changed = Fires when the audio sync offset of an OBS input source changes.
kind_name_obs_audio_source_volume_changed = OBS audio source volume changed
kind_summary_obs_audio_source_volume_changed = Fires when the volume level of an OBS input source changes.
kind_name_obs_browser_refresh = Refresh Browser Source
kind_summary_obs_browser_refresh = Refreshes a browser source, bypassing its cache.
kind_name_obs_capture_screenshot = Save Source Screenshot
kind_summary_obs_capture_screenshot = Saves a screenshot of an OBS source to a file, format from the file extension.
kind_name_obs_collection_changing = OBS scene collection changing
kind_summary_obs_collection_changing = Fires when OBS begins switching to a different scene collection.
kind_name_obs_collection_current_changed = OBS scene collection changed
kind_summary_obs_collection_current_changed = Fires after OBS finishes switching to a different scene collection.
kind_name_obs_collection_list_changed = OBS scene collection list changed
kind_summary_obs_collection_list_changed = Fires when scene collections are added or removed in OBS.
kind_name_obs_connection_auth_failed = OBS authentication failed
kind_summary_obs_connection_auth_failed = Fires when the OBS WebSocket connection is rejected due to authentication failure.
kind_name_obs_connection_connected = OBS connected
kind_summary_obs_connection_connected = Fires when the OBS WebSocket connection is successfully established.
kind_name_obs_connection_disconnected = OBS disconnected
kind_summary_obs_connection_disconnected = Fires when the OBS WebSocket connection is lost.
kind_name_obs_filter_set_enabled = Set Filter Enabled
kind_summary_obs_filter_set_enabled = Enables or disables a filter on an OBS source.
kind_name_obs_filters_created = OBS filter created
kind_summary_obs_filters_created = Fires when a filter is added to an OBS source.
kind_name_obs_filters_enabled_changed = OBS filter enable state changed
kind_summary_obs_filters_enabled_changed = Fires when a filter on an OBS source is enabled or disabled.
kind_name_obs_filters_removed = OBS filter removed
kind_summary_obs_filters_removed = Fires when a filter is removed from an OBS source.
kind_name_obs_media_restart = Restart Media Source
kind_summary_obs_media_restart = Restarts playback of an OBS media input from the beginning.
kind_name_obs_profile_current_changed = OBS profile changed
kind_summary_obs_profile_current_changed = Fires when OBS finishes switching to a different profile.
kind_name_obs_profile_list_changed = OBS profile list changed
kind_summary_obs_profile_list_changed = Fires when profiles are added or removed in OBS.
kind_name_obs_profile_switch = Switch Profile
kind_summary_obs_profile_switch = Switches OBS to a different profile by name.
kind_name_obs_record_file_changed = OBS recording file changed
kind_summary_obs_record_file_changed = Fires when OBS switches to a new output file mid-session (e.g. automatic file split).
kind_name_obs_record_get_status = Get Recording Status
kind_summary_obs_record_get_status = Queries OBS for the current recording state, pause state, and elapsed duration.
kind_name_obs_record_pause = Pause Recording
kind_summary_obs_record_pause = Pauses the OBS recording output.
kind_name_obs_record_paused = OBS recording paused
kind_summary_obs_record_paused = Fires when a running OBS recording output is paused.
kind_name_obs_record_resume = Resume Recording
kind_summary_obs_record_resume = Resumes the OBS recording output after a pause.
kind_name_obs_record_resumed = OBS recording resumed
kind_summary_obs_record_resumed = Fires when a paused OBS recording output resumes writing.
kind_name_obs_record_set_active = Set Record Active
kind_summary_obs_record_set_active = Starts or stops the OBS record output to an explicit state.
kind_name_obs_record_set_directory = Set Record Directory
kind_summary_obs_record_set_directory = Sets the directory OBS writes recording files to.
kind_name_obs_record_start = Start Recording
kind_summary_obs_record_start = Starts OBS recording output.
kind_name_obs_record_started = OBS recording started
kind_summary_obs_record_started = Fires when OBS recording output becomes active and writing to disk.
kind_name_obs_record_starting = OBS recording starting
kind_summary_obs_record_starting = Fires when OBS begins the recording start sequence (before output is active).
kind_name_obs_record_status_changed = OBS recording status changed
kind_summary_obs_record_status_changed = Fires on any recording output state transition (starting, started, stopping, stopped, paused, resumed).
kind_name_obs_record_stop = Stop Recording
kind_summary_obs_record_stop = Stops OBS recording output.
kind_name_obs_record_stopped = OBS recording stopped
kind_summary_obs_record_stopped = Fires when OBS recording output becomes inactive. The output_path arg carries the saved file path.
kind_name_obs_record_stopping = OBS recording stopping
kind_summary_obs_record_stopping = Fires when OBS begins the recording stop sequence (before output becomes inactive).
kind_name_obs_record_toggle_pause = Toggle Recording Pause
kind_summary_obs_record_toggle_pause = Toggles the pause state of the OBS recording output.
kind_name_obs_replay_save = Save Replay Buffer
kind_summary_obs_replay_save = Saves the current contents of the OBS replay buffer to disk.
kind_name_obs_replay_set_active = Set Replay Buffer Active
kind_summary_obs_replay_set_active = Starts or stops the OBS replay buffer to an explicit state.
kind_name_obs_replay_start = Start Replay Buffer
kind_summary_obs_replay_start = Starts the OBS replay buffer output.
kind_name_obs_replay_stop = Stop Replay Buffer
kind_summary_obs_replay_stop = Stops the OBS replay buffer output.
kind_name_obs_scene_collection_switch = Switch Scene Collection
kind_summary_obs_scene_collection_switch = Switches OBS to a different scene collection by name.
kind_name_obs_scenes_created = OBS scene created
kind_summary_obs_scenes_created = Fires when a new scene is created in OBS.
kind_name_obs_scenes_current_changed = OBS scene changed
kind_summary_obs_scenes_current_changed = Fires when the current OBS program scene changes.
kind_name_obs_scenes_get_list = Get Scene List
kind_summary_obs_scenes_get_list = Queries OBS for all scenes and the current program scene.
kind_name_obs_scenes_list_changed = OBS scene list changed
kind_summary_obs_scenes_list_changed = Fires when scenes are added, removed, or reordered in OBS.
kind_name_obs_scenes_preview_changed = OBS preview scene changed
kind_summary_obs_scenes_preview_changed = Fires when the Studio Mode preview scene changes.
kind_name_obs_scenes_removed = OBS scene removed
kind_summary_obs_scenes_removed = Fires when a scene is deleted in OBS.
kind_name_obs_scenes_renamed = OBS scene renamed
kind_summary_obs_scenes_renamed = Fires when a scene is renamed in OBS.
kind_name_obs_scenes_set_preview = Set Preview Scene
kind_summary_obs_scenes_set_preview = Sets the current preview scene in OBS Studio Mode.
kind_name_obs_scenes_set_transition = Set Scene Transition
kind_summary_obs_scenes_set_transition = Sets the active scene transition by name.
kind_name_obs_scenes_switch_current = Switch Scene
kind_summary_obs_scenes_switch_current = Sets the current OBS program scene.
kind_name_obs_sources_get_input_settings = Get Input Settings
kind_summary_obs_sources_get_input_settings = Queries OBS for the current settings object of a named input source.
kind_name_obs_sources_get_list = Get Source/Input List
kind_summary_obs_sources_get_list = Queries OBS for all inputs (sources) by name.
kind_name_obs_sources_input_created = OBS input source created
kind_summary_obs_sources_input_created = Fires when a new input source is added to OBS.
kind_name_obs_sources_input_removed = OBS input source removed
kind_summary_obs_sources_input_removed = Fires when an input source is removed from OBS.
kind_name_obs_sources_input_renamed = OBS input source renamed
kind_summary_obs_sources_input_renamed = Fires when an input source is renamed in OBS.
kind_name_obs_sources_scene_item_created = OBS scene item added
kind_summary_obs_sources_scene_item_created = Fires when a source is added to a scene in OBS.
kind_name_obs_sources_scene_item_lock_changed = OBS scene item lock changed
kind_summary_obs_sources_scene_item_lock_changed = Fires when a scene item's lock state is toggled in OBS.
kind_name_obs_sources_scene_item_removed = OBS scene item removed
kind_summary_obs_sources_scene_item_removed = Fires when a source is removed from a scene in OBS.
kind_name_obs_sources_scene_item_visibility_changed = OBS scene item visibility changed
kind_summary_obs_sources_scene_item_visibility_changed = Fires when a scene item's visibility is toggled in OBS.
kind_name_obs_sources_set_input_settings = Set Input Settings
kind_summary_obs_sources_set_input_settings = Applies a JSON settings object to an OBS input source.
kind_name_obs_sources_set_locked = Set Source Locked
kind_summary_obs_sources_set_locked = Locks or unlocks a source within an OBS scene.
kind_name_obs_sources_set_visible = Set Source Visible
kind_summary_obs_sources_set_visible = Shows or hides a source within an OBS scene.
kind_name_obs_stream_get_status = Get Streaming Status
kind_summary_obs_stream_get_status = Queries OBS for the current stream output state and elapsed duration.
kind_name_obs_stream_send_caption = Send Stream Caption
kind_summary_obs_stream_send_caption = Sends CEA-608 caption text over the OBS stream output.
kind_name_obs_stream_set_active = Set Stream Active
kind_summary_obs_stream_set_active = Starts or stops the OBS stream output to an explicit state.
kind_name_obs_stream_start = Start Stream
kind_summary_obs_stream_start = Starts OBS streaming output.
kind_name_obs_stream_started = OBS stream started
kind_summary_obs_stream_started = Fires when OBS stream output becomes active and data is flowing.
kind_name_obs_stream_starting = OBS stream starting
kind_summary_obs_stream_starting = Fires when OBS begins the stream start sequence (before output is active).
kind_name_obs_stream_status_changed = OBS stream status changed
kind_summary_obs_stream_status_changed = Fires on any stream output state transition (starting, started, stopping, stopped, reconnecting, reconnected).
kind_name_obs_stream_stop = Stop Stream
kind_summary_obs_stream_stop = Stops OBS streaming output.
kind_name_obs_stream_stopped = OBS stream stopped
kind_summary_obs_stream_stopped = Fires when OBS stream output has fully stopped.
kind_name_obs_stream_stopping = OBS stream stopping
kind_summary_obs_stream_stopping = Fires when OBS begins the stream stop sequence (before output goes idle).
kind_name_obs_studio_disable = Disable Studio Mode
kind_summary_obs_studio_disable = Disables OBS studio mode.
kind_name_obs_studio_disabled = OBS Studio Mode disabled
kind_summary_obs_studio_disabled = Fires when OBS Studio Mode is turned off.
kind_name_obs_studio_enable = Enable Studio Mode
kind_summary_obs_studio_enable = Enables OBS studio mode.
kind_name_obs_studio_enabled = OBS Studio Mode enabled
kind_summary_obs_studio_enabled = Fires when OBS Studio Mode is turned on.
kind_name_obs_studio_set_enabled = Set Studio Mode
kind_summary_obs_studio_set_enabled = Sets OBS studio mode to an explicit on or off state.
kind_name_obs_studio_trigger_transition = Trigger Studio Transition
kind_summary_obs_studio_trigger_transition = Triggers the current transition in OBS studio mode, moving preview to program.
kind_name_obs_transition_ended = OBS scene transition ended
kind_summary_obs_transition_ended = Fires when a scene transition completes in OBS (cut point; video may still play).
kind_name_obs_transition_started = OBS scene transition started
kind_summary_obs_transition_started = Fires when a scene transition begins in OBS.
kind_name_obs_transition_video_ended = OBS transition video ended
kind_summary_obs_transition_video_ended = Fires when a transition's video playback fully completes (stinger video end, not just the cut point).
kind_name_obs_virtualcam_set_active = Set Virtual Camera Active
kind_summary_obs_virtualcam_set_active = Starts or stops the OBS virtual camera output to an explicit state.
kind_name_obs_virtualcam_started = OBS virtual camera started
kind_summary_obs_virtualcam_started = Fires when the OBS virtual camera output becomes active.
kind_name_obs_virtualcam_state_changed = OBS virtual camera state changed
kind_summary_obs_virtualcam_state_changed = Fires on any virtual camera state transition (starting, started, stopping, stopped).
kind_name_obs_virtualcam_stopped = OBS virtual camera stopped
kind_summary_obs_virtualcam_stopped = Fires when the OBS virtual camera output has fully stopped.
