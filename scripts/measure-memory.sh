#!/usr/bin/env bash
# Measures the resident memory (RSS) of the desktop app on macOS, including
# the WebKit processes it spawns for its web views. Those run as XPC
# services, not as children of the app, so they are found by diffing the
# WebKit processes before and after launch. Close other apps that use
# WebKit (Safari, Mail...) during the run for a clean number.
#
# Usage: scripts/measure-memory.sh [path/to/redactor-desktop] [seconds]
#        FIRST_RUN=1 scripts/measure-memory.sh   # with the dashboard open
set -euo pipefail

APP=${1:-target/release/bundle/macos/redactor.app/Contents/MacOS/redactor-desktop}
WAIT=${2:-6}
CONFIG=$(mktemp -d)
# Without a global config the app treats it as a first run and opens the
# dashboard; with one, it starts with no window at all.
[ -n "${FIRST_RUN:-}" ] || touch "$CONFIG/config.toml"

webkit() { pgrep -f 'com.apple.WebKit' | sort || true; }

before=$(webkit)
REDACTOR_CONFIG_DIR=$CONFIG "$APP" >/dev/null 2>&1 &
pid=$!
trap 'kill $pid 2>/dev/null; rm -rf "$CONFIG"' EXIT
sleep "$WAIT"
spawned=$(comm -13 <(echo "$before") <(webkit) | tr '\n' ' ')

printf '%-8s %9s  %s\n' PID RSS PROCESS
total=0
for p in $pid $spawned; do
  line=$(ps -o rss=,comm= -p "$p" 2>/dev/null) || continue
  rss=$(awk '{print $1}' <<<"$line")
  name=$(awk '{ $1=""; print }' <<<"$line" | xargs basename)
  printf '%-8s %7.1f MB  %s\n' "$p" "$(bc <<<"scale=1; $rss/1024")" "$name"
  total=$((total + rss))
done
printf '%-8s %7.1f MB\n' TOTAL "$(bc <<<"scale=1; $total/1024")"
