#!/usr/bin/env python3
"""Dependency closure gate.

Resolves the normal-dependency closure of every workspace crate, one crate at
a time and across all targets, and holds the crate layering:

- Only the audio and TTS crates and the composition root link the audio
  stack (device output, decoding) or the language-detector stack.
- forge-platform-core reaches no forge crate other than types, events, net.
- forge-speech-types reaches no forge crate at all.

Every allowance below names its reason; an allowance nothing needs any more is
reported as stale, so the list only shrinks.

Usage: dependency-closure.py [repo-root]
Exit status: 0 clean, 1 violations or stale allowances.
"""

import json
import pathlib
import re
import subprocess
import sys

STACKS = {
    "cpal": "audio-device stack",
    "symphonia": "audio-decode stack",
    "lingua": "language-detector stack",
}

AUDIO = "audio crate, owns device output and decoding"
SPEECH = "TTS crate, plays or detects through the audio stack"
ROOT = "composition root"

STACK_CONSUMERS = {
    "forge-audio": AUDIO,
    "forge-soundboard": AUDIO,
    "forge-speak-queue": SPEECH,
    "forge-tts-pipeline": SPEECH,
    "forge-tts-cloud": SPEECH,
    "forge-desktop": ROOT,
}

FORGE_REACH = {
    "forge-platform-core": {"forge-types", "forge-events", "forge-net"},
    "forge-speech-types": set(),
}

TREE_LINE = re.compile(r"^(\d+)(\S+) v\S+")


def workspace_members(root):
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
        cwd=root,
        check=True,
        capture_output=True,
    ).stdout
    meta = json.loads(out)
    members = set(meta["workspace_members"])
    return sorted(p["name"] for p in meta["packages"] if p["id"] in members)


def closure_paths(root, crate):
    out = subprocess.run(
        [
            "cargo",
            "tree",
            "--locked",
            "--package",
            crate,
            "--edges",
            "normal",
            "--target",
            "all",
            "--prefix",
            "depth",
            "--format",
            "{p}",
        ],
        cwd=root,
        check=True,
        capture_output=True,
    ).stdout.decode()
    chain = []
    paths = {}
    for line in out.splitlines():
        match = TREE_LINE.match(line)
        if not match:
            continue
        depth, name = int(match.group(1)), match.group(2)
        del chain[depth:]
        chain.append(name)
        paths.setdefault(name, list(chain))
    return paths


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    members = workspace_members(root)
    violations = []
    used = set()

    for crate in members:
        paths = closure_paths(root, crate)
        for stack, label in STACKS.items():
            if stack not in paths:
                continue
            if crate in STACK_CONSUMERS:
                used.add(crate)
                continue
            violations.append(
                f"{crate} links the {label} ({stack}): {' -> '.join(paths[stack])}"
            )
        if crate in FORGE_REACH:
            allowed = FORGE_REACH[crate]
            for name, path in sorted(paths.items()):
                if name == crate or name not in members or name in allowed:
                    continue
                permitted = ", ".join(sorted(allowed)) or "no forge crate"
                violations.append(
                    f"{crate} reaches {name} (allowed: {permitted}): {' -> '.join(path)}"
                )

    stale = sorted(set(STACK_CONSUMERS) - used)
    missing = sorted((set(STACK_CONSUMERS) | set(FORGE_REACH)) - set(members))

    for line in violations:
        print(f"error: {line}")
    for crate in stale:
        print(f"error: stale allowance, {crate} no longer links any guarded stack")
    for crate in missing:
        print(f"error: {crate} is named by the gate but is not a workspace member")
    if violations or stale or missing:
        print(
            f"\ndependency-closure: {len(violations)} violation(s), {len(stale)} stale,"
            f" {len(missing)} unknown. Move the dependency behind the owning audio or TTS"
            " crate, or name the crate in the gate's allowances with a reason."
        )
        return 1
    print(f"dependency-closure: clean ({len(members)} crates)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
