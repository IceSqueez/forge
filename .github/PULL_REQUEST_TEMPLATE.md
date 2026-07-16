## Summary

<!-- One paragraph: what does this PR change and why? -->

## Type of change

<!-- Check all that apply -->

- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds functionality)
- [ ] Breaking change (fix or feature that breaks existing behavior)
- [ ] Documentation update
- [ ] CI / tooling
- [ ] Refactor (no behavior change)

## Related issues

<!-- Link issues this PR closes or addresses. Use `Closes #N` to auto-close. -->

Closes #

## Pre-merge checklist

- [ ] `cargo build --workspace` passes
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] Commits follow Conventional Commits format (`feat:`, `fix:`, etc.)
- [ ] Each commit is a single logical change (no "WIP" / squash-me commits)
- [ ] No `TODO` / `FIXME` / `XXX` comments added
- [ ] Public items have `///` rustdoc only where contract is non-obvious
- [ ] No new dependencies added (or justified in summary above)
- [ ] If this changes UI: verified by running `cargo run -p forge-app` and clicking through affected screens

## Screenshots / demo (if UI change)

<!-- Attach images or short clips showing the change. -->

## Notes for reviewer

<!-- Anything reviewers should know - tricky edge cases, design alternatives you considered, follow-up work, etc. -->
