# forge

Event-driven stream automation with multi-engine TTS — a single Rust desktop app for Linux, Windows, and macOS.

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](./LICENSE)
[![Rust 1.96.0+](https://img.shields.io/badge/rust-1.96.0%2B-orange)](https://www.rust-lang.org/)
[![Platforms](https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS-green)](#installing-releases)

[![Latest Release](https://img.shields.io/github/v/release/IceSqueez/forge?include_prereleases&logo=github&label=Latest&cacheSeconds=600)](https://github.com/IceSqueez/forge/releases)
[![Release Date](https://img.shields.io/github/release-date-pre/IceSqueez/forge?logo=github&label=Released&cacheSeconds=600)](https://github.com/IceSqueez/forge/releases)
[![Release Workflow](https://github.com/IceSqueez/forge/actions/workflows/release.yml/badge.svg)](https://github.com/IceSqueez/forge/actions/workflows/release.yml)
[![Nightly Workflow](https://github.com/IceSqueez/forge/actions/workflows/nightly.yml/badge.svg)](https://github.com/IceSqueez/forge/actions/workflows/nightly.yml)

[![Commits Since Latest Release](https://img.shields.io/github/commits-since/IceSqueez/forge/latest?include_prereleases&logo=github&label=Commits%20since&cacheSeconds=600)](https://github.com/IceSqueez/forge/commits/main)
[![Open Issues](https://img.shields.io/github/issues/IceSqueez/forge?logo=github&label=Issues&cacheSeconds=600)](https://github.com/IceSqueez/forge/issues)
[![Commit Activity](https://img.shields.io/github/commit-activity/m/IceSqueez/forge?logo=github&label=Activity&cacheSeconds=600)](https://github.com/IceSqueez/forge/pulse)
[![Total Downloads](https://img.shields.io/github/downloads/IceSqueez/forge/total?logo=github&label=Downloads&cacheSeconds=600)](https://github.com/IceSqueez/forge/releases)

Connect to Twitch, YouTube, and Kick simultaneously. React to chat events with action chains that control OBS, send messages, run scripts, play sounds, and speak text through configurable TTS engines.

## Features

### Chat platforms
- **Twitch**
  - EventSub triggers: chat (commands, messages, cheers), shared-chat, subs/resubs/gift-subs, follows, raids (received & sent), channel-point redemptions (custom & automatic) & reward CRUD, polls, predictions, hype train, charity, goals, moderation (ban/timeout/unban, mod add/remove, shield mode, suspicious users, warning acknowledged), shoutouts (sent & received), guest-star, ad break, automod, stream on/off, channel & chat-settings updates
  - Sub-actions: send chat/reply/announcement/whisper, ban/timeout/unban/warn, mod & VIP management, shoutout, start/cancel raid, run/snooze ad, poll & prediction lifecycle, reward CRUD & redemption fulfillment, automod approve/deny/terms, update title/category/tags, stream marker, get current goal, guest-star & shield-mode control, chat clear & message delete
- **YouTube** (live-chat polling)
  - Triggers: chat message, chat command, Super Chat, Super Sticker, message deleted, new member, member milestone, membership gift (mass) & gift received, stream online/offline, stream title changed, user banned/timed-out
  - Sub-actions: send/delete chat message, ban/timeout/unban user, add/remove moderator, update stream title/description/category/privacy
- **Kick** — Live chat, subs, hosts, bans (community implementation)

### Actions & automation
- Action editor with trigger configuration and sub-action chains
- Sub-actions: send chat, delay, set/get/increment globals, run script, play sound, speak text, read file, random int; Twitch sub-actions selectable in the same editor (see Twitch above)
- Command parser for chat-triggered actions
- Queue scheduling: Sequential or RandomPick execution

### Integrations
- **OBS WebSocket v5** — switch scene, source visibility, input mute, start/stop stream/record; scene-changed trigger
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

- **Linux:** AppImage, `.deb` (Debian/Ubuntu), `.rpm` (Fedora/openSUSE) — x86_64
- **Windows:** portable `.exe` + MSI installer
- **macOS:** universal `.dmg` (Apple Silicon + Intel)

## Building from source

**Requires Rust 1.96.0 or later** — [install via rustup](https://rustup.rs/).

Linux also needs: `sudo apt-get install build-essential pkg-config libssl-dev` (Debian/Ubuntu).

```bash
git clone https://github.com/IceSqueez/forge.git
cd forge
cargo build --release
./target/release/forge
```

On first run, forge creates its data directory (XDG on Linux, AppData on Windows, Application Support on macOS) and opens to the dashboard.

## Contributing

Open an issue or pull request on [GitHub](https://github.com/IceSqueez/forge).

## License

Licensed under either of [MIT](./LICENSE-MIT) or [Apache 2.0](./LICENSE-APACHE) at your option.
