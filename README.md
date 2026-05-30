# forge

Cross-platform desktop tool for stream automation and multi-engine TTS — built in Rust.

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](./LICENSE)
[![Rust 1.95.0+](https://img.shields.io/badge/rust-1.95.0%2B-orange)](https://www.rust-lang.org/)
[![Platforms](https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS-green)](#installing-releases)

[![Latest Release](https://img.shields.io/github/v/release/IceSqueez/forge?include_prereleases&logo=github&label=Latest&cacheSeconds=600)](https://github.com/IceSqueez/forge/releases)
[![Release Date](https://img.shields.io/github/release-date-pre/IceSqueez/forge?logo=github&label=Released&cacheSeconds=600)](https://github.com/IceSqueez/forge/releases)
[![Release Workflow](https://github.com/IceSqueez/forge/actions/workflows/release.yml/badge.svg)](https://github.com/IceSqueez/forge/actions/workflows/release.yml)
[![Nightly Workflow](https://github.com/IceSqueez/forge/actions/workflows/nightly.yml/badge.svg)](https://github.com/IceSqueez/forge/actions/workflows/nightly.yml)

[![Commits Since Latest Release](https://img.shields.io/github/commits-since/IceSqueez/forge/latest?include_prereleases&logo=github&label=Commits%20since&cacheSeconds=600)](https://github.com/IceSqueez/forge/commits/main)
[![Open Issues](https://img.shields.io/github/issues/IceSqueez/forge?logo=github&label=Issues&cacheSeconds=600)](https://github.com/IceSqueez/forge/issues)
[![Commit Activity](https://img.shields.io/github/commit-activity/m/IceSqueez/forge?logo=github&label=Activity&cacheSeconds=600)](https://github.com/IceSqueez/forge/pulse)
[![Total Downloads](https://img.shields.io/github/downloads/IceSqueez/forge/total?logo=github&label=Downloads&cacheSeconds=600)](https://github.com/IceSqueez/forge/releases)

## What is forge?

Forge is an open-source desktop application that automates stream workflows across multiple chat and streaming platforms. It combines event-driven action automation with a powerful multi-engine text-to-speech pipeline, all in a single, focused tool built entirely in Rust.

**Core capabilities** (roadmap, shipping incrementally):

- **Event-driven automation:** React to chat messages, channel subscriptions, followers, raids, and custom events with configurable action chains.
- **Multi-platform chat support:** Connect to Twitch, YouTube, Trovo, and Kick simultaneously. Receive chat messages, track viewers, and trigger actions on any platform.
- **Multi-engine TTS:** Synthesize speech from multiple services — local (Piper, eSpeak-NG), cloud (Azure, OpenAI, ElevenLabs, Google), and platform-native (SAPI on Windows, NSpeech on macOS).
- **Voice aliases & preprocessing:** Define custom voice profiles, apply text replacements, and route messages through reusable processing pipelines.
- **OBS & VTube Studio integration:** Control scene switches, source visibility, filters, and VTube Studio model/expression state directly from actions.
- **Extensible integration layer:** Discord webhooks, MIDI controllers, system hotkeys, and custom webhooks for third-party software.
- **Browser-source overlay server:** WebSocket + HTTP server for browser-source overlays. HTML overlays subscribe to real-time chat/events, update live stats, and trigger actions back via the server API. Full path-traversal sandbox, configurable CORS, and bearer-token auth. Third-party tools and custom overlays are first-class.
- **Rhai scripting sandbox:** Write powerful, sandboxed scripts in rhai to compute dynamic values and orchestrate complex logic.

**Key design goals:**

- **Clarity over features:** Every line of code should be understandable. The user must be able to read any single crate in one sitting.
- **Extensibility by trait:** New platforms, TTS engines, and integrations are added by dropping in a new crate that implements an existing trait — not by modifying core.
- **Cross-platform from day one:** Full support for Linux (Wayland-first), Windows, and macOS. All binaries released simultaneously.
- **Production discipline:** No half-finished features, no TODOs in shipped code. Every feature is fully tested and documented before release.

## Current Status

**Current release: v0.1.0-beta.1** — First beta; YouTube platform, VTube Studio integration, and further platform hardening.

**Chat platforms: Twitch (full), YouTube (full).**

**What's included (alpha-1 through beta-1):**

- **Workspace & storage layer:** 14-crate workspace; SQLite backend with AES-GCM encrypted credential storage; schema versioning with append-only migration pipeline.
- **iced UI shell:** Catppuccin Mocha theme + Tokyo Night and Latte; sidebar navigation; Hub dashboard; Settings with sub-screens; cross-platform CI pipeline (Linux, Windows, macOS).
- **Twitch platform:** Device-code OAuth flow with auto-refresh; EventSub chat ingestion; Helix send-chat; viewer tracking. Live Chat screen, Settings → Platforms screen with reconnect.
- **YouTube platform:** Device-code OAuth flow with auto-refresh; live chat polling (3–60 s adaptive interval); Super Chat, Super Sticker, new member, and member milestone triggers; send-chat sub-action; daily quota tracking with automatic long-interval fallback at 9,000/10,000 units; broadcast lifecycle triggers. See [docs/platforms/youtube.md](docs/platforms/youtube.md).
- **Action engine:** Action editor with trigger configuration, sub-action chains, and queue scheduling. Sub-actions: `SendChat`, `Delay`, `SetGlobal`, `RunScript`, `PlaySound`. Command parser for chat-triggered actions.
- **Globals system:** Per-key read/write counters; `%variable%` interpolation in action config; Globals editor with filter, JSON export, and Variant editor modal.
- **Rhai scripting sandbox:** `ForgeApi` god-object with op-count and time limits; `ScriptRegistry` with hot-reload; `RunScript` sub-action; 3-pane ScriptEditor screen.
- **OBS WebSocket v5 integration:** `forge-obs` crate; challenge-response auth; exponential-backoff reconnect; sub-actions: `SetScene`, `SetSourceVisible`, `SetInputMute`, `StartRecord`, `StopRecord`, `StartStream`, `StopStream`; `ObsSceneChanged` trigger; OBS events on the bus (`scene.changed`, `recording.*`, `streaming.*`, `source.visibility.changed`); generic `IntegrationDetail` screen; `StreamApps` landing screen; Onboarding ConnectObs step.
- **EventFeed + Replay debugging:** 2-pane Event Feed screen with filter chips (All / Chat / Subs / Bits / Timers / OBS / Audio / Errors), Pause / Resume / Clear / Export controls, and a per-event payload inspector with syntax-highlighted JSON viewer. Every event persists to SQLite (`event_log`, 7-day retention) and carries a full causation chain (`caused_by`) across all subsystems. One-click replay of any captured event re-runs the full action pipeline — useful for debugging action flows without waiting for a live trigger. Replayed events are visually distinguished in the feed.
- **WebSocket server:** Full WS server at `/ws/v1/` with 14+ methods (subscribe, getInfo, getActions, doAction, getCommands, getGlobals, setGlobal, getUserGlobals, triggerCodeEvent, getEvents, replayEvent, getActiveViewers, getOverlayFiles). Bearer-token auth. Per-client subscription filtering, backpressure, and ev/s tracking. Server screen with live status, connected-clients list, bandwidth/throughput metrics, overlay file listing, and lifecycle controls.
- **HTTP overlay-host:** Serves HTML overlays from user-configured sandbox directory with path-traversal protection, CORS controls, and token-optional gating. Overlays subscribe to real-time events via WebSocket and trigger actions back through the API.
- **Settings → WebSocket:** Configurable bind address (127.0.0.1 vs 0.0.0.0 with LAN-bind warning), port, auth toggles, overlay-root picker, and CORS policy.
- **Audio engine (alpha-10):** `forge-audio` crate with `AudioSink` trait, cpal device discovery, multi-sink fan-out, symphonia decoder, rubato resampler, channel remix. `AudioEvent` and `AudioEventSink` abstraction. Audio events on the bus (`playback.started`, `playback.finished`, `playback.failed`). Settings → Audio sub-screen with device test-tone.
- **Soundboard (alpha-10):** `forge-soundboard` crate with clip schema (file path, hotkey, output device, volume). Grid-based Soundboard screen with add-clip modal. In-app hotkey listener scoped to Soundboard. `PlaySound` sub-action picker in Action editor. `SoundboardPlayer` decodes, resamples, applies volume, and routes to selected output device.
- **Variant::Datetime (alpha-12):** 8th first-class Variant type; `forge::time::now()` and `forge::time::unix()` rhai builtins for timestamp capture and Unix-epoch access in scripts.
- **Sub-actions: ReadFile & RandomInt (alpha-12):** `ReadFile` reads sandboxed files (under data dir/assets, 1 MiB cap, no path traversal) into globals. `RandomInt` generates random i64 into a global. Both in Sub-Action picker in Action editor.
- **Action execution modes (alpha-12):** `ExecutionMode::Sequential` (default: run all sub-actions in order) or `ExecutionMode::RandomPick` (pick and run exactly one sub-action per trigger fire). Toggle in Add Action modal.
- **Viewers screen (alpha-12):** Track chat participants across all connected platforms. List shows avatar, platform pill, message count, last-seen time, and custom-greeting badge. Filter by platform and search by username. ViewerTracker task subscribes to `chat.message` events and upserts viewer data in real-time.
- **Settings sub-screens (alpha-12):** Storage & backups (DB path display, Vacuum button, timestamped backup), Diagnostics (log dir path, Open log directory button, RUST_LOG hint), Queues & threading (tokio worker count and link to Queues screen).
- **Home (Hub renamed, alpha-12):** Hero card with version and uptime ticker. Title bar shows 8-subsystem connectivity counter (Twitch chat, EventSub, OBS, Server WS, Audio, Soundboard, Speak queue, DataProvider).
- **Retro polish (alpha-12):** Event Feed export to JSON via native dialog. Actions Duplicate button (clones with `(copy)` suffix). Sub-action timing badges showing rolling 20-execution average. Twitch detail header shows token expiry countdown. Trigger row Delete button for inline removal. cpal device enumeration cached 5s for Settings → Audio refresh performance.

**Feature timeline (pending):**

- **beta-2+:** Trovo and Kick chat platforms; Discord webhooks; MIDI controllers; system hotkeys; TTS engines (Piper, eSpeak-NG, cloud services). Voice aliases & preprocessing pipeline. Full notifications customization.

## Building from Source

### Prerequisites

- **Rust 1.95.0 or later** — [install here](https://rustup.rs/).
- **Linux:** GCC, pkg-config. On Ubuntu/Debian: `sudo apt-get install build-essential pkg-config libssl-dev`.
- **Windows:** Visual Studio Build Tools or MSVC.
- **macOS:** Xcode Command Line Tools (`xcode-select --install`).

### Build

```bash
git clone https://github.com/IceSqueez/forge.git
cd forge
cargo build --release
```

The binary will be at `target/release/forge` (Linux/macOS) or `target/release/forge.exe` (Windows).

### Run

```bash
./target/release/forge
```

On first run, the app will initialize your data directory (XDG-compliant on Linux, AppData on Windows, Library on macOS) and route you to the Onboarding screen.

## Installing Releases

Binary releases for Linux, Windows, and macOS are published on [GitHub Releases](https://github.com/IceSqueez/forge/releases).

- **Linux:** AppImage (universal), Flatpak, AUR (community).
- **Windows:** Portable ZIP (no install) + MSI installer (coming soon).
- **macOS:** Disk image (.dmg) with signed binary (coming soon).

Per-platform installation details will be added as packaging matures.

## Known limitations

Forge is completing alpha and transitioning to beta. Current gaps:

- **`ObsRaw` sub-action is non-functional.** The variant exists in the schema for forward compatibility, but `obws` v0.15 does not expose a raw-request passthrough. Execution returns a protocol error at runtime. Resolves when `obws` 0.16+ ships a `send_raw` API.
- **Additional chat platforms** (Trovo, Kick) landing in beta-2 and beyond.
- **VTube Studio integration, Discord webhooks, MIDI controllers, system hotkeys** coming in beta and rc stages.
- **TTS engines** (Piper, eSpeak-NG, cloud services: Azure, OpenAI, ElevenLabs, Google) and **voice aliases & preprocessing pipeline** deferred to beta-2+.
- **TLS/WSS** for the WebSocket server is deferred to beta or rc; current use is local-network only.

## Contributing

Contributions welcome once alpha-2 ships. For now, the project is in rapid foundational iteration.

## License

Licensed under either of:

- **MIT License** ([LICENSE-MIT](./LICENSE-MIT) or http://opensource.org/licenses/MIT)
- **Apache License 2.0** ([LICENSE-APACHE](./LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)

at your option.

## Acknowledgements

Forge is an independent open-source project built from first principles using Rust, iced, tokio, and community libraries.
