#!/usr/bin/env bash
# Smoke test of a Linux package (release.yml): it installs, the app starts on a
# fresh FREEKERF_HOME under Xvfb and is still running after 15 s, the bundled
# `freekerf` CLI answers, and it uninstalls. Runs as root in a clean container
# (Debian/Ubuntu or Fedora) or, for the Flatpak, on the runner with sudo.
#
#   smoke-linux-installer.sh deb      <file.deb>       # apt install, run, apt remove
#   smoke-linux-installer.sh appimage <file.AppImage>  # --appimage-extract-and-run
#   smoke-linux-installer.sh flatpak  <file.flatpak>   # bundle install, run, MCP attach
set -euo pipefail

kind=${1:?kind: deb, appimage or flatpak}
file=$(realpath "${2:?package file}")
APP_ID=io.github.freekerf.FreeKerf
STAY_UP_S=15

log() { printf '\n== %s\n' "$*"; }
fail() {
  printf '\nFAILED: %s\n' "$*" >&2
  exit 1
}

# xvfb-run and what a desktop session usually has (fonts, Mesa for WebKit's EGL)
install_x() {
  if command -v xvfb-run >/dev/null && command -v pgrep >/dev/null; then return; fi
  log "installing Xvfb"
  if command -v apt-get >/dev/null; then
    apt-get update -qq
    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends \
      xvfb xauth procps fonts-dejavu-core libgl1-mesa-dri ca-certificates >/dev/null
  elif command -v dnf >/dev/null; then
    dnf install -y -q xorg-x11-server-Xvfb /usr/bin/xvfb-run xauth procps-ng util-linux \
      dejavu-sans-fonts mesa-dri-drivers mesa-libEGL mesa-libgbm >/dev/null
  else
    fail "no apt-get or dnf to install Xvfb"
  fi
}

# Starts "$@" under Xvfb in its own process group (APP_PID, APP_OUT, APP_T0).
start_app() {
  APP_OUT=$(mktemp)
  log "starting $*"
  setsid xvfb-run -a -s "-screen 0 1280x800x24" "$@" >"$APP_OUT" 2>&1 &
  APP_PID=$!
  APP_T0=$SECONDS
}

# Checks that freekerf-desktop is still running STAY_UP_S after the start, then
# stops the process group.
still_up_then_stop() {
  local left=$((STAY_UP_S - (SECONDS - APP_T0)))
  if ((left > 0)); then sleep "$left"; fi
  if ! pgrep -f freekerf-desktop >/dev/null; then
    cat "$APP_OUT"
    fail "freekerf-desktop exited within ${STAY_UP_S}s"
  fi
  echo "freekerf-desktop is up after $((SECONDS - APP_T0))s"
  kill -- "-$APP_PID" 2>/dev/null || true
  wait "$APP_PID" 2>/dev/null || true
  pkill -f freekerf-desktop 2>/dev/null || true
  # WebKit/EGL complaints are reported, not fatal: the window may still work
  if grep -iE 'egl|dmabuf|gbm|webkit|error' "$APP_OUT" >/dev/null; then
    echo "-- app output (EGL/WebKit lines):"
    grep -iE 'egl|dmabuf|gbm|webkit|error' "$APP_OUT" | head -40
  fi
}

case "$kind" in
deb)
  install_x
  pkg=$(dpkg-deb -f "$file" Package)
  log "apt install $file (package $pkg)"
  apt-get install -y -qq "$file" >/dev/null
  command -v freekerf-desktop >/dev/null || fail "freekerf-desktop not on PATH"
  freekerf --version || fail "freekerf --version"
  test -f /usr/share/metainfo/$APP_ID.metainfo.xml || fail "metainfo not installed"
  FREEKERF_HOME=$(mktemp -d) start_app freekerf-desktop
  still_up_then_stop
  log "apt remove $pkg"
  apt-get remove -y -qq "$pkg" >/dev/null
  # the files, not `command -v`: bash remembers where it found them
  test ! -e /usr/bin/freekerf-desktop || fail "freekerf-desktop still installed"
  test ! -e /usr/bin/freekerf || fail "freekerf still installed"
  ;;

appimage)
  install_x
  chmod +x "$file"
  work=$(mktemp -d)
  (cd "$work" && "$file" --appimage-extract >/dev/null)
  "$work/squashfs-root/usr/bin/freekerf" --version || fail "bundled freekerf --version"
  rm -rf "$work"
  # no FUSE in containers: extract and run
  FREEKERF_HOME=$(mktemp -d) start_app "$file" --appimage-extract-and-run
  still_up_then_stop
  ;;

flatpak)
  install_x
  export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/tmp/xdg-runtime}
  mkdir -p -m 700 "$XDG_RUNTIME_DIR"
  log "flatpak install $file"
  flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
  flatpak install --user -y --noninteractive "$file"
  flatpak run --command=freekerf $APP_ID --version || fail "freekerf --version in the sandbox"
  # agents attach to the running window through $XDG_RUNTIME_DIR/app/$APP_ID
  flatpak run --command=freekerf $APP_ID config set automation.mcp.enabled true
  start_app flatpak run $APP_ID
  sock="$XDG_RUNTIME_DIR/app/$APP_ID/ipc/ipc.sock"
  for _ in $(seq 30); do
    test -S "$sock" && break
    sleep 0.5
  done
  test -S "$sock" || {
    cat "$APP_OUT"
    fail "no endpoint at $sock"
  }
  log "freekerf mcp in a second sandbox attaches to the window"
  reply=$(
    {
      printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}'
      printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}'
      printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
      sleep 3
    } | timeout 20 flatpak run --command=freekerf $APP_ID mcp
  ) || true
  echo "$reply" | grep -q '"id":2' || fail "no tools/list answer from freekerf mcp: $reply"
  ! pgrep -f "freekerf serve" >/dev/null || fail "freekerf mcp started its own instance"
  echo "freekerf mcp attached to the window"
  still_up_then_stop
  log "flatpak uninstall"
  flatpak uninstall --user -y --noninteractive $APP_ID
  ;;

*)
  fail "unknown kind $kind"
  ;;
esac

log "OK: $kind $(basename "$file")"
