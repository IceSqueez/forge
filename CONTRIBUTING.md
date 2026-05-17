# Contributing to Forge

Thanks for your interest in contributing! Forge is in active alpha development and welcomes contributions across code, docs, design, and bug reports.

## Code of Conduct

All contributors and maintainers are expected to follow the [Contributor Covenant](CODE_OF_CONDUCT.md). Report concerns to icesqueez@gmail.com.

## Getting started

### Prerequisites

- Rust 1.95.0 stable toolchain (managed via `rust-toolchain.toml` — rustup installs it automatically when you build).
- Standard build tools (gcc/clang, pkg-config).
- Linux/Wayland (Hyprland tested), Windows 10+, or macOS 12+.

### Building

```bash
git clone https://github.com/IceSqueez/forge.git
cd forge
cargo build --workspace
cargo run -p forge-app
```

## Development workflow

### Branch from the active release

Forge uses release-prefixed branches (`release/alpha.N`, `release/beta.N`, `release/1.0`). Feature work targets the current alpha/beta branch — check the README "Status" section for which branch is active.

### Commit conventions

- **Conventional Commits format:** `<type>(<scope>): <subject>`
- Types: `feat`, `fix`, `refactor`, `docs`, `chore`, `ci`, `style`, `test`, `perf`.
- Subject ≤72 characters, imperative mood.
- One logical change per commit. Granular commits are preferred over "one big commit per feature".
- No body required. If you write one, use it for `BREAKING CHANGE:` notes only.

### Pre-commit gate (MANDATORY)

Every commit MUST pass all four:

```bash
cargo build --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

A commit that fails any of these will be rejected during review.

## Code style

### General

- Forge is in pre-1.0 development. Breaking changes are welcome — no backward-compat shims, no `_unused` parameters, no deprecated re-exports.
- Comments are sparing. Public items get `///` rustdoc ONLY when the contract is non-obvious (panics, lifetimes, invariants). Avoid tautologies like `/// Creates a new X` above `fn new()`.
- No `TODO`, `FIXME`, `XXX`, or `unimplemented!()` outside explicitly-stubbed crates. Production discipline applies to every commit.

### Naming

- Crates: `forge-<domain>` (e.g. `forge-runtime`, `forge-platform-twitch`).
- Modules: `snake_case`. Types: `PascalCase`. Functions/variables: `snake_case`.
- Error enums: per-crate, derived from `thiserror`. No `Box<dyn Error>` in public APIs.

### Architecture rules

- **External isolation:** Third-party types (iced, sqlx, reqwest, tokio handles, cpal, rhai, etc.) MUST NOT leak into public APIs of any `forge-*` crate. Wrap behind owned traits.
- **No `unsafe` outside FFI:** Anywhere else needs explicit approval in PR review. FFI modules document safety invariants above each `unsafe` block.
- **Errors as values:** `Result<T, E>` everywhere. No panics in library code. `.unwrap()` / `.expect()` only in `main.rs` startup paths or tests.

## Pull requests

1. Fork the repo and create a feature branch from the current release branch (`release/alpha.N`).
2. Make granular commits per the conventions above.
3. Run the full pre-commit gate.
4. Open a PR against the same release branch, using the PR template.
5. CI will run the gate on Linux/Windows/macOS — all three must pass.
6. Address review feedback by adding new commits (no force-push during review).
7. Maintainer will squash-merge or rebase-merge once approved.

## Adding dependencies

Forge is intentionally minimal. Before adding a new crate dependency:

1. Check that the workspace's existing deps (`Cargo.toml` → `[workspace.dependencies]`) don't already provide what you need.
2. Justify in the PR description: what problem the dep solves, why a smaller existing crate or vendored function won't suffice.
3. Verify license compatibility (MIT/Apache/BSD-style preferred; GPL-incompatible deps blocked outside subprocess-isolated paths like eSpeak-NG).

## Areas that need help

- Translations: `fluent-rs` localization (en + uk shipped; community contributions for more languages welcome from beta-11+).
- Cross-platform testing (Wayland Hyprland is primary; Windows + macOS coverage appreciated).
- Twitch/YouTube/Trovo/Kick integration testing (need real accounts).

## Questions

- Open a [GitHub Discussion](https://github.com/IceSqueez/forge/discussions) for design questions, ideas, or general help.
- File an [Issue](https://github.com/IceSqueez/forge/issues) for bugs or confirmed feature requests.

## License

By contributing to Forge, you agree that your contributions will be dual-licensed under the [MIT License](LICENSE-MIT) and the [Apache License 2.0](LICENSE-APACHE), at the recipient's option.
