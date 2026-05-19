# Changelog
All notable changes to this project will be documented in this file.

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

### 🐛 Bug Fixes
- *(obs)* Align event payloads with RFC-031 and populate caused_by
- *(twitch)* Align observability events with RFC-031 audit
- *(runtime)* [**breaking**] Populate caused_by across full event causation chain
- *(twitch)* Use char-boundary truncation for body_snippet
- *(widgets)* Cap json_viewer recursion depth to prevent stack overflow

### 📚 Documentation
- *(readme)* Remove Twitch client_id manual-setup limitation
- *(readme)* Refresh current status for alpha-8 EventFeed milestone

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

