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
- **Browser-source overlay server:** WebSocket + HTTP server for browser-source overlays, third-party statistics, and external client tools.
- **Rhai scripting sandbox:** Write powerful, sandboxed scripts in rhai to compute dynamic values and orchestrate complex logic.

**Key design goals:**

- **Clarity over features:** Every line of code should be understandable. The user must be able to read any single crate in one sitting.
- **Extensibility by trait:** New platforms, TTS engines, and integrations are added by dropping in a new crate that implements an existing trait — not by modifying core.
- **Cross-platform from day one:** Full support for Linux (Wayland-first), Windows, and macOS. All binaries released simultaneously.
- **Production discipline:** No half-finished features, no TODOs in shipped code. Every feature is fully tested and documented before release.

## Current Status

**Current alpha: v0.1.0-alpha.7** — OBS WebSocket integration milestone.

**What's included (alpha-1 through alpha-7):**

- **Workspace & storage layer:** 12-crate workspace; SQLite backend with AES-GCM encrypted credential storage; schema versioning with append-only migration pipeline.
- **iced UI shell:** Catppuccin Mocha theme + Tokyo Night and Latte; sidebar navigation; Hub dashboard; Settings with sub-screens; cross-platform CI pipeline (Linux, Windows, macOS).
- **Twitch platform:** Device-code OAuth flow with auto-refresh; EventSub chat ingestion; Helix send-chat; viewer tracking. Live Chat screen, Settings → Platforms screen with reconnect.
- **Action engine:** Action editor with trigger configuration, sub-action chains, and queue scheduling. Sub-actions: `SendChat`, `Delay`, `SetGlobal`, `RunScript`. Command parser for chat-triggered actions.
- **Globals system:** Per-key read/write counters; `%variable%` interpolation in action config; Globals editor with filter, JSON export, and Variant editor modal.
- **Rhai scripting sandbox:** `ForgeApi` god-object with op-count and time limits; `ScriptRegistry` with hot-reload; `RunScript` sub-action; 3-pane ScriptEditor screen.
- **OBS WebSocket v5 integration:** `forge-obs` crate; challenge-response auth; exponential-backoff reconnect; sub-actions: `SetScene`, `SetSourceVisible`, `SetInputMute`, `StartRecord`, `StopRecord`, `StartStream`, `StopStream`; `ObsSceneChanged` trigger; OBS events on the bus (`scene.changed`, `recording.*`, `streaming.*`, `source.visibility.changed`); generic `IntegrationDetail` screen; `StreamApps` landing screen; Onboarding ConnectObs step.

**Feature timeline (pending):**

- **beta-1+:** YouTube, Trovo, Kick chat platforms; TTS engines (local and cloud); VTube Studio; Discord webhooks; MIDI controllers; browser-source overlay server.

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
- **Windows:** Portable ZIP (no install) + MSI installer (coming beta-1).
- **macOS:** Disk image (.dmg) with signed binary (coming beta-1).

Per-platform installation details will be added as packaging matures.

## Known limitations

Forge is in active alpha development. Current gaps:

- **Twitch integration requires manual setup.** Pre-built binaries do not include a baked-in
  Twitch client_id. The project owner is awaiting Twitch developer account verification, which
  is currently blocked by SMS delivery issues to Ukrainian phone numbers. In the meantime, you
  can register your own Twitch application at
  [dev.twitch.tv/console/apps](https://dev.twitch.tv/console/apps) and set
  `FORGE_TWITCH_CLIENT_ID=<your-client-id>` before launching Forge — the device-code flow
  works end-to-end once the variable is present.
- **`ObsRaw` sub-action is non-functional.** The variant exists in the schema for forward
  compatibility, but `obws` v0.15 does not expose a raw-request passthrough. Execution returns
  a protocol error at runtime. Resolves when `obws` 0.16+ ships a `send_raw` API.
- **Other platform integrations** (YouTube, Trovo, Kick) are not yet implemented.

## Contributing

Contributions welcome once alpha-2 ships. For now, the project is in rapid foundational iteration.

## License

Licensed under either of:

- **MIT License** ([LICENSE-MIT](./LICENSE-MIT) or http://opensource.org/licenses/MIT)
- **Apache License 2.0** ([LICENSE-APACHE](./LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)

at your option.

## Acknowledgements

Forge is an independent open-source project built from first principles using Rust, iced, tokio, and community libraries.
