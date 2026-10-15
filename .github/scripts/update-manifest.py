#!/usr/bin/env python3
"""Write the self-update manifest (forge-update.json) for one release.

The document shape mirrors crates/forge-update/src/manifest.rs. Every update
payload the client can apply must be present exactly once, otherwise the
release would silently stop offering in-place updates to some installs.

Usage: update-manifest.py <artifacts-dir> <tag> <repo> <output-file>
"""

import datetime
import hashlib
import json
import os
import re
import sys
import urllib.parse

MANIFEST_FORMAT = 1
BIN_NAME = "forge"

# (platform id, artifact kind id, asset file-name suffix)
PAYLOADS = [
    ("linux-x64", "appimage", "-linux-x64.AppImage"),
    ("macos-universal", "app-tar-gz", "-macos-universal.app.tar.gz"),
    ("windows-x64", "exe", "-windows-x64.exe"),
    ("windows-x64", "msi", "-windows-x64.msi"),
]

PLAIN_FILE_NAME = re.compile(r"^[A-Za-z0-9_+\-][A-Za-z0-9._+\-]*$")
VERSION = re.compile(r"^\d+\.\d+\.\d+(-[0-9A-Za-z]+(\.[0-9A-Za-z]+)*)?$")


def fail(message):
    print(f"::error::{message}", file=sys.stderr)
    sys.exit(1)


def sha256_of(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def find_payload(artifacts_dir, version, suffix):
    prefix = f"{BIN_NAME}-{version}+"
    matches = sorted(
        name
        for name in os.listdir(artifacts_dir)
        if name.endswith(suffix) and os.path.isfile(os.path.join(artifacts_dir, name))
    )
    if len(matches) != 1:
        fail(f"expected exactly one *{suffix} asset, found {matches or 'none'}")
    name = matches[0]
    if not name.startswith(prefix):
        fail(f"{name} does not belong to version {version} (expected prefix {prefix})")
    if not PLAIN_FILE_NAME.match(name):
        fail(f"{name} is not a plain file name the client accepts")
    return name


def main(argv):
    if len(argv) != 5:
        fail("usage: update-manifest.py <artifacts-dir> <tag> <repo> <output-file>")
    artifacts_dir, tag, repo, output = argv[1:]

    version = tag[1:] if tag.startswith("v") else tag
    if not VERSION.match(version):
        fail(f"tag {tag} is not a release version")

    download_base = f"https://github.com/{repo}/releases/download/{urllib.parse.quote(tag)}"
    artifacts = []
    for platform, kind, suffix in PAYLOADS:
        name = find_payload(artifacts_dir, version, suffix)
        path = os.path.join(artifacts_dir, name)
        artifacts.append(
            {
                "platform": platform,
                "kind": kind,
                "name": name,
                "url": f"{download_base}/{urllib.parse.quote(name, safe='')}",
                "sha256": sha256_of(path),
                "size": os.path.getsize(path),
            }
        )

    manifest = {
        "format": MANIFEST_FORMAT,
        "version": version,
        "published_at": datetime.datetime.now(datetime.timezone.utc).strftime(
            "%Y-%m-%dT%H:%M:%SZ"
        ),
        "release_page": f"https://github.com/{repo}/releases/tag/{urllib.parse.quote(tag)}",
        "artifacts": artifacts,
    }
    with open(output, "w", encoding="utf-8") as handle:
        json.dump(manifest, handle, indent=2)
        handle.write("\n")
    print(json.dumps(manifest, indent=2))


if __name__ == "__main__":
    main(sys.argv)
