#!/usr/bin/env python3
"""HTTP client boundary gate.

Outside the leaf crate forge-net, nothing constructs a reqwest client, reads a
reqwest response body, or depends on reqwest outside an explicit exception.
The scan covers every tracked Rust source and Cargo manifest, test code and
dev-dependencies included. Each exception below is named and carries a reason;
an exception that no longer matches anything is reported as stale, so the
list shrinks as the migration finishes.

Usage: http-client-boundary.py [repo-root]
Exit status: 0 clean, 1 violations or stale exceptions.
"""

import pathlib
import re
import subprocess
import sys
import tomllib

LEAF_CRATE = "crates/forge-net"

LOCAL_SERVER = "test dials the forge server bound on loopback"
LOOPBACK_CALLBACK = "test triggers the OAuth loopback callback listener"

# Production code that may own a reqwest client. Must stay empty.
PRODUCTION_EXCEPTIONS = {}

# Files whose test code may dial a local server directly. Hits before the
# file's `#[cfg(test)] mod` block (production code) are still violations.
TEST_EXCEPTIONS = {
    "crates/forge-server/src/server.rs": LOCAL_SERVER,
    "crates/forge-server/src/routes/api_v1.rs": LOCAL_SERVER,
    "crates/forge-server/src/routes/audio.rs": LOCAL_SERVER,
    "crates/forge-server/src/routes/overlays.rs": LOCAL_SERVER,
    "crates/forge-platform-kick/src/auth/mod.rs": LOOPBACK_CALLBACK,
    "crates/forge-platform-youtube/src/auth.rs": LOOPBACK_CALLBACK,
}

# Whole trees outside the shipped product.
PATH_EXCEPTIONS = {
    "tools/forge-emulator/": "separate black-box test-harness workspace, never linked into forge; its fakes and scenario runner speak HTTP to forge directly",
}

# Crates whose tests use reqwest value types (status codes, URLs, header
# names) without constructing a client. The source scan still applies to them.
DEV_DEPENDENCY_EXCEPTIONS = {
    "crates/forge-platform-core": "tests parse redirect URLs with reqwest::Url",
    "crates/forge-platform-twitch": "tests name Helix statuses with reqwest::StatusCode",
    "crates/forge-tts-cloud": "signer tests build request URLs with reqwest::Url",
}

CLIENT_PATTERNS = [
    ("client construction", re.compile(r"\breqwest\s*::\s*(Client|ClientBuilder)\b")),
    ("client construction", re.compile(r"\breqwest\s*::\s*get\b")),
    ("blocking client", re.compile(r"\breqwest\s*::\s*blocking\b")),
    ("aliased import", re.compile(r"\breqwest\s+as\s+\w+")),
]

USE_GROUP = re.compile(r"\buse\s+reqwest\s*::\s*\{([^}]*)\}", re.DOTALL)
FORBIDDEN_USE_ITEM = re.compile(r"(?<![\w:])(Client|ClientBuilder|get|blocking)\b")

BODY_READ = re.compile(
    r"\.\s*(bytes|text|chunk|bytes_stream)\s*\(\s*\)\s*\.\s*await\b"
    r"|\.\s*json\s*(::\s*<[^()]*>)?\s*\(\s*\)\s*\.\s*await\b"
)

TEST_MODULE_ATTR = re.compile(r"^\s*#\[cfg\(test\)\]\s*$")
ATTRIBUTE_LINE = re.compile(r"^\s*#\[.*\]\s*$")
TEST_MODULE_OPEN =re.compile(r"^\s*(pub(\([^)]*\))?\s+)?mod\s+\w+\s*\{")

RUST_COMMENT = re.compile(r"//.*?$|/\*.*?\*/", re.DOTALL | re.MULTILINE)


def tracked(root, pattern):
    out = subprocess.run(
        ["git", "ls-files", "-z", "--", pattern],
        cwd=root,
        check=True,
        capture_output=True,
    ).stdout.decode()
    return sorted(p for p in out.split("\0") if p)


def in_leaf(path):
    return path.startswith(LEAF_CRATE + "/")


def path_exempt(path):
    return any(path.startswith(prefix) for prefix in PATH_EXCEPTIONS)


def strip_comments(text):
    return RUST_COMMENT.sub(lambda m: re.sub(r"[^\n]", " ", m.group(0)), text)


