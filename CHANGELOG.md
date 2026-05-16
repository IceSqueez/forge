# Changelog
All notable changes to this project will be documented in this file.

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

### 🎨 Styling
- *(types)* Strip tautological doc comments
- *(events)* Strip doc-policy violations
- *(widgets)* Collapse multi-line doc comments on font helpers

### 🐛 Bug Fixes
- *(storage)* Land queue.rs to match lib.rs mod declaration

### 📚 Documentation
- *(readme)* Add project README and alpha-1 release notes

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

