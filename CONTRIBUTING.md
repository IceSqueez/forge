# Contributing to Forge

Thanks for your interest in contributing. Forge welcomes contributions across code, docs, translations and bug reports.

## Code of Conduct

All contributors and maintainers are expected to follow the [Contributor Covenant](CODE_OF_CONDUCT.md). Report concerns to icesqueez@gmail.com.

## Getting started

### Prerequisites

- Rust, pinned in `rust-toolchain.toml` (currently 1.98.1, with `rustfmt` and `clippy`). [rustup](https://rustup.rs/) installs it on the first build. If a tool such as mise exports `RUSTUP_TOOLCHAIN`, unset it so the pinned toolchain is used.
- On Debian / Ubuntu, the system libraries CI installs:

  ```bash
  sudo apt-get install build-essential pkg-config libwayland-dev libxkbcommon-dev \
    libxkbcommon-x11-dev libgl1-mesa-dev libx11-dev libasound2-dev libdbus-1-dev libfontconfig1-dev
  ```

- Windows and macOS need no extra system packages beyond the platform build tools (MSVC Build Tools / Xcode Command Line Tools).

### Building and running

```bash
git clone https://github.com/IceSqueez/forge.git
cd forge
cargo build --workspace
cargo run -p forge-desktop
```

The binary is called `forge`. It keeps its data in your user data folder; set `FORGE_DATA_DIR` to point a development build at a scratch folder instead of your real settings.

### Workspace layout

All crates live under `crates/`:

| Area | Crates |
| :--- | :--- |
| App and UI | `forge-desktop` (the `forge` binary), `forge-components` (UI component kit) |
| Core | `forge-types`, `forge-events`, `forge-registry`, `forge-runtime`, `forge-script` |
| Storage | `forge-storage` (the `DataProvider` trait), `forge-storage-sqlite` |
| Chat platforms | `forge-platform-core` (shared traits, auth, rate limiting), `forge-platform-twitch`, `forge-platform-youtube`, `forge-platform-kick` |
| Integrations | `forge-obs`, `forge-vtube`, `forge-discord`, `forge-midi`, `forge-hotkey` |
| Server and overlays | `forge-server`, `forge-overlay` |
| Audio and TTS | `forge-audio`, `forge-soundboard`, `forge-tts-core`, `forge-tts-pipeline`, `forge-voice`, `forge-speak-queue`, `forge-tts-piper`, `forge-tts-espeak`, `forge-tts-sapi` (Windows), `forge-tts-nsspeech` (macOS), `forge-tts-cloud` |

New platforms, TTS engines and integrations are added as new crates that implement an existing trait, not by editing the core.

## Development workflow

### Branches

`main` is the only long-lived branch. Releases are tags on `main`. External contributors fork the repository, work on a branch in their fork and open a pull request against `main`.

### Pre-commit gate

Every commit must pass:

```bash
cargo build --workspace --all-targets
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo test --workspace
```

Run each command on its own and check its exit code; do not pipe them through `head`, `grep` or similar, which hides failures. CI runs the same checks on pull requests to `main`: format, clippy and doc on Linux, tests on Linux, Windows and macOS.

For UI changes, also run the app and click through the affected screens. A passing build is not proof that a screen works.

### Commit messages

[Conventional Commits](https://www.conventionalcommits.org/), subject line only:

```
<type>(<scope>): <subject>

BREAKING CHANGE: <one-line summary of what breaks>
```

- Types: `feat`, `fix`, `refactor`, `perf`, `docs`, `test`, `style`, `ci`, `chore`.
- Scope is the feature area (`twitch`, `tts`, `ui`, `soundboard`), not the crate name: `feat(twitch)`, not `feat(forge-platform-twitch)`.
- Subject in imperative mood ("add", "fix", "replace"), at most 72 characters, describing the change in behavior rather than the files touched.
- No body. The only allowed extra line is an optional `BREAKING CHANGE:` note.
- One logical change per commit. Small, focused commits are preferred over one commit per feature.

`CHANGELOG.md` is generated from commit subjects at release time. Do not edit it by hand.

## Tests

- Unit tests sit next to the code in `#[cfg(test)]` modules.
- Integration tests live in each crate's `tests/` directory (for example `crates/forge-runtime/tests/`).
- `tools/forge-emulator/` is a separate black-box harness with its own workspace and `Cargo.lock`. `cargo test --workspace` does not build it; run `./check.sh` inside that folder if you change it.

Tests must not need real services, hardware or network: no live Twitch/YouTube/Kick accounts, no real TTS engines, no audio devices. Use mocks. A test that cannot run in CI is deleted, not marked `#[ignore]`.

## Code style

### General

- Forge is pre-1.0. Breaking changes are fine: no backward-compat shims, no `_unused` parameters, no deprecated re-exports.
- Comments are rare. Public items get `///` docs only when the contract is not obvious from the signature (panics, invariants, lifetimes). No `/// Creates a new X` above `fn new()`.
- No `TODO`, `FIXME`, `XXX` or `unimplemented!()` in committed code.
- No magic numbers: use a named constant or a typed value (such as `reqwest::StatusCode::NOT_FOUND`) instead of a bare literal.

### Naming

- Crates: `forge-<domain>`.
- Modules, functions, variables: `snake_case`. Types: `PascalCase`.
- One `thiserror` error enum per crate. No `Box<dyn Error>` in public APIs.

### Architecture rules

- **External isolation:** third-party types (sqlx, reqwest, rhai, cpal and so on) do not appear in the public API of a `forge-*` crate unless that crate exists to wrap them. Wrap them behind owned traits and types.
- **No `unsafe` outside FFI.** FFI code documents the safety invariant above each `unsafe` block.
- **Errors as values:** `Result<T, E>` everywhere. No panics in library code; `.unwrap()` / `.expect()` only in startup code or tests.
- **Rate limits and secrets:** every platform API call goes through the shared rate limiter in `forge-platform-core`. Tokens are never logged.
- **Async:** never hold a lock across `.await`. The UI holds handles to the runtime, never runtime state.

## Pull requests

1. Fork the repository and create a branch from `main`.
2. Make focused commits following the conventions above.
3. Run the full pre-commit gate.
4. Open a pull request against `main` and fill in the template.
5. CI must pass on Linux, Windows and macOS.
6. Address review feedback with new commits. Do not force-push during review.

## Adding dependencies

Forge keeps its dependency list short. Before adding a crate:

1. Check that `[workspace.dependencies]` in the root `Cargo.toml` does not already cover the need.
2. Explain in the pull request what the dependency solves and why a smaller option will not do.
3. Check the license. Forge is MIT OR Apache-2.0; GPL code is only acceptable behind a subprocess boundary (as with eSpeak-NG), never linked.

## Areas that need help

- Translations: the UI uses Fluent (`.ftl` files); English and Ukrainian ship today.
- Cross-platform testing, especially Windows and macOS.
- Testing Twitch, YouTube and Kick integrations with real accounts.

## Questions

- Open a [GitHub Discussion](https://github.com/IceSqueez/forge/discussions) for design questions, ideas or general help.
- File an [Issue](https://github.com/IceSqueez/forge/issues) for bugs or confirmed feature requests.

## License

By contributing to Forge, you agree that your contributions are dual-licensed under the [MIT License](LICENSE-MIT) and the [Apache License 2.0](LICENSE-APACHE), at the recipient's option.