def test_region_start(path, lines):
    parts = pathlib.PurePosixPath(path).parts
    if "tests" in parts or "benches" in parts or "examples" in parts:
        return 0
    for index, line in enumerate(lines):
        if not TEST_MODULE_ATTR.match(line):
            continue
        following = next(
            (l for l in lines[index + 1 :] if l.strip() and not ATTRIBUTE_LINE.match(l)), ""
        )
        if TEST_MODULE_OPEN.match(following):
            return index
    return None


def line_of(text, offset):
    return text.count("\n", 0, offset) + 1


def source_hits(text):
    hits = []
    for rule, pattern in CLIENT_PATTERNS:
        for match in pattern.finditer(text):
            hits.append((line_of(text, match.start()), rule))
    for group in USE_GROUP.finditer(text):
        for item in FORBIDDEN_USE_ITEM.finditer(group.group(1)):
            hits.append((line_of(text, group.start(1) + item.start()), "client import"))
    if re.search(r"\breqwest\b", text):
        for match in BODY_READ.finditer(text):
            hits.append((line_of(text, match.start()), "response body read"))
    return sorted(set(hits))


def scan_sources(root, violations, used_exceptions):
    for path in tracked(root, "*.rs"):
        if in_leaf(path) or path_exempt(path):
            if path_exempt(path):
                used_exceptions.add(next(p for p in PATH_EXCEPTIONS if path.startswith(p)))
            continue
        raw = (root / path).read_text(encoding="utf-8")
        text = strip_comments(raw)
        hits = source_hits(text)
        if not hits:
            continue
        if path in PRODUCTION_EXCEPTIONS:
            used_exceptions.add(path)
            continue
        lines = raw.splitlines()
        test_start = test_region_start(path, lines) if path in TEST_EXCEPTIONS else None
        for line, rule in hits:
            if test_start is not None and line - 1 >= test_start:
                used_exceptions.add(path)
                continue
            violations.append(f"{path}:{line}: {rule}: {lines[line - 1].strip()}")


def reqwest_entries(table):
    for name, spec in (table or {}).items():
        package = spec.get("package") if isinstance(spec, dict) else None
        if name == "reqwest" or package == "reqwest":
            yield name


def dependency_tables(manifest):
    for key in ("dependencies", "dev-dependencies", "build-dependencies"):
        yield key, manifest.get(key)
    for target, body in (manifest.get("target") or {}).items():
        for key in ("dependencies", "dev-dependencies", "build-dependencies"):
            yield f"target.{target}.{key}", body.get(key)


def crate_of(path):
    return str(pathlib.PurePosixPath(path).parent)


def crate_owns_exception(crate, exceptions):
    return any(entry.startswith(crate + "/") for entry in exceptions)


def scan_manifests(root, violations, used_exceptions):
    for path in tracked(root, "*Cargo.toml"):
        if path_exempt(path):
            continue
        crate = crate_of(path)
        if crate == LEAF_CRATE:
            continue
        manifest = tomllib.loads((root / path).read_text(encoding="utf-8"))
        for section, table in dependency_tables(manifest):
            for _ in reqwest_entries(table):
                if section.endswith("dev-dependencies"):
                    if crate_owns_exception(crate, TEST_EXCEPTIONS) or crate_owns_exception(
                        crate, PRODUCTION_EXCEPTIONS
                    ):
                        continue
                    if crate in DEV_DEPENDENCY_EXCEPTIONS:
                        used_exceptions.add(crate)
                        continue
                elif section.endswith("dependencies") and not section.endswith(
                    "build-dependencies"
                ):
                    if crate_owns_exception(crate, PRODUCTION_EXCEPTIONS):
                        continue
                violations.append(f"{path}: reqwest in [{section}] outside {LEAF_CRATE}")


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    violations = []
    used_exceptions = set()
    scan_sources(root, violations, used_exceptions)
    scan_manifests(root, violations, used_exceptions)

    declared = (
        set(PRODUCTION_EXCEPTIONS)
        | set(TEST_EXCEPTIONS)
        | set(PATH_EXCEPTIONS)
        | set(DEV_DEPENDENCY_EXCEPTIONS)
    )
    stale = sorted(declared - used_exceptions)

    for line in violations:
        print(f"error: {line}")
    for entry in stale:
        print(f"error: stale exception, nothing matches it any more: {entry}")
    if violations or stale:
        print(
            f"\nhttp-client-boundary: {len(violations)} violation(s), {len(stale)} stale exception(s)."
            f" Route the call through {LEAF_CRATE} or name the file in the gate's exceptions with a reason."
        )
        return 1
    print("http-client-boundary: clean")
    return 0


if __name__ == "__main__":
    sys.exit(main())
