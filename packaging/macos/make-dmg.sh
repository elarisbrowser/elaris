#!/usr/bin/env bash
# Builds the Elaris disk image: the app beside an Applications link, in a 660x400 window.
# Usage: make-dmg.sh <Elaris.app> <output.dmg> [volume name]
# Needs create-dmg (brew install create-dmg).
set -euo pipefail

app="$1"
out="$2"
volname="${3:-Elaris}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

stage="$work/stage"
mkdir -p "$stage"
cp -R "$app" "$stage/"

rm -f "$out"
create-dmg \
  --volname "$volname" \
  --window-pos 200 120 \
  --window-size 660 400 \
  --icon-size 128 \
  --text-size 14 \
  --icon "$(basename "$app")" 165 200 \
  --hide-extension "$(basename "$app")" \
  --app-drop-link 495 200 \
  --no-internet-enable \
  "$out" \
  "$stage"
