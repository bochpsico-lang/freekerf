#!/usr/bin/env bash
# Regenerates the offline sources of the Flatpak build from the lock files:
#   cargo-sources.json  from Cargo.lock              (flatpak-cargo-generator)
#   node-sources.json   from ui/web/package-lock.json (flatpak-node-generator)
# Run it after any change to either lock file and commit the result; check-sources.py
# (the flatpak-test job of release.yml) fails when they are stale
# (doc/features/desktop-app.md "Releases").
#
#   mise exec python@3.13 -- packaging/flatpak/update-sources.sh
set -euo pipefail

# flatpak/flatpak-builder-tools, pinned: a new commit is a reviewed change here
TOOLS_COMMIT=74697c75b630d7330e77250fc13cb5ea688d9479

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

git -C "$work" init -q tools
git -C "$work/tools" fetch -q --depth 1 https://github.com/flatpak/flatpak-builder-tools "$TOOLS_COMMIT"
git -C "$work/tools" checkout -q FETCH_HEAD

python3 -m venv "$work/venv"
"$work/venv/bin/pip" install -q aiohttp tomlkit "$work/tools/node"

# run from the work dir: the generators' own folders stay off the module path
cd "$work"
"$work/venv/bin/python" -I "$work/tools/cargo/flatpak-cargo-generator.py" \
  "$root/Cargo.lock" -o "$here/cargo-sources.json"
"$work/venv/bin/flatpak-node-generator" npm "$root/ui/web/package-lock.json" \
  -o "$here/node-sources.json"
# the browsers Playwright downloads (gigabytes) are for the UI tests, not the build
"$work/venv/bin/python" -I - "$here/node-sources.json" <<'PY'
import json, sys
path = sys.argv[1]
with open(path) as f:
    sources = json.load(f)
sources = [s for s in sources if not s.get("dest", "").startswith("flatpak-node/cache/ms-playwright")]
with open(path, "w") as f:
    json.dump(sources, f, indent=4)
    f.write("\n")
PY
echo "updated $here/cargo-sources.json and $here/node-sources.json"
