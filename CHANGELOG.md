# Changelog
All notable changes to this project will be documented in this file.

## [0.1.0-beta.1] - 2026-05-31
### ⚙️ Miscellaneous Tasks
- *(workspace)* Regenerate Cargo.lock for forge-platform-youtube

### 🐛 Bug Fixes
- *(platform-core)* Gate APP_DIR_NAME on non-macos targets
- Track new modules from earlier SRP refactor commits

### 📚 Documentation
- Add YouTube platform guide + README beta-1 mention

### 🚀 Features
- *(events)* Add EventSource::to_platform_id helper
- *(widgets)* Add platform color tokens to ForgePalette
- *(types)* Add PlatformScope and trigger_instances migration
- *(platform-youtube)* Scaffold crate with DeviceCode AuthFlow
- *(registry)* Add KindPlatformContract to descriptor trait
- *(runtime)* Add platform scope guard to TriggerEvaluator
- *(types)* Add UnifiedChatRow and supporting types
- *(runtime)* Add ChatPayload envelope and chat_stream bus bridge
- *(platform-youtube)* Implement GoogleAuthFlow device-code polling
- *(platform-twitch)* Emit _chat payload on chat and events
- *(app)* Add platform-scope picker to trigger editor
- *(platform-youtube)* Add credentials manager with on-demand refresh
- *(platform-youtube)* Add live chat polling with quota guard
- *(platform-youtube)* Register 10 YouTube trigger descriptors
- *(app)* Add DeviceCodeFlow screen for OAuth completion
- *(app)* Refactor LiveChat to render UnifiedChatRow
- *(runtime)* Wire YouTube credentials + chat poller + triggers
- *(app)* Wire Settings platforms to DeviceCodeFlow screen
- *(platform-youtube)* Implement send-chat with liveChatId tracking

### 🚜 Refactor
- *(types)* Promote PlatformId to forge-types
- *(platform-youtube)* Split chat_poller per SRP
- *(app)* Extract LiveChat view into separate module

### 🛠️ Build
- Pass YouTube OAuth credentials to CI builds

## [0.1.0-alpha.16] - 2026-05-29
### ⚠️ BREAKING CHANGES
- **general**: DataProvider::command_repo() removed;
Command/CommandId/CommandPermission/CommandRepo/CommandParser deleted;
migration 0013 drops the commands table

### ⚙️ Miscellaneous Tasks
- *(workspace)* Remove tautological comments
- Release

### 🐛 Bug Fixes
- *(ci)* Install libasound2-dev on Linux for cpal/alsa-sys builds
- *(storage)* Enable foreign_keys pragma on SqliteConnectOptions
- *(app)* Render trigger picker overlay on Actions list page
- *(app)* Make trigger picker assign refresh detail and surface custom vs default
- *(ui)* Click on trigger now moves you to editor

### 📚 Documentation
- *(release)* Release v0.1.0-alpha.16

### 🚀 Features
- *(storage)* Add migration 0012 for trigger_instances and action_trigger_instances tables
- *(types)* Add TriggerInstance and TriggerInstanceId types
- *(storage)* Add TriggerInstanceRepo trait and ReferenceBlock error variant
- *(storage)* Implement SqliteTriggerInstanceRepo and wire DataProvider accessor
- *(registry)* Add effective_config merge helper for Template/Patch model
- *(app)* Wire TriggerRegistry into boot with default-instance upsert
- *(runtime)* Switch TriggerEvaluator to TriggerInstance with effective_config merge
- *(runtime)* Apply effective_config to sub-action steps before execute
- *(app)* Scaffold Triggers Registry screen with empty state
- *(storage)* Add list_all method to TriggerInstanceRepo
- *(app)* Implement Triggers Registry list, filters, side-sheet, and confirm-disable dialog
- *(app)* Remove legacy Commands UI surface
- *(storage)* Add link_action and unlink_action to TriggerInstanceRepo
- *(app)* Migrate ActionDetail from Trigger to TriggerInstance and rewire add-trigger flow
- *(app)* Replace add-trigger modal with 3-Level Picker for Actions side-sheet
- [**breaking**] Remove legacy Commands subsystem across all layers
- *(storage)* Remove legacy Trigger struct and TriggerRepo across all layers
- *(app)* Wire cross-navigation between Actions and Triggers Registry
- *(app)* Add Create Trigger Instance form on Triggers Registry page

### 🧪 Testing
- *(runtime)* Add e2e smoke for Template/Patch trigger and sub-action effective_config

