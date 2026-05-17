# Changelog
All notable changes to this project will be documented in this file.

## [0.1.0-alpha.3] - 2026-05-17
### ⚠️ BREAKING CHANGES
- **twitch**: TwitchChat / ChatSession / send_chat no longer take reqwest::Client parameter

### ⚙️ Miscellaneous Tasks
- *(cargo)* Cleanup

### 🐛 Bug Fixes
- *(app)* Wire user_info into credentials and chat operations
- *(app)* Bypass keyring in test helpers via open_with_key

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

