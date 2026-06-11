# forge

Event-driven stream automation with multi-engine TTS — a single Rust desktop app for Linux, Windows, and macOS.

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](./LICENSE)
[![Platforms](https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS-green)](#installing-releases)
[![Latest Release](https://img.shields.io/github/v/release/IceSqueez/forge?include_prereleases&logo=github&label=Latest&cacheSeconds=600)](https://github.com/IceSqueez/forge/releases)

Connect to Twitch, YouTube, and Kick simultaneously. React to chat events with action chains that control OBS, send messages, run scripts, play sounds, and speak text through configurable TTS engines.

## Features

### Chat platforms
- **Twitch** — EventSub chat, subs, bits, raids, follows; send-chat sub-action
- **YouTube** — Live chat polling, Super Chat/Sticker, new member, member milestone; send-chat sub-action
- **Kick** — Live chat, subs, hosts, bans (community implementation)

### Actions & automation
- Action editor with trigger configuration and sub-action chains
- Sub-actions: send chat, delay, set/get/increment globals, run script, play sound, speak text, read file, random int
- Command parser for chat-triggered actions
- Queue scheduling: Sequential or RandomPick execution

### Integrations
- **OBS WebSocket v5** — scene-changed trigger and live OBS state events on the bus
- **VTube Studio** — model load/move, expression set, parameter set/reset
- **Discord** — post text, embed, edit message
- **MIDI** — note-on/off triggers, CC triggers
- **System hotkeys** — global key-combination triggers
- **Soundboard** — clip grid with per-clip output device and volume; `PlaySound` sub-action

### TTS pipeline
- Local engines: Piper (ONNX), eSpeak-NG, SAPI (Windows), NSpeech (macOS)
- Cloud engines: Azure, OpenAI, ElevenLabs, Google TTS
- Voice aliases with text-replacement preprocessing filters
- Speak queue with pause, resume, and skip controls

### Scripting & storage
- Rhai sandbox: op-count and time limits, `ForgeApi` access, hot-reload
- Named globals with `%variable%` interpolation; JSON export
- SQLite backend with AES-GCM encrypted credentials; 7-day event log

### Overlay server
- WebSocket server at `/ws/v1/` — 14+ methods, bearer-token auth, per-client subscriptions
- HTTP overlay host with path-traversal sandbox and configurable CORS
- Live server screen: connected-client list, bandwidth metrics, lifecycle controls

### UI
- Event Feed with filter chips, pause/resume, per-event payload inspector, and one-click event replay
- Live Chat screen across all connected platforms; Viewers screen with per-platform filter
- Settings: appearance (density + font pickers), language (English / Ukrainian), keyboard shortcuts editor, audio device test

## Installing releases

Binary releases for Linux, Windows, and macOS are on [GitHub Releases](https://github.com/IceSqueez/forge/releases).

- **Linux:** AppImage (universal), Flatpak, AUR (community)
- **Windows:** Portable ZIP
- **macOS:** Disk image (.dmg)

## Building from source

**Requires Rust 1.96.0 or later** — [install via rustup](https://rustup.rs/).

Linux also needs: `sudo apt-get install build-essential pkg-config libssl-dev` (Debian/Ubuntu).

```bash
git clone https://github.com/IceSqueez/forge.git
cd forge
cargo build --release
./target/release/forge
```

On first run, forge initializes your data directory and opens the onboarding screen.

## Contributing

Open an issue or pull request on [GitHub](https://github.com/IceSqueez/forge).

## License

Licensed under either of [MIT](./LICENSE-MIT) or [Apache 2.0](./LICENSE-APACHE) at your option.
