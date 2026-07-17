#!/usr/bin/env bash
# make-app.sh — build a double-clickable macOS DiskSage.app that launches
# `disksage serve` (opens the local web UI in your browser).
#
# Usage:
#   bash scripts/make-app.sh                 # builds ./DiskSage.app
#   bash scripts/make-app.sh /Applications   # builds /Applications/DiskSage.app
#
# The app is a thin launcher: double-clicking opens Terminal running
# `disksage serve` (so you see the "Serving…" line and can Ctrl-C to stop),
# which serves the report at http://127.0.0.1:8765 and opens your browser.
# It is NOT a Rust/Tauri native app — that's the v0.4 milestone. This is the
# low-cost "double-click launch" step.

set -uo pipefail

[[ "$(uname)" == "Darwin" ]] || { echo "make-app.sh is macOS-only (.app bundles)." >&2; exit 1; }

repo_root="$(cd "$(dirname "$0")/.." >/dev/null 2>&1 && pwd)"
disksage_bin="$repo_root/disksage"
[[ -x "$disksage_bin" ]] || { echo "disksage not found/executable at $disksage_bin" >&2; exit 1; }

dest_dir="${1:-$repo_root}"
app="$dest_dir/DiskSage.app"
macos="$app/Contents/MacOS"

rm -rf "$app"
mkdir -p "$macos"

# Read the version from the CLI so the bundle stays in sync.
version="$("$disksage_bin" version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)"
[[ -n "$version" ]] || version="0.1.0"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>DiskSage</string>
  <key>CFBundleDisplayName</key><string>DiskSage</string>
  <key>CFBundleIdentifier</key><string>com.disksage.app</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleExecutable</key><string>DiskSage</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>10.13</string>
  <key>NSHumanReadableCopyright</key><string>Apache-2.0</string>
</dict>
</plist>
PLIST

# Launcher — quoted heredoc keeps everything literal; the CLI path is baked in
# via a placeholder so there is no shell-quoting ambiguity.
launcher="$macos/DiskSage"
cat > "$launcher" <<'LAUNCH'
#!/bin/bash
# Prefer an installed `disksage`; fall back to the path baked at build time.
DS="$(command -v disksage 2>/dev/null)"
[ -x "$DS" ] || DS="__DISKSAGE_PATH__"
if [ ! -x "$DS" ]; then
  osascript -e 'display alert "DiskSage" message "The disksage CLI could not be found. Re-run scripts/make-app.sh from the repo, or install disksage to your PATH."'
  exit 1
fi
# Open Terminal running `disksage serve` — you see the log and can Ctrl-C to stop.
osascript -e "tell application \"Terminal\" to do script \"'$DS' serve\""
osascript -e 'tell application "Terminal" to activate'
LAUNCH

# Bake the absolute CLI path (| delimiter: repo paths never contain it).
tmp="$(mktemp)"
sed "s|__DISKSAGE_PATH__|$disksage_bin|" "$launcher" > "$tmp" && mv "$tmp" "$launcher"
chmod +x "$launcher"

# Refresh Finder/LaunchServices so the new bundle is recognized.
touch "$app"

echo "Built: $app"
echo "Try it:   open \"$app\"     (or double-click it in Finder)"
echo "Install:  mv \"$app\" /Applications/"
