#!/usr/bin/env bash
# Smoke test of the macOS disk image (release.yml): it mounts and holds the app and
# the Applications link, the app is signed (ad hoc) and universal, the bundled
# `freekerf` CLI answers, the app starts on a fresh FREEKERF_HOME and is still
# running after 15 s, and `freekerf mcp` attaches to it. Runs on a macOS runner
# (or any Mac) without administrator rights; bash 3.2, the one macOS ships.
#
#   smoke-macos-installer.sh <file.dmg>
set -euo pipefail

dmg=${1:?disk image}
STAY_UP_S=15
ARCHS="arm64 x86_64"

log() { printf '\n== %s\n' "$*"; }
fail() {
  printf '\nFAILED: %s\n' "$*" >&2
  exit 1
}
# runs "$@" for at most $1 seconds (macOS has no timeout(1))
with_timeout() {
  local s=$1
  shift
  perl -e 'alarm shift; exec @ARGV' "$s" "$@"
}

# short paths: the endpoint is a Unix socket under FREEKERF_HOME (104 bytes at most)
work=$(mktemp -d /tmp/fk-smoke.XXXXXX)
mnt="$work/mnt"
app_pid=
cleanup() {
  if [ -n "$app_pid" ]; then kill "$app_pid" 2>/dev/null || true; fi
  hdiutil detach -quiet "$mnt" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

log "hdiutil attach $dmg"
mkdir "$mnt"
hdiutil attach -nobrowse -readonly -noautoopen -mountpoint "$mnt" "$dmg" >/dev/null
ls -la "$mnt"
src=$(find "$mnt" -maxdepth 1 -name '*.app' | head -n1)
[ -n "$src" ] || fail "no .app in the disk image"
[ "$(readlink "$mnt/Applications")" = /Applications ] || fail "no Applications link in the disk image"

# what a user does: drag the app out of the image
app="$work/$(basename "$src")"
cp -R "$src" "$app"
hdiutil detach -quiet "$mnt"

plist="$app/Contents/Info.plist"
exe="$app/Contents/MacOS/$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$plist")"
cli="$app/Contents/MacOS/freekerf"
echo "bundle id $(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$plist")," \
  "version $(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$plist")," \
  "macOS $(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$plist")+"

log "signature (ad hoc) and architectures"
codesign --verify --deep --strict --verbose=2 "$app" || fail "codesign --verify"
codesign -dv "$app" 2>&1 | grep -E 'Identifier|Signature|flags' || true
for bin in "$exe" "$cli"; do
  [ -x "$bin" ] || fail "$bin is missing"
  archs=$(lipo -archs "$bin")
  echo "$(basename "$bin"): $archs"
  for a in $ARCHS; do
    case " $archs " in *" $a "*) ;; *) fail "$(basename "$bin") has no $a slice" ;; esac
  done
done

log "the bundled CLI"
"$cli" --version || fail "bundled freekerf --version"

export FREEKERF_HOME="$work/home"
mkdir "$FREEKERF_HOME"
"$cli" config set automation.mcp.enabled true

log "starting $exe"
out="$work/app.log"
"$exe" >"$out" 2>&1 &
app_pid=$!
t0=$SECONDS

sock="$FREEKERF_HOME/ipc/ipc.sock"
for _ in $(seq 60); do
  [ -S "$sock" ] && break
  kill -0 "$app_pid" 2>/dev/null || break
  sleep 0.5
done
[ -S "$sock" ] || {
  cat "$out"
  fail "no endpoint at $sock"
}

log "freekerf mcp attaches to the window"
reply=$(
  {
    printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}'
    printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}'
    printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
    sleep 3
  } | with_timeout 20 "$cli" mcp
) || true
echo "$reply" | grep -q '"id":2' || fail "no tools/list answer from freekerf mcp: $reply"
! pgrep -f "freekerf serve" >/dev/null || fail "freekerf mcp started its own instance"
echo "freekerf mcp attached to the window"

left=$((STAY_UP_S - (SECONDS - t0)))
if [ "$left" -gt 0 ]; then sleep "$left"; fi
kill -0 "$app_pid" 2>/dev/null || {
  cat "$out"
  fail "the app exited within ${STAY_UP_S}s"
}
echo "the app is up after $((SECONDS - t0))s"
kill "$app_pid"
wait "$app_pid" 2>/dev/null || true
app_pid=

log "OK: $(basename "$dmg")"
