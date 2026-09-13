#!/usr/bin/env bash
# forge-emulator check - the nested workspace's own gate, unpiped so no exit code is masked.
# The repository gate never sees this workspace; run this from anywhere.

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

# Judge the toolchain pinned by the repository's rust-toolchain.toml, never an ambient override.
unset RUSTUP_TOOLCHAIN

run() { printf '\n==> %s\n' "$*"; "$@"; }

run cargo build --workspace --all-targets
# No `--all`: that flag would also reach into the path-dependency crates under crates/.
run cargo fmt -- --check
run cargo clippy --workspace --all-targets -- -D warnings
printf '\n==> RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps\n'
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
run cargo test --workspace

printf '\nEMULATOR CHECK GREEN\n'
