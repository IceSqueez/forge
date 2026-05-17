# Security Policy

## Supported versions

Forge is in alpha development. Only the latest released alpha/beta/rc/stable version is supported with security fixes. Older alpha versions are not patched — please upgrade.

| Version              | Supported |
|:---------------------|:----------|
| latest alpha/beta/rc | ✅        |
| older alphas         | ❌        |

## Reporting a vulnerability

**Do not file a public GitHub issue for security vulnerabilities.**

Instead, report privately via one of:

- **Email:** icesqueez@gmail.com — use subject prefix `[FORGE SECURITY]`.
- **GitHub Security Advisories:** open a private advisory at <https://github.com/IceSqueez/forge/security/advisories/new>.

We aim to acknowledge reports within 72 hours and provide a fix or a public disclosure timeline within 14 days for high-severity issues. Please include:

- Affected version (commit SHA or release tag).
- Steps to reproduce.
- Expected vs. observed behavior.
- Suggested fix (optional).

## Scope

In scope:

- Forge desktop application (`forge` binary).
- Forge crates (`forge-*` under `crates/`).
- Authentication flows (OAuth Device Code, credential storage, token redaction).
- rhai scripting sandbox bypass.
- WebSocket server (`forge-server`) auth bypass or path traversal.

Out of scope:

- Third-party platforms (Twitch, YouTube, OBS, etc.) — report to those projects directly.
- User-installed rhai scripts or downloaded soundboard files — Forge does not vet user-supplied content.

## Recognition

We do not yet have a bug bounty program. Reporters of valid vulnerabilities will be credited in release notes (with their consent).
