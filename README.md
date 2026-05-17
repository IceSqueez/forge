# forge

Cross-platform desktop tool for stream automation and multi-engine TTS — built in Rust.

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](./LICENSE.md)
[![Rust 1.95.0+](https://img.shields.io/badge/rust-1.95.0%2B-orange)](https://www.rust-lang.org/)
[![Platform Support: Linux | Windows | macOS](https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS-green)](./docs/install/)
[![Status: Alpha](https://img.shields.io/badge/status-alpha--1-yellow)](./docs/release-notes/)

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

**v0.1.0-alpha.1** is the foundation release: workspace structure, trait architecture, SQLite data layer, iced UI shell, and CI pipeline. Real feature work (chat platforms, integrations, TTS engines) ships in alpha-2 onward.

**What's included in alpha-1:**

- 10-crate workspace with trait scaffolding for platforms, integrations, TTS, storage, and server.
- SQLite backend with AES-GCM encrypted credential storage.
- Catppuccin Mocha theme + theming system with Tokyo Night and Latte.
- iced app shell with Hub dashboard, sidebar navigation, Settings.
- First-run detection and Onboarding placeholder.
- Cross-platform CI pipeline (GitHub Actions matrix for Linux, Windows, macOS).

**Known limitations:**

- No real chat platform connections yet.
- No TTS engines (rhai scripting API lands alpha-6).
- No soundboard or custom overlays.
- No action editor or trigger configuration UI.
- Onboarding wizard is a placeholder.

**Feature timeline:**

- **alpha-2:** Twitch IRC chat + user viewer DB.
- **alpha-3:** OBS WebSocket + Piper TTS engine.
- **alpha-4:** Action editor + trigger UI.
- **alpha-5:** Globals editor + variable system.
- **alpha-6:** rhai scripting API + LoomApi.
- **beta-1+:** YouTube, Trovo, Kick; more TTS engines; VTube Studio; Discord webhooks; MIDI.

See the full roadmap once alpha-2 ships.

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

See [`docs/install/`](./docs/install/) for per-platform installation details.

## Known limitations

Forge is in active alpha development. Current gaps:

- **Twitch integration requires manual setup.** Pre-built binaries do not include a baked-in
  Twitch client_id. The project owner is awaiting Twitch developer account verification, which
  is currently blocked by SMS delivery issues to Ukrainian phone numbers. In the meantime, you
  can register your own Twitch application at
  [dev.twitch.tv/console/apps](https://dev.twitch.tv/console/apps) and set
  `FORGE_TWITCH_CLIENT_ID=<your-client-id>` before launching Forge — the device-code flow
  works end-to-end once the variable is present.
- **Other platform integrations** (YouTube, Trovo, Kick) are not yet implemented.

For additional known issues, see release notes for each alpha.

## Contributing

Contributions welcome once alpha-2 ships. For now, the project is in rapid foundational iteration.

## License

Licensed under either of:

- **MIT License** ([LICENSE-MIT](./LICENSE-MIT) or http://opensource.org/licenses/MIT)
- **Apache License 2.0** ([LICENSE-APACHE](./LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)

at your option.

## Acknowledgements

Forge is an independent open-source project built from first principles using Rust, iced, tokio, and community libraries.
