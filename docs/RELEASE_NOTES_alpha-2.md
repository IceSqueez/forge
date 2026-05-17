# forge v0.1.0-alpha.2 — Onboarding + Twitch OAuth

**Release Date:** 2026-05-17

Alpha-2 delivers the first-run experience: a 5-step onboarding wizard that walks a new user from Welcome through Twitch authentication (Device Code Flow) and into Hub, with credentials encrypted at rest and surviving restarts. The project was also renamed from streamer-loom/loom to forge across the entire workspace during this stage.

## Highlights

### Project rename: streamer-loom → Forge

All 11 workspace crates (`loom-*` → `forge-*`), the compiled binary (`loom` → `forge`), and the on-disk data path (`~/.local/share/streamer-loom/` → `~/.local/share/forge/`) have been renamed. Alpha-1 users should see "Upgrading from alpha-1" below — there is no automatic data migration.

### Onboarding flow

A 5-step wizard guides new users through setup:

1. **Welcome** — introduction and language tip
2. **Connect Platform** — pick from Twitch, YouTube, Trovo, or Kick (only Twitch is active; others show "Coming soon")
3. **Device Code Flow** — authenticate Twitch without a browser callback
4. **Connect OBS** — placeholder (OBS integration lands alpha-7)
5. **Starter Pack** — placeholder (starter packs land beta-10); transitions to Hub on "Finish"

Steps 3–4 carry an `OPTIONAL` badge and a "Skip for now" action. Mid-flow progress persists: if the app restarts before onboarding completes, it resumes at the last active step.

### Twitch OAuth — Device Code Flow

Implements RFC 8628 Device Authorization Grant against `id.twitch.tv/oauth2/device` and `id.twitch.tv/oauth2/token`. The screen shows a short user code and a URL; the user opens that URL on any device, authorizes, and the app detects success automatically. Handles `authorization_pending`, `slow_down` (adaptive back-off), `expired_token`, and `access_denied` responses. On success, access and refresh tokens are stored encrypted in `CredentialsRepo` under the key `twitch:broadcaster`.

See "Known limitations" — a Twitch app `client_id` must be supplied by the user until the project can register its own.

### Design refresh

All onboarding and core widgets were rebuilt to match the HTML mockups: revised button radii, card borders, surface overlay tokens, input styles, sidebar proportions, and typography. Twelve new widgets were added to `forge-widgets` for the onboarding/auth family.

### CI improvements

- Lint and test jobs split into separate steps.
- `cargo audit` security job added.
- All workflow jobs run with least-privilege `permissions` blocks.
- `FORGE_TWITCH_CLIENT_ID` secret injected at build time via `option_env!` for release and manual builds.

### Open-source scaffolding

`LICENSE-MIT`, `LICENSE-APACHE`, `CODE_OF_CONDUCT.md`, `CONTRIBUTING.md`, `SECURITY.md`, and GitHub issue/PR templates added.

---

## What's new — by area

### Core / Runtime

No runtime changes this stage. Event bus, action queues, and scripting engine are unchanged.

### Platforms

- New crate `forge-platform-twitch`: Device Code auth surface — `request_device_code`, `poll_for_token`, token-refresh on 401 with `PlatformError::ReauthRequired`. Chat ingestion deferred to alpha-3.
- `forge-platform-core::oauth::DeviceCodePoller`: reusable RFC 8628 polling state machine — handles all terminal and non-terminal poll outcomes, `slow_down` back-off, cancellation flag, and configurable initial interval.
- Twitch `client_id` resolved from `FORGE_TWITCH_CLIENT_ID` env var at runtime, with a compile-time bake via `option_env!`.

### Storage

- `SettingsRepo` extended: `ONBOARDING_COMPLETED` and `LAST_ONBOARDING_STEP` reserved keys fully wired — boot reads both to decide whether to show onboarding and at which step.
- Keyring resilience: if no OS keyring is available, falls back to a file-based 256-bit key at `$XDG_DATA_HOME/forge/credentials-key` (permissions `0o600`).
- AES-GCM encrypted credentials at rest with per-record random 12-byte nonces.

### UI

- 12 new `forge-widgets` components: `onboarding_stepper`, `onboarding_step_header` (with `OPTIONAL` pill), `platform_picker_card`, `locale_tip_card`, `onboarding_footer`, `device_code_display` (monospace, 8 px letter-spacing), `expiration_timer`, `live_status_banner` (pulsing dot + technical hint), `numbered_box_step`, `page_shell`, `title_bar_with_logo`, `coming_soon_view`.
- Full onboarding screen wired end-to-end: `Welcome → ConnectPlatform → DeviceCodeFlow → ConnectObs → StarterPack → Ready → Hub`.
- `DeviceCodeFlow` screen reachable outside onboarding for future re-auth flows.
- `SIDEBAR_WIDTH` constant set to 200 px; sidebar, toolbar, and title-bar proportions aligned with design source.

### Infrastructure

- iced 0.13 → 0.14 migration.
- keyring 3.x → 4.x with explicit backend feature flag.
- All `loom-*` workspace crates renamed to `forge-*`; binary renamed `loom` → `forge`; data path updated.

---

## Known limitations

- **Twitch `client_id` not shipped with binaries** — register your own app at [dev.twitch.tv/console/apps](https://dev.twitch.tv/console/apps) and set `FORGE_TWITCH_CLIENT_ID` in your shell environment. The onboarding screen shows a `MissingClientId` banner with instructions if the var is absent. See README for full steps.
- **Settings → Platforms screen deferred to alpha-3** — the screen exists as a placeholder. Re-authorizing Twitch or checking connection status requires re-running onboarding (see workaround in KNOWN_ISSUES ISS-002).
- **Reconnect button deferred to alpha-3** — token refresh on expiry requires manual re-onboarding for now.
- **Other platforms not yet implemented** — YouTube, Trovo, and Kick remain trait skeletons.

---

## Upgrading from alpha-1

**Data path moved.** The app boots with a fresh database at `~/.local/share/forge/forge.db`. Alpha-1 data at `~/.local/share/streamer-loom/` is not migrated automatically. To carry over alpha-1 data:

```sh
mkdir -p ~/.local/share/forge
cp ~/.local/share/streamer-loom/streamer-loom.db ~/.local/share/forge/forge.db
```

**Env var renamed.** If you set `LOOM_CREDENTIAL_KEY_FILE` for Docker or CI key injection, rename it to `FORGE_CREDENTIAL_KEY_FILE`.

**Git remote.** The repository moved to `github.com/IceSqueez/forge`. GitHub redirects the old URL automatically, but updating your local remote is recommended:

```sh
git remote set-url origin https://github.com/IceSqueez/forge.git
```

---

## Breaking changes

- All `loom-*` crate names renamed to `forge-*`. Path or git-ref dependencies need updating.
- `LoomPalette` renamed to `ForgePalette`.
- Binary name changed from `loom` to `forge`.
- `DeviceCodePoller` no longer accepts a `reqwest::Client` parameter on `request_device_code`, `poll_once`, or `run` — the HTTP client is now managed internally.

---

## Contributors

- IceSqueez (maintainer)

---

## Full changelog

See [CHANGELOG.md](../CHANGELOG.md) for the complete commit log.
