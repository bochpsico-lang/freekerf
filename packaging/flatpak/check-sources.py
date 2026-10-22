#!/usr/bin/env python3
"""Checks, offline, that the Flatpak's offline sources match the lock files.

cargo-sources.json must hold exactly the crates of Cargo.lock (name, version,
checksum; git dependencies by commit) and node-sources.json exactly the npm
packages of ui/web/package-lock.json (URL and sha512). When they do not, run
update-sources.sh and commit the result (doc/features/desktop-app.md "Releases").

    python3 packaging/flatpak/check-sources.py
"""

import base64
import json
import sys
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent


def cargo_problems() -> list[str]:
    with open(ROOT / "Cargo.lock", "rb") as f:
        lock = tomllib.load(f)
    sources = json.loads((HERE / "cargo-sources.json").read_text())

    want_crates, want_git = set(), set()
    for p in lock.get("package", []):
        source = p.get("source", "")
        if source.startswith("registry+"):
            want_crates.add((f"{p['name']}-{p['version']}", p["checksum"]))
        elif source.startswith("git+"):
            url = source.removeprefix("git+").split("?")[0].split("#")[0]
            want_git.add((url, source.rsplit("#", 1)[1]))

    have_crates, have_git = set(), set()
    for s in sources:
        dest = s.get("dest", "")
        if s["type"] == "archive" and dest.startswith("cargo/vendor/"):
            have_crates.add((dest.removeprefix("cargo/vendor/"), s["sha256"]))
        elif s["type"] == "git":
            have_git.add((s["url"], s["commit"]))

    return [f"Cargo.lock {c} missing from cargo-sources.json" for c in sorted(want_crates - have_crates)] + [
        f"cargo-sources.json {c} not in Cargo.lock" for c in sorted(have_crates - want_crates)
    ] + [f"Cargo.lock git {g} missing from cargo-sources.json" for g in sorted(want_git - have_git)] + [
        f"cargo-sources.json git {g} not in Cargo.lock" for g in sorted(have_git - want_git)
    ]


def npm_problems() -> list[str]:
    lock = json.loads((ROOT / "ui/web/package-lock.json").read_text())
    sources = json.loads((HERE / "node-sources.json").read_text())

    want = {}
    for p in lock.get("packages", {}).values():
        url, integrity = p.get("resolved"), p.get("integrity", "")
        if not url or not url.startswith("https://"):
            continue  # the root, links, local folders
        algo, _, digest = integrity.partition("-")
        # sha1 integrities are upgraded by the generator: only the URL is comparable
        want[url] = base64.b64decode(digest).hex() if algo == "sha512" else None

    have = {}
    for s in sources:
        if s["type"] == "file" and s.get("dest", "").startswith("flatpak-node/npm-cache"):
            have[s["url"]] = s.get("sha512")

    problems = [f"package-lock.json {u} missing from node-sources.json" for u in sorted(want.keys() - have.keys())]
    problems += [f"node-sources.json {u} not in package-lock.json" for u in sorted(have.keys() - want.keys())]
    problems += [
        f"node-sources.json {u}: sha512 differs from package-lock.json"
        for u in sorted(want.keys() & have.keys())
        if want[u] is not None and want[u] != have[u]
    ]
    return problems


def main() -> int:
    problems = cargo_problems() + npm_problems()
    for p in problems:
        print(p)
    if problems:
        print(f"\n{len(problems)} differences: run packaging/flatpak/update-sources.sh and commit the result")
        return 1
    print("cargo-sources.json and node-sources.json match the lock files")
    return 0


if __name__ == "__main__":
    sys.exit(main())
