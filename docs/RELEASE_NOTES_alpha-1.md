# forge v0.1.0-alpha.1 — Foundation

**Release Date:** 2026-05-17

## Highlights

- **10-crate workspace** with complete trait architecture for chat platforms, integrations, TTS engines, storage backends, and server layer.
- **SQLite data layer** with AES-GCM encrypted credentials, migration runner, and 10 repository traits.
- **Iced UI shell** with Catppuccin Mocha theming (+ Tokyo Night & Latte), sidebar navigation, Settings, and first-run routing.
- **Design system (Tier 1)** with 20+ reusable widgets: buttons, cards, status indicators, forms, navigation, and empty states.
- **Cross-platform CI pipeline** shipping binaries for Linux (x86_64 + musl), Windows (MSVC), and macOS (x86_64 + ARM64).

## What's Working

- **App launch** on Linux, Windows, and macOS.
- **First-run detection** routes new users to Onboarding placeholder; returning users land on Hub.
- **Theme switching** in Settings persists across restarts (Catppuccin Mocha, Tokyo Night, Latte).
- **Sidebar navigation** between Hub, Actions, Commands, Platforms, Integrations, TTS, Soundboard, Logs, Settings screens.
- **Settings** → Diagnostics view shows DataProvider status, schema version, app version, and data directory path.
- **Data storage** at platform-appropriate paths: `~/.local/share/forge/` (Linux/Wayland), `%APPDATA%\forge\` (Windows), `~/Library/Application Support/forge/` (macOS).

## Known Limitations

- **No real chat platform integrations** — Twitch, YouTube, Trovo, Kick are trait skeletons only (implementation lands alpha-2+).
- **No TTS engines** — rhai scripting sandbox configured, LoomApi lands alpha-6.
- **No Soundboard** — UI placeholder only.
- **No Action Editor** — Actions / Commands / Triggers screens are empty.
- **No Globals system** — Variables/globals UI lands alpha-5.
- **Onboarding wizard is a placeholder** — real flow (Device Code OAuth, OBS connection) lands alpha-2.
- **No OBS, VTube Studio, Discord, MIDI, Hotkey integrations** — trait layer ready; implementations start alpha-7.
- **No browser-source overlay server** — axum skeleton ready; routes land alpha-9.

## Architecture

**Core layer** (`forge-types`, `forge-events`, `forge-runtime`, `forge-script`):
- `Variant`: 7-type polymorphic value system (Int, Float, Bool, String, Datetime, Array, Object).
- `Event` bus & `EventSource` taxonomy for system observability.
- `EventBus` trait + re-entrant event dispatch (impl lands alpha-3).
- rhai scripting engine sandbox (LoomApi god-object lands alpha-6).

**Data layer** (`forge-storage`, `forge-storage-sqlite`):
- 10 repository traits: Globals, UserGlobals, Settings, Actions, Triggers, Commands, Queues, Scripts, Credentials, History.
- SQLite WAL mode + async sqlx connection pool.
- AES-GCM encryption for stored credentials using OS keyring-derived keys.
- Migration `0001_init` with schemas for globals, settings, action history, and encrypted credential vault.

**Platform layer** (`forge-platform-core`):
- `ChatPlatform` trait contract.
- `AuthFlow` enum: Device Code (Twitch, YouTube), Local Callback (fallback), None (community/unofficial APIs).
- Four Integration Detail traits: `IntegrationStatus`, `IntegrationHealth`, `IntegrationCatalog`, `QuickActions` — used by all integrations (Twitch, OBS, VTube, Discord, MIDI, Hotkey, etc.).

**Integration & Audio layers** (skeleton crates):
- `forge-server`: axum HTTP + tokio-tungstenite WebSocket foundation.
- Audio layer ready for `forge-audio`, `forge-tts-core`, `forge-voice`, `forge-speak-queue`, `forge-soundboard` — implementations start alpha-3.

**UI layer** (`forge-widgets`, `forge-app`):
- **forge-widgets:** Design-system kit with 20+ Tier 1 components, theme factories, semantic color palette, spacing/density/typography tokens.
- **forge-app:** iced App shell with view router, Hub, Sidebar, Settings, Onboarding, first-run routing, theme persistence.

## CI / Release Pipeline

- **pr.yml:** Cross-platform matrix (Linux gate, Win/macOS informational in alpha) running rustfmt, clippy, tests, and release build.
- **nightly.yml:** Daily build at 03:00 UTC, skips if no commits since last success, uploads to "nightlies" recycling release.
- **release.yml:** On `v*` tag, invokes cargo-dist for binary builds and GitHub Release publication.
- **manual.yml:** workflow_dispatch per-platform build (debug/release) for QA testing.
- **Dependabot:** Weekly cargo ecosystem grouped by semver, weekly GitHub Actions.
- **Caching:** Swatinem/rust-cache for ~70% cold-build speedup.

## Next Stage (alpha-2)

- Twitch IRC chat integration + viewer database.
- OBS WebSocket skeleton → real control.
- More integration implementations.
- Action editor UI scaffolding.

## Installation

See [`docs/install/`](../docs/install/) for per-platform setup, or build from source per `README.md`.

## Support & Contributing

Open issues on GitHub. Contributing guidelines TBD (opening post-alpha-2).