## [0.1.0-alpha.15] - 2026-05-27
### ⚙️ Miscellaneous Tasks
- *(deps)* Bump actions/download-artifact from 7 to 8 (#16)
- *(deps)* Bump actions/upload-artifact from 6 to 7 (#17)
- *(deps)* Bump serde_json from 1.0.149 to 1.0.150 (#18)
- *(deps)* Bump tower-http from 0.6.10 to 0.6.11 (#19)
- *(deps)* Bump sqlx from 0.8.6 to 0.9.0 (#20)
- *(deps)* Bump rhai from 1.24.0 to 1.25.0 (#21)
- Release

### 🐛 Bug Fixes
- *(widgets)* Batch SideSheet layout invalidation
- *(widgets)* Render menu_button panel as overlay, not inline column
- *(app)* Remove redundant menu dismiss wrapper around drawer panel
- *(storage-sqlite)* Adapt query_as calls to sqlx 0.9 SqlSafeStr
- *(twitch)* Migrate builtin quick action templates to SubActionStep
- *(widgets)* Migrate quick action test fixtures to SubActionStep
- *(server)* Migrate test fixtures and spawn_action_engine to registry-based dispatch
- *(storage)* Migrate trigger and action tests to kind_id and SubActionStep

### 📚 Documentation
- *(release)* Release v0.1.0-alpha.15

### 🚀 Features
- *(widgets)* Add SideSheet widget skeleton with overlay layout
- *(storage)* Add sheet_width typed accessors to SettingsRepo
- *(widgets)* Animate SideSheet open/close with eased x-offset
- *(widgets)* Add drag-to-resize handle to SideSheet
- *(registry)* Scaffold forge-registry crate with descriptor and runner traits
- *(twitch)* Implement TriggerKindDescriptor for 7 trigger kinds
- *(audio)* Implement SubActionRunner for soundboard.sound.play and tts.speak.text
- *(obs)* Implement TriggerKindDescriptor and 8 SubActionRunner kinds
- *(runtime)* Migrate sub-actions and triggers to registry-based dispatch
- *(storage)* Migrate trigger and sub-action rows from enum JSON to kind_id format

### 🚜 Refactor
- *(app)* Extract home view fns from app.rs to home.rs
- *(app)* Extract settings view fns to settings.rs
- *(app)* Extract platforms overview view to platforms_view.rs
- *(app)* Extract actions view fns to actions_view.rs
- *(app)* Extract action modal views to actions_modals.rs
- *(app)* Extract navigation helpers to navigation.rs
- *(app)* Extract subscription wiring to subscriptions.rs
- *(app)* Extract tts section view fns to tts_view.rs
- *(app)* Extract boot async helpers to boot.rs
- *(app)* Extract view screen-dispatch router to view_router.rs
- *(app)* Extract page-chrome helpers to page_chrome module
- *(app)* Move boot/server result handlers to boot.rs
- *(app)* Extract Settings message handler to settings.rs
- *(app)* Extract navigate arm to navigation::handle_navigate
- *(app)* Extract viewer-drawer panel to live_chat_drawer.rs
- *(app)* Migrate viewer drawer to SideSheet widget
- *(widgets)* Drop legacy side_sheet fn and sheet_chrome chrome
- *(app)* Split actions.rs into trigger_kinds/forms/telemetry
- *(app)* Split action_editor.rs into view and update layers
- *(widgets)* Split builtin into header/health/quick_actions
- *(app)* Extract globals_variant_editor module from globals_view
- *(actions)* Extract ActionsService; UI dispatches via Tasks
- *(tests)* Introduce mockall
- *(storage)* DataProvider accessors return Arc<dyn XRepo>
- *(app)* Narrow DataProvider args to specific repos
- *(twitch)* Move credential format from forge-app
- *(runtime)* Extract dashboard::compute_stats from forge-app
- *(twitch)* Move EventSub parsers from forge-app
- *(script)* Move script execution from forge-app to forge-script
- *(app)* Nest boot/subsystem variants under Message envelopes
- *(widgets)* Split server.rs into per-widget files
- *(server)* Split protocol/mod.rs into 5 function families
- *(storage)* Split SettingsRepo into KV + typed configs
- *(runtime)* Narrow command-parser and trigger-evaluator services to per-repo Arcs
- *(server)* Narrow DispatchContext to 4 repo Arcs
- *(runtime)* Narrow 7 sub-action handlers from DataProvider to GlobalsRepo
- *(script)* Narrow ForgeApi and run_inline to Arc<dyn GlobalsRepo>
- *(runtime)* Narrow ActionEngine to ActionRepo+HistoryRepo+GlobalsRepo
- *(runtime)* Narrow ActionsService to per-repo Arcs
- *(server)* Narrow AppState and ServerConfig to 4 repo Arcs
- *(app)* Narrow boot to Arc<dyn DataProvider>, drop SqliteBackend _arc helpers
- *(types)* Swap TriggerKind/SubActionSpec enums for kind_id + SubActionStep
- *(ui)* Migrate forge-app to kind_id strings and SubActionStep
- *(obs)* Extract store_and_connect orchestration to forge-obs::credentials
- *(app)* Extract twitch reauth handler from central update

## [0.1.0-alpha.14] - 2026-05-24
### ⚙️ Miscellaneous Tasks
- Release

### ⚡ Performance
- *(chat)* Cache rendered rows with iced::widget::lazy

### 🎨 Styling
- *(widgets)* Bump inline pill padding from Xxs to Xs

### 🐛 Bug Fixes
- *(widgets)* Request redraw on chat username hover state change

### 📚 Documentation
- *(release)* Release v0.1.0-alpha.14

### 🚀 Features
- *(widgets)* Add ChatRowWidget skeleton implementing iced Widget trait
- *(widgets)* Implement ChatRowWidget layout, draw, and update
- *(widgets)* Add dotted underline on chat username hover

### 🚜 Refactor
- *(app)* Introduce RuntimeView and move services to App::rt
- *(app)* Wrap EventArrived payload in Arc
- *(globals)* Migrate globals_view to RuntimeView contract
- *(queues)* Migrate queues_view to RuntimeView contract
- *(feed)* Migrate event_feed to RuntimeView contract
- *(commands)* Migrate commands_view to RuntimeView contract
- *(soundboard)* Migrate soundboard to RuntimeView contract
- *(server)* Migrate server_screen to RuntimeView contract
- *(settings)* Migrate settings_audio to RuntimeView contract
- *(settings)* Migrate settings_websocket to RuntimeView contract
- *(home)* Migrate home to RuntimeView contract
- *(tts-view)* Migrate tts_dashboard to RuntimeView contract
- *(tts-view)* Migrate tts_engines to RuntimeView contract
- *(tts-view)* Migrate tts_filters to RuntimeView contract
- *(tts-view)* Migrate tts_triggers to RuntimeView contract
- *(tts-view)* Migrate voice_aliases to RuntimeView contract
- *(editor)* Migrate script_editor to RuntimeView contract
- *(viewers)* Migrate viewers to RuntimeView contract
- *(workspace)* Rename Integration domain to Builtin
- *(builtin)* Migrate builtin_detail to RuntimeView contract
- *(chat)* Migrate live_chat to RuntimeView contract
- *(actions)* Migrate actions list handler to RuntimeView contract
- *(actions)* Migrate add_action handler into action_editor
- *(actions)* Migrate add_trigger handler to action_editor
- *(actions)* Migrate three sub-action handlers to action_editor
- *(obs)* Move obs_panel handler into module
- *(twitch)* Move twitch_panel handler into module
- *(tts-view)* Inline TTS sub-router into central match
- *(app)* Fan out EventArrived through per-screen on_event
- *(actions)* Nest 5 editor message variants under Actions::Editor
- *(app)* Group per-screen states under app ui struct
- *(app)* Depend on DataProvider trait instead of SqliteBackend
- *(chat)* Publish chat.send.request to bus instead of inline send
- *(widgets)* Convert chat_row API to owned data
- *(widgets)* Apply Spacing tokens to internal paddings
- *(app)* Apply Spacing tokens across screen paddings
- *(widgets)* Switch to ChatRowWidget, delete old chat_row API

## [0.1.0-alpha.13] - 2026-05-22
### ⚙️ Miscellaneous Tasks
- *(widgets)* Drop _v2 naming and remove dead sidebar code
- *(app)* Drop standalone Viewers screen now folded into chat drawer
- *(widgets)* Drop onboarding module and move banner to sections
- *(app)* Drop Settings::Platforms subsection (duplicates top-level)
- Release

### 🎨 Styling
- *(chat)* Highlight usernames on mouse hover
- *(widgets)* Input bar icons and styling
- *(widgets)* Use 4-step font sizes
- *(app)* Rebuild Stream apps overview
- *(app)* Rewrite TTS preview as per-stage cards
- *(actions)* Update actions styling
- *(ui)* Align border/radius tokens and integrate live home screen telemetry

### 🐛 Bug Fixes
- *(widgets)* Vertically center title bar content
- *(widgets)* Vertically center app footer content
- *(widgets)* Anchor sidebar Settings to bottom of viewport
- *(chat)* Use 2-row layout with row separator between messages
- *(livechat)* Align viewer drawer with design and wire username clicks
- *(chat)* Widen viewers drawer to 360px
- *(chat)* Bind drawer detail to clicked viewer with chat fallback
- *(chat)* Pad drawer action buttons so icons clear border
- *(widgets)* Replace unicode glyphs with tabler icons
- *(app)* Replace unicode glyphs with tabler icons
- *(app)* Flatten Platforms and Stream apps in sidebar per design
- *(app)* Align telemetry grid box size with empty placeholders
- *(app)* Show only action count right-aligned in tree group header
- *(app)* Match trigger and sub-action row size with placeholder
- *(app)* Add inline add buttons to triggers and sub-actions headers
- *(app)* Use icon-prefixed ghost buttons for Test run and Duplicate
- *(chat)* Replace meta/filter bars with single page header
- *(home)* Replace global crumb_bar with screen page header

### 📚 Documentation
- *(widgets)* Strip tautological rustdoc from token definitions

### 🚀 Features
- *(widgets)* Add ModalSize design-token enum with 3 widths
- *(widgets)* Add semantic-color alpha helpers to palette
- *(widgets)* Add toast queue, viewport widget, and app wiring
- *(widgets)* Add popover MenuButton and RowActions widgets
- *(chat)* Rebuild layout and add 5 typed chat row renderers
- *(livechat)* Add viewers drawer panel with detail card and list
- *(widgets)* Add username click callback to chat row builders
- *(widgets)* Add hover affordance to chat username and ghost button
- *(widgets)* Switch search_input to Tabler icon and tighten padding
- *(widgets)* Add big_jump_card builder for Home dashboard
- *(app)* Rewrite Home screen and align font sizes to token scale
- *(storage)* Add action telemetry query and execution log
- *(app)* Add action telemetry grid, type filter chips, tree row hover
- *(app)* Add sub-action reorder controls and colored variable display
- *(widgets)* Add external-link icon to complex value preview
- *(app)* Add Platforms overview screen with 2x2 platform cards
- *(app)* Add generic platform/app placeholder for beta-N integrations
- *(app)* Add Commands screen and Settings Language/Shortcuts panes
- *(app)* Inline rename and verious styling
- *(widgets)* Add side_sheet overlay widget
- *(app)* Unify page headers across screens and migrate to side sheets
- *(chat)* Adds emoji and real viewer data

### 🚜 Refactor
- *(widgets)* Compress Spacing enum to 4 design-token variants
- *(widgets)* Compress font constants to 4 design tokens
- *(widgets)* Compress Radius enum to 3 design-token variants
- *(widgets)* Redesign title bar with logo, profile name, version
- *(app)* Reorganize sidebar with section labels per new design
- *(widgets)* Remove version label from title bar
- *(widgets)* Drop version and profile labels from title bar
- *(app)* Uppercase sidebar section labels at source
- *(widgets)* Migrate icons from Bootstrap font to Tabler SVG

## [0.1.0-alpha.12] - 2026-05-21
### ⚙️ Miscellaneous Tasks
- Release

### ⚡ Performance
- *(audio)* Cache cpal device enumeration with 5s TTL and explicit refresh

### 📚 Documentation
- *(readme)* Refresh current status for alpha-12 release
- *(release)* Release v0.1.0-alpha.12

### 🚀 Features
- *(scripts)* Add Variant::Datetime sub-actions ReadFile, RandomInt and forge::time rhai
- *(app)* Add ReadFile and RandomInt to Add SubAction picker
- *(app)* Add Viewers screen with ViewerRepo and platform tracking
- *(app)* Add Viewers screen Settings sub-screens and viewer tracker
- *(app)* Wire Hub hero uptime and title bar 8-subsystem counter
- *(app)* Wire Event Feed export and Actions duplicate button
- *(app)* Retro polish across actions twitch and event feed

### 🚜 Refactor
- *(app)* Rename Hub to Home across messages widgets and state

## [0.1.0-alpha.11] - 2026-05-21
### Deps
- *(libs)* Bump libraries versions

### ⚙️ Miscellaneous Tasks
- *(workspace)* Sync Cargo.lock for tts crate deps
- Release

### 🐛 Bug Fixes
- *(audio)* Adapt to rubato 3.0 and symphonia 0.6 APIs after bump
- *(app)* Register Piper engine and wire CpalSink in speak queue boot
- *(app)* Register Piper engine and wire CpalSink in speak queue boot
- *(ci)* Add libasound2-dev apt dep for cpal Linux build

### 📚 Documentation
- *(release)* Release v0.1.0-alpha.11

### 🚀 Features
- *(tts-core)* Create forge-tts-core crate with TtsEngine trait
- *(voice)* Create forge-voice crate with VoiceAlias and resolver
- *(tts-pipeline)* Create forge-tts-pipeline crate skeleton
- *(tts-piper)* Create forge-tts-piper crate skeleton
- *(speak-queue)* Create forge-speak-queue crate skeleton
- *(storage)* Add VoiceAliasRepo trait and StoredVoiceAlias
- *(storage-sqlite)* Add migration 0007 voice_aliases and repo impl
- *(voice)* Implement VoiceAliasResolver with three strategies
- *(tts-pipeline)* Implement 5-stage process and preview API
- *(tts-piper)* Implement subprocess synthesis and voice scanner
- *(speak-queue)* Implement tokio actor with verbs and bus events
- *(storage)* Add TTS settings keys and pipeline config helpers
- *(types,runtime)* Add SubActionSpec::Speak and SpeakDispatcher
- *(speak-queue)* Expose subscribe() on SpeakQueueHandle
- *(app)* Add TtsSection enum and parameterize Screen::Tts
- *(tts-ui)* Add TTS message types and module declarations
- *(tts-ui)* Add TTS dashboard, engines, filters, triggers, voice aliases screens
- *(tts-ui)* Wire TTS state into App and add tts_section_view routing
- *(tts-ui)* Wire speak queue, SpeakBridge, and SpeakEvent subscription
- *(tts-ui)* Add Speak sub-action to action editor
- *(script)* Wire forge::tts rhai module via SpeakRequester trait

### 🧪 Testing
- *(voice)* Add resolver_deterministic regression tests
- *(voice)* Add resolver_blocked regression tests
- *(voice)* Add resolver_ignore_profile regression tests
- *(tts-pipeline)* Add pipeline_full_flow regression tests
- *(tts-pipeline)* Add pipeline_preview regression tests
- *(speak-queue)* Add queue_per_user_limit regression tests
- *(speak-queue)* Add queue_pause_resume regression tests
- *(speak-queue)* Add queue_priority regression tests
- *(tts-piper)* Add voice_scan regression tests
- *(runtime)* Add speak_dispatch regression tests

## [0.1.0-alpha.10] - 2026-05-21
### ⚙️ Miscellaneous Tasks
- *(workspace)* Update Cargo.lock for new audio deps
- *(workspace)* Update Cargo.lock for forge-soundboard runtime dep
- *(workspace)* Sync Cargo.lock for forge-audio in forge-app deps
- Release

### 🎨 Styling
- *(tests)* Apply rustfmt to alpha-10 regression tests

### 🐛 Bug Fixes
- *(soundboard)* Emit PlaybackFailed on decode and sink-factory errors
- *(app)* Wire BusAudioEventSink so audio events reach the event bus

### 📚 Documentation
- *(readme)* Refresh current status for alpha-10 audio milestone

### 🚀 Features
- *(events)* Add Audio variant to EventSource enum
- *(types)* Add PlaySound SubAction variant with ClipId and OutputDevice
- *(audio)* Create forge-audio crate with AudioSink trait and cpal device discovery
- *(soundboard)* Create forge-soundboard crate with clip schema
- *(storage)* Add SoundboardClipsRepo trait and StoredClip
- *(storage-sqlite)* Add migration 0006 soundboard_clips and repo impl
- *(audio)* Add crossbeam-channel and futures to workspace deps
- *(audio)* Add AudioEvent enum and AudioEventSink trait
- *(audio)* Add convert module with rubato resampler and channel remix
- *(audio)* Add fan_out helper for multi-sink concurrent playback
- *(audio)* Add symphonia decoder for clip files
- *(audio)* Add CpalSink with bounded ring buffer playback
- *(soundboard)* Add AudioSinkFactory trait and CpalSinkFactory
- *(runtime)* Add SoundPlayer trait for soundboard integration
- *(soundboard)* Add BusAudioEventSink and SoundboardPlayer with tests
- *(runtime)* Wire SoundPlayer into PlaySound runner and spawn_action_engine
- *(widgets)* Add volume_slider widget
- *(widgets)* Add output_device_picker widget
- *(widgets)* Add clip_card widget
- *(app)* Add Soundboard screen with grid layout and add-clip modal
- *(app)* Add in-app hotkey subscription for Soundboard
- *(app)* Add Settings Audio sub-screen with device test-tone
- *(app)* Instantiate SoundboardPlayer at boot and wire to App
- *(app)* Wire Soundboard sidebar navigation and screen routing
- *(app)* Add PlaySound option to Add SubAction picker

### 🧪 Testing
- *(audio)* Add convert_correctness and decode_smoke integration tests
- *(soundboard)* Add player_emits_events and player_emits_failed regression tests
- *(runtime)* Add play_sound_dispatch integration tests via action engine

## [0.1.0-alpha.9] - 2026-05-20
### ⚠️ BREAKING CHANGES
- **runtime**: EventLogRepo gains recent_since() method
- **server**: ServerConfig::new now requires data_provider argument
- **server**: TriggerKind gains CodeEvent variant; exhaustive matches must handle it

### ⚙️ Miscellaneous Tasks
- Release

### 🐛 Bug Fixes
- *(app)* Hold tokio runtime guard so background spawns don't panic
- *(app)* Handle tokio runtime init error without panicking
- *(app)* Make sidebar Actions Twitch and OBS items reach real screens
- *(app)* Wire device code copy button and verification URL open
- *(widgets)* Pad onboarding step pills so text doesn't crowd the edges
- *(platform-core)* Surface Twitch device-code response body in errors
- *(storage-sqlite)* Ignore missing migrations to survive binary downgrade
- *(app)* Tighten Home empty card height and add stat row padding
- *(app)* Even Home bottom cards width and zero-default At a glance counts
- *(app)* Always route sidebar Twitch and OBS to IntegrationDetail
- *(platform-core)* Accept Twitch token error format with message field
- *(platform-core)* Accept Twitch scope field as either string or array
- *(storage-sqlite)* Hex-encode digest by byte for sha2 0.11 compat
- *(workspace)* Enable rustls TLS feature on tokio-tungstenite for EventSub
- *(widgets)* Map kebab-case icon names to bundled bootstrap codepoints
- *(platform-twitch)* Align scopes and quick actions with Twitch design
- *(platform-twitch)* Stop chat reconnect loop when token scope is missing

### 📚 Documentation
- *(readme)* Refresh current status for alpha-9 server milestone
- *(release)* Release v0.1.0-alpha.9

### 🚀 Features
- *(server)* Add axum router skeleton with ws/api/overlays routes
- *(storage)* Add 4 server config settings keys
- *(runtime)* [**breaking**] Add EventBus::recent_since with DB fallback
- *(server)* Add bearer-token auth middleware for /api/v1
- *(server)* Add bus-adapter with per-client filter dispatch
- *(server)* Add WsClient state with per-client ev/s tracking
- *(widgets)* Add throughput_sparkline canvas widget
- *(widgets)* Add bearer_token_display widget with mask and reveal
- *(server)* [**breaking**] Add WS protocol envelope and method dispatcher base
- *(server)* Implement subscribe and unsubscribe WS methods
- *(widgets)* Add client_table_row with 3-state dot and chips
- *(widgets)* Add bind_address_card radio widget
- *(server)* Implement getInfo with clients and bandwidth telemetry
- *(server)* Implement getActions and doAction WS methods
- *(widgets)* Add type_to_confirm_modal blocking widget
- *(widgets)* Add overlay_file_list with sizes and browser URL
- *(server)* Implement getCommands, getGlobals, getGlobal, setGlobal
- *(server)* [**breaking**] Implement getUserGlobals and triggerCodeEvent methods
- *(app)* Add Server screen with stats clients and overlay list
- *(server)* Implement getEvents and replayEvent WS methods
- *(app)* Add Settings WebSocket sub-screen with LAN-bind modal
- *(server)* Implement WS authenticate first-frame method
- *(server)* Implement getActiveViewers WS method
- *(server)* Implement getOverlayFiles WS method with sandbox
- *(server)* Add HTTP REST mirror at /api/v1
- *(server)* Serve overlay files with sandbox and CORS toggle
- *(server)* Notify clients of dropped events on backpressure lag
- *(server)* Add ServerHandle stop and restart lifecycle
- *(server)* Guard against unsafe LAN bind without token or flag
- *(server)* Expose auth_state and bind_addr getters on ServerHandle
- *(app)* Add server_subsystem wrapping ServerHandle lifecycle
- *(app)* Autostart server from settings on app boot
- *(app)* Wire server screen restart stop and regenerate buttons
- *(app)* Auto-restart server when WebSocket settings change
- *(app)* Subscribe to live server metrics when Server screen visible
- *(app)* Write tracing logs to daily-rotated files alongside console
- *(widgets)* Bundle Inter and JetBrains Mono so UI looks the same everywhere
- *(app)* Redesign Home empty state with dimmed values and create CTAs
- *(app)* Add Twitch disconnected inline panel with device code mockup
- *(app)* Wire Twitch device code flow polling and credential storage
- *(app)* Reconnect Twitch chat from stored credentials on app boot
- *(app)* Show Twitch connected panel with login when chat session is live
- *(app)* Add OBS disconnected inline panel with form and test connect
- *(platform-twitch)* Impl 4 integration traits for full connected view
- *(app)* Redesign Actions list as grouped table with filters and stats
- *(app)* Add Queues management screen with cards and pause controls
- *(app)* Show re-auth banner when Twitch token scope is missing
- *(platform-twitch)* Subscribe to all EventSub topics with shared tracker
- *(storage)* Aggregate last_ran and runs_24h via HistoryRepo stats_summary
- *(app)* Add Action editor screen with 2-pane tree and sub-action flow
- *(app)* Seed Hub triggers_fired from 24h history and increment on action.done
- *(runtime)* Expose paused_queues query and feed Queues view live state
- *(app)* Treat Drain as pause plus queue.drain_requested bus event
- *(server)* Track http_requests and events_out counters in ServerInfo
- *(app)* Surface live WS client subscriptions in Server screen rows
- *(app)* Scan overlay root from disk and feed Server screen file list
- *(app)* Replace active bool with 3-state liveness on Server client rows
- *(app)* Load Settings WebSocket state from storage and apply overlay config to server
- *(app)* Add hover affordance to Server connected-clients rows
- *(app)* Add kick-to-disconnect hint to Server clients header

### 🚜 Refactor
- *(app)* Remove Onboarding screen and route boot directly to Home
- *(platform-twitch)* Replace custom DCF with twitch_api and twitch_oauth2
- *(widgets)* Rewrite IntegrationDetail layout to match HTML designs
- *(platform-core)* Remove dead custom OAuth code now handled by twitch_oauth2

## [0.1.0-alpha.8] - 2026-05-19
### ⚠️ BREAKING CHANGES
- **storage**: DataProvider gains event_log_repo() method
- **runtime**: EventBus::replay renamed to lookup; replay name reserved for replay_and_publish
- **runtime**: EventBus::new now requires Arc<dyn EventLogRepo>
- **runtime**: none

### ⚙️ Miscellaneous Tasks
- *(workspace)* Update Cargo.lock
- *(deps)* Update lockfile for forge-widgets forge-events dep
- *(workspace)* Update Cargo.lock and rustfmt cleanup
- Release

### 🐛 Bug Fixes
- *(obs)* Align event payloads with RFC-031 and populate caused_by
- *(twitch)* Align observability events with RFC-031 audit
- *(runtime)* [**breaking**] Populate caused_by across full event causation chain
- *(twitch)* Use char-boundary truncation for body_snippet
- *(widgets)* Cap json_viewer recursion depth to prevent stack overflow

### 📚 Documentation
- *(readme)* Remove Twitch client_id manual-setup limitation
- *(readme)* Refresh current status for alpha-8 EventFeed milestone
- *(release)* Release v0.1.0-alpha.8

### 🚀 Features
- *(events)* Add Event::replay flag for replay_and_publish
- *(storage)* [**breaking**] Add EventLogRepo trait and DataProvider extension
- *(storage-sqlite)* Add migration 0005 and EventLogRepo impl
- *(storage)* Add event_log_retention_days settings key
- *(storage-sqlite)* Add background event_log retention pruning task
- *(widgets)* Add source_badge widget with EventSource colors
- *(runtime)* [**breaking**] Add EventBus::replay_and_publish with DB fallback
- *(runtime)* Add ring buffer flush task for event_log persistence
- *(widgets)* Add event_row_observability widget
- *(widgets)* Add causation_chip widget
- *(widgets)* Add json_viewer widget with syntax colors
- *(runtime)* Emit global.set/incr/del observability events per RFC-031
- *(widgets)* Add replay_button widget
- *(runtime)* Emit queue and cooldown observability events
- *(widgets)* Add event_inspector composite widget
- *(script)* Emit script.exec and script.error events per RFC-031
- *(app)* Add EventFeed screen with filters and replay flow
- *(app)* Wire real EventLogRepo into EventBus at boot
- *(app)* Rewire TestTrigger to use EventBus::replay_and_publish

### 🚜 Refactor
- *(runtime)* [**breaking**] Rename EventBus::replay to lookup

### 🧪 Testing
- *(runtime)* Add replay correctness regression tests

## [0.1.0-alpha.7] - 2026-05-19
### ⚠️ BREAKING CHANGES
- **platform-core**: IntegrationStatus requires capability_flags() and header_actions()
- **platform-core**: HealthMetric loses sublabel/color; HealthValue replaces; IntegrationHealth adds stream()
- **types**: ExecutionContext.trigger_event_id replaced by metadata: ExecutionMetadata
- **platform-core**: IntegrationCatalog removed; replaced by IntegrationContent + DetailSection enum (8 variants)
- **platform-core**: QuickAction.payload removed; enabled, subaction_template, picker added; PickerKind enum with Copy+Hash added
- **obs**: ObsClient::connect now requires Arc<dyn EventPublisher> parameter
- **runtime**: ActionEngine now requires Option<Arc<dyn ObsSink>> as fourth argument to spawn_action_engine
- **types**: TriggerKind gains ObsSceneChanged variant; exhaustive matches must add the arm

### ⚙️ Miscellaneous Tasks
- *(workspace)* Update Cargo.lock
- *(deps)* Add futures-core to forge-platform-core lockfile
- *(deps)* Update Cargo.lock for forge-events dep in forge-obs
- Release

### 🐛 Bug Fixes
- *(obs)* Align scene event kinds with EventSource::Obs taxonomy

### 📚 Documentation
- *(readme)* Add dynamic release and activity badge block
- *(readme)* Refresh current status for alpha-7 OBS milestone
- *(release)* Release v0.1.0-alpha.7

### 🚀 Features
- *(obs)* Add forge-obs crate skeleton
- *(platform-core)* [**breaking**] Extend IntegrationStatus with capability flags
- *(types)* Add OBS sub-action variants and runtime stubs
- *(platform-core)* [**breaking**] Replace IntegrationHealth metrics with stream shape
- *(types)* [**breaking**] Add ExecutionMetadata enum to ExecutionContext
- *(platform-core)* [**breaking**] Replace IntegrationCatalog with DetailSection
- *(platform-core)* [**breaking**] Rework QuickAction shape and add PickerKind
- *(obs)* Define ObsSink and ObsSource traits
- *(widgets)* Add integration_header_card widget
- *(obs)* Implement ObsClient connect with challenge-response auth
- *(widgets)* Add integration_health_grid widget
- *(obs)* Add reconnect with exponential backoff and jitter
- *(obs)* Impl IntegrationStatus for ObsClient
- *(obs)* Impl IntegrationHealth with stream-based deltas
- *(widgets)* Add integration_content_renderer dispatcher
- *(obs)* Impl IntegrationContent with scenes and sources
- *(obs)* Impl QuickActions with 4 actions and pickers
- *(widgets)* Add integration_quick_actions_grid widget
- *(obs)* Impl ObsSink methods via obws API calls
- *(app)* Add IntegrationDetail generic screen
- *(obs)* [**breaking**] Map obws events to forge bus EventSource::Obs
- *(app)* Add StreamApps landing screen with OBS card
- *(runtime)* [**breaking**] Wire OBS sub-action runners to ObsSink
- *(widgets)* Add full-screen picker_modal widget
- *(app)* Wire onboarding ConnectObs step to real flow
- *(types)* [**breaking**] Add Trigger::ObsSceneChanged variant and evaluator
- *(app)* Boot ObsClient and populate IntegrationDetail state
- *(app)* Wire QuickAction picker to SubAction enqueue and toast

### 🛠️ Build
- Remove redundant token-redaction job from pr and nightly
- *(manual)* Dedupe artifact upload via runner.os ternary
- *(nightly)* Use gh api for commit check instead of full checkout
- *(nightly)* Add explicit should_run guard to test job

### 🧪 Testing
- *(obs)* Add backoff monotonicity and reconnect-supervisor stub
- *(runtime)* Add OBS sub-action delegation regression with RecordingSink
- *(runtime)* Add QuickAction vs SubAction equivalence and event-kind regression

## [0.1.0-alpha.6] - 2026-05-18
### ⚠️ BREAKING CHANGES
- **types**: VariantKind::color() removed — use forge_widgets::variant_kind_color(kind, palette) instead
- **storage**: ScriptRecord.source_code renamed to body; description removed; contract: ScriptContract and body_hash: String added; upsert renamed to save.
- **script**: Engine::with_config now stores wall_timer as field; Engine struct is no longer Send (was already not Send due to rhai::Engine).
- **runtime**: dispatch signature gains registry param; bus changed to &Arc<EventBus>, dp to Arc<dyn DataProvider>
- **runtime**: spawn_action_engine now requires Arc<ScriptRegistry>

### ⚙️ Miscellaneous Tasks
- *(release)* Release v0.1.0-alpha.6
- Release

### 🐛 Bug Fixes
- Remove stage-marker strings from docs and error messages
- *(script)* Use Engine::new_raw with explicit packages and sleep budget

### 🚀 Features
- *(storage)* [**breaking**] Add migration 0004 and ScriptContract to ScriptRepo
- *(script)* [**breaking**] Add ForgeApi god-object with sandbox limits
- *(script)* Add doc-comment contract parser and scope builder
- *(runtime)* Add ScriptRegistry with hot-reload
- *(runtime)* [**breaking**] Add RunScript sub-action runner with registry lookup
- *(widgets)* Add console widget with typed prefix levels
- *(widgets)* Add code_editor wrapper with line numbers
- *(app)* Build ScriptEditor 3-pane screen
- *(runtime)* [**breaking**] Wire ScriptRegistry into ActionEngine for RunScript

### 🚜 Refactor
- *(types)* [**breaking**] Move VariantKind to forge-types from forge-widgets

### 🛠️ Build
- *(release)* Allow dirty release.yml for intentional manual deltas

### 🧪 Testing
- *(runtime)* Add ignored regression for RunScript wiring gap in ActionEngine

## [0.1.0-alpha.5] - 2026-05-18
### ⚠️ BREAKING CHANGES
- **runtime**: SubActionSpec gets 3 new variants — match arms in user code must be exhaustive
- **widgets**: forge-widgets now depends on forge-types

### ⚙️ Miscellaneous Tasks
- Release

### 🎨 Styling
- *(cliff)* Adds fix for typo in commit history

### 🚀 Features
- *(app)* Rebuild Hub screen per design mockup
- *(widgets)* Add title_bar_v2 breadcrumb with status pills
- *(widgets)* Add sidebar_v2 with nested groups and status dots
- *(app)* Rebuild Home content per v2 and wire chrome v2
- *(storage)* Wire globals reads/writes counters and add incr
- *(storage)* Add GlobalTransit, GlobalsExport and export_all
- *(runtime)* [**breaking**] Add globals sub-action runners and interpolation hook
- *(widgets)* [**breaking**] Add Tier 1.5 data widgets for globals editor
- *(app)* Build Globals screen with filter search and table
- *(app)* Add Variant editor modal for globals create and edit
- *(app)* Wire globals JSON export to native file dialog

### 🚜 Refactor
- *(app)* Rename Screen::Hub to Screen::Home

### 🧪 Testing
- *(runtime)* Assert interpolation increments globals reads counter

## [0.1.0-alpha.4] - 2026-05-18
### ⚠️ BREAKING CHANGES
- **paths**: data path on Windows now %APPDATA%\icesqueez\forge\data\, on macOS ~/Library/Application Support/com.icesqueez.forge/. Linux unchanged.
- **storage**: repo traits now return forge_types domain types; DataProvider gains 5 accessor methods replacing supertrait composition for those repos
- **twitch**: ChatSendBridge::spawn now takes Arc<dyn CredentialsRepo>

### ⚙️ Miscellaneous Tasks
- Release

### 🐛 Bug Fixes
- *(paths)* Use BaseDirs to drop owner segment from Windows path

### 📚 Documentation
- *(release)* Release v0.1.0-alpha.4

### 🚀 Features
- *(paths)* [**breaking**] Add cross-platform ProjectDirs helper and migrate call-sites
- *(types)* Add Action Trigger Command Queue SubAction primitives
- *(storage)* [**breaking**] Replace ActionRepo/TriggerRepo/CommandRepo/QueueRepo/HistoryRepo with domain types
- *(runtime)* Add ActionEngine with sub-action dispatch loop
- *(runtime)* Add QueueScheduler with pause and concurrency control
- *(runtime)* Add SendChat Delay SetGlobal sub-action runners
- *(runtime)* Add CommandParser for chat-message dispatch
- *(widgets)* Add actions editor widget family
- *(app)* Spawn ActionEngine Scheduler CommandParser on boot
- *(app)* Build Actions screen with tree pane and detail view
- *(app)* Add Add Action modal and create-action flow
- *(app)* Add Add Trigger modal with kind picker and config form
- *(app)* Add Add Sub-Action modal and remove button
- *(twitch)* Bridge chat.send.request events to Helix send

### 🚜 Refactor
- *(twitch)* [**breaking**] Hide SqliteBackend behind CredentialsRepo trait
- Strip stage refs from docs and drop unused import stub

### 🛠️ Build
- *(github)* Fix cargo dist
- *(github)* Undo running release on pull-request

### 🧪 Testing
- *(app)* Add end-to-end action pipeline causation test

## [0.1.0-alpha.3] - 2026-05-17
### ⚠️ BREAKING CHANGES
- **twitch**: TwitchChat / ChatSession / send_chat no longer take reqwest::Client parameter

### ⚙️ Miscellaneous Tasks
- *(cargo)* Cleanup
- Release

### 🐛 Bug Fixes
- *(app)* Wire user_info into credentials and chat operations
- *(app)* Bypass keyring in test helpers via open_with_key

### 📚 Documentation
- *(release)* Release v0.1.0-alpha.3

### 🚀 Features
- *(runtime)* Add EventBus with broadcast and bounded replay buffer
- *(twitch)* Add chat ingestion via EventSub and send-chat via Helix
- *(widgets)* Add chat row input bar and filter chip widgets
- *(app)* Wire LiveChat screen with EventBus subscription
- *(app)* Add Settings Platforms screen with Reconnect button
- *(twitch)* Add fetch_user_info via Helix GET users endpoint

### 🚜 Refactor
- *(twitch)* [**breaking**] Hide reqwest behind chat module internals
- *(events)* Remove dead EventBus trait and async-trait dep

### 🛠️ Build
- Drop redundant Swatinem cache step (setup-rust-toolchain handles it)
- Fix PR concurrency group and enable fail-fast on test matrix
- *(manual)* Tighten upload-artifact paths to exact binary names
- *(release)* Split build into per-OS matrix and download-merge publish

### 🧪 Testing
- *(twitch)* Add token redaction CI gate per RFC-011 qa concern
- *(app)* Add livechat end-to-end integration test

## [0.1.0-alpha.2] - 2026-05-17
### ⚠️ BREAKING CHANGES
- **widgets**: LoomPalette adds surface_overlay field
- **widgets**: Spacing scale + Density::Spacious rename + new Radius/BorderWidth/FontSize tokens
- **widgets**: onboarding widget signatures rebuilt — see DESIGN_AUDIT.md
- **oauth**: DeviceCodePoller methods no longer take reqwest::Client parameter

### ⚙️ Miscellaneous Tasks
- Rename project loom to forge across workspace
- Add MIT and Apache-2.0 dual license files
- *(github)* Add issue templates and pull request template
- Remove docs/ — CHANGELOG.md is the canonical release log
- Release

### 🐛 Bug Fixes
- *(widgets)* Migrate to iced 0.14 API after dep bump
- *(storage)* Register keyring backend and fall back to file key
- *(storage)* Rename LOOM_CREDENTIAL_KEY_FILE env var to FORGE prefix
- *(storage)* Bypass keyring in tests via open_with_key

### 📚 Documentation
- *(readme)* Capitalize Forge in acknowledgements sentence
- *(readme)* Add known limitations section for twitch client id
- Add CODE_OF_CONDUCT CONTRIBUTING and SECURITY policy
- *(readme)* Fix path to LICENSE
- *(release)* Release v0.1.0-alpha.2
- *(release)* Release v0.1.0-alpha.2

### 🚀 Features
- *(oauth)* Add DeviceCodePoller state machine with slow-down
- *(twitch)* Bootstrap loom-platform-twitch with device-code auth
- *(widgets)* Add onboarding widget family for first-run flow
- *(app)* Add OnboardingState and Welcome screen scaffold
- *(app)* Build ConnectPlatform screen with 4-card picker
- *(app)* Add ConnectObs StarterPack Ready onboarding screens
- *(app)* Persist onboarding_completed flag on finish or skip
- *(twitch)* Add client_id resolver with env and option_env
- *(app)* Wire DeviceCodeFlow screen with twitch oauth
- *(app)* Persist and resume last_onboarding_step across restarts

### 🚜 Refactor
- *(widgets)* [**breaking**] Fix theme background and add surface_overlay token
- *(widgets)* [**breaking**] Rebuild token scale with html-observed values
- *(widgets)* Rebuild button styles to match design source
- *(widgets)* Rebuild card styles to match design source
- *(widgets)* Align sidebar toolbar title-bar with design
- *(widgets)* Style text-input search-input and select
- *(widgets)* [**breaking**] Rewrite onboarding family to match mockups
- *(widgets)* Style section headers and add expandable variant
- *(widgets)* Rename LoomPalette to ForgePalette
- *(oauth)* [**breaking**] Hide reqwest behind DeviceCodePoller internals

### 🛠️ Build
- Split lint/test jobs, add audit, least-privilege perms
- Restore action version tags from dependabot bumps
- Inject FORGE_TWITCH_CLIENT_ID secret into release builds
- Add timeout-minutes guard on test jobs

## [0.1.0-alpha.1] - 2026-05-16
### ⚙️ Miscellaneous Tasks
- *(workspace)* Add cross-platform .gitignore
- *(workspace)* Bootstrap alpha-1 10-crate skeleton
- *(deps)* Bump softprops/action-gh-release from 2 to 3 (#2)
- *(deps)* Bump actions/upload-artifact from 4 to 7 (#3)
- *(deps)* Bump actions/checkout from 4 to 6 (#4)
- *(deps)* Bump iced from 0.13.1 to 0.14.0 (#5)
- *(deps)* Bump axum from 0.7.9 to 0.8.9 (#7)
- *(deps)* Bump tokio-tungstenite from 0.24.0 to 0.29.0 (#9)
- *(deps)* Bump keyring from 3.6.3 to 4.0.1 (#6)
- *(deps)* Bump iced_fonts from 0.1.1 to 0.3.0 (#8)
- Release

### 🎨 Styling
- *(types)* Strip tautological doc comments
- *(events)* Strip doc-policy violations
- *(widgets)* Collapse multi-line doc comments on font helpers
- *(workspace)* Fix formating

### 🐛 Bug Fixes
- *(storage)* Land queue.rs to match lib.rs mod declaration

### 📚 Documentation
- *(readme)* Add project README and alpha-1 release notes
- *(release)* Release v0.1.0-alpha.1

### 🚀 Features
- *(types)* Seed loom-types with Variant value system
- *(events)* Seed loom-events with Event bus contract
- *(storage)* Add StorageError enum with typed variants
- *(globals)* Add GlobalsRepo trait with GlobalEntry type
- *(storage)* Add UserGlobalsRepo trait for per-broadcaster scope
- *(storage)* Add SettingsRepo trait with reserved-keys catalog
- *(storage)* Add ActionRepo trait with ActionRecord type
- *(storage)* Add TriggerRepo trait with TriggerRecord type
- *(storage)* Add CommandRepo trait with CommandRecord type
- *(storage)* Add ScriptRepo trait with ScriptRecord type
- *(storage)* Add CredentialsRepo trait with CredentialId type
- *(storage)* Add HistoryRepo trait with HistoryRecord type
- *(storage)* Add DataProvider super-trait composing 10 repos
- *(platforms)* Add PlatformError enum with typed variants
- *(rhai)* Add ScriptError enum with sandbox-aware variants
- *(server)* Add ServerError enum with auth and sandbox variants
- *(oauth)* Add AuthFlow enum for device-code and local-callback
- *(platforms)* Add PlatformCapabilities with Limited flag
- *(server)* Add bindable axum stub with ServerHandle
- *(rhai)* Add sandboxed Engine wrapper with op-limit config
- *(platforms)* Add ChatPlatform trait with ConnectionState
- *(platforms)* Add RateLimiter trait with outcome enum
- *(platforms)* Add IntegrationDetail page trait family
- *(storage)* Add sqlite migration 0001 with all alpha-1 tables
- *(widgets)* Add LoomPalette and design tokens for 3 themes
- *(globals)* Implement SQLite GlobalsRepo with telemetry
- *(widgets)* Add Tier 1 button family with iced 0.13 styling
- *(widgets)* Add Tier 1 status indicator family
- *(widgets)* Add Tier 1 card family with metric + hero cards
- *(app)* Add iced application shell with Screen enum router
- *(storage)* Implement SQLite action-engine quad repo impls
- *(storage)* Implement AES-GCM crypto + CredentialsRepo
- *(storage)* Implement SQLite ScriptRepo and migration 0003
- *(storage)* Implement SQLite HistoryRepo
- *(storage)* Add SqliteBackend with DataProvider impl
- *(widgets)* Add Tier 1 navigation family with sidebar
- *(widgets)* Add Tier 1 layout family with title bar
- *(widgets)* Add Tier 1 input family with search and select
- *(widgets)* Add Tier 1 sections and notifications family
- *(hub)* Wire sidebar navigation and Hub view layout
- *(settings)* Add Settings sub-screens and Onboarding routing
- *(app)* Wire SqliteBackend into boot with first-run routing
- *(runtime)* Add InMemoryEventBus and iced subscription bridge

### 🛠️ Build
- *(github)* Add pr.yml workflow with cross-platform matrix
- *(github)* Add nightly workflow and dependabot config
- *(release)* Add release.yml manual.yml and cargo-dist config

