#!/usr/bin/env bash
#
# DiskSage — menu bar plugin for SwiftBar / xbar.
#
# Shows your startup disk's free space in the macOS menu bar, always visible, so
# you notice space vanishing *before* it hits zero — the whole reason DiskSage
# exists. Refreshes every 30s (encoded in the filename: disksage.30s.sh).
#
# <xbar.title>DiskSage Free Space</xbar.title>
# <xbar.version>v0.0.1</xbar.version>
# <xbar.author>DiskSage</xbar.author>
# <xbar.desc>Startup disk free space in the menu bar, fed by the DiskSage engine.</xbar.desc>
# <xbar.dependencies>disksage-engine</xbar.dependencies>
#
# Install: copy or symlink this file into your SwiftBar plugin folder, e.g.
#   ln -s "$PWD/menubar/disksage.30s.sh" ~/SwiftBar/disksage.30s.sh
# then chmod +x it. Get SwiftBar: https://github.com/swiftbar/SwiftBar
#
# It resolves the engine from $DISKSAGE_ENGINE, your PATH, or the repo's
# release build. Build the engine first with:  cd engine && cargo build --release

# --- resolve a DiskSage binary ----------------------------------------------
resolve() {  # $1 = binary name, $2 = optional explicit override path
  local name="$1" override="$2" c
  for c in "$override" "$(command -v "$name" 2>/dev/null)" \
           "$HOME/Development/AI-Driven/disksage/engine/target/release/$name" \
           "$HOME/Development/AI-Driven/disksage/$name" \
           "/usr/local/bin/$name" "/opt/homebrew/bin/$name"; do
    [ -n "$c" ] && [ -x "$c" ] && { printf '%s' "$c"; return 0; }
  done
  return 1
}

ENGINE="$(resolve disksage-engine "$DISKSAGE_ENGINE")"
if [ -z "$ENGINE" ]; then
  echo "💾 —"
  echo "---"
  echo "DiskSage engine not found"
  echo "Build it: cd engine && cargo build --release | color=gray"
  exit 0
fi

JSON="$("$ENGINE" df --json 2>/dev/null)"

# --- parse the startup container (free/pct/total/snapshots) ------------------
PARSED="$(printf '%s' "$JSON" | python3 -c '
import sys, json
try:
    d = json.load(sys.stdin)
except Exception:
    print("NA"); sys.exit()
cs = d.get("containers", [])
startup = next((c for c in cs if "Startup" in c.get("label", "")), cs[0] if cs else None)
if not startup:
    print("NA"); sys.exit()
def h(b):
    u = ["B","KB","MB","GB","TB","PB"]; n = float(b); i = 0
    while n >= 1024 and i < len(u)-1:
        n /= 1024; i += 1
    return ("%d %s" % (int(n), u[i])) if i == 0 else ("%.1f %s" % (n, u[i]))
snaps = d.get("snapshots")
snaps = "n/a" if snaps is None else str(snaps)
print("%s\t%d\t%s\t%s" % (h(startup["free"]), startup["pct"], h(startup["total"]), snaps))
' 2>/dev/null)"

if [ -z "$PARSED" ] || [ "$PARSED" = "NA" ]; then
  echo "💾 ?"
  echo "---"
  echo "Could not read disk usage | color=gray"
  exit 0
fi

IFS=$'\t' read -r FREE PCT TOTAL SNAPS <<EOF
$PARSED
EOF

# Color the title by fullness: red when nearly full, amber when getting there.
COLOR=""
if [ "$PCT" -ge 95 ]; then
  COLOR=" color=red"
elif [ "$PCT" -ge 85 ]; then
  COLOR=" color=orange"
fi

# --- menu bar title + dropdown ----------------------------------------------
echo "💾 ${FREE}|${COLOR}"
echo "---"
echo "DiskSage — startup disk | font=Menlo"
echo "Free: ${FREE}  (${PCT}% used of ${TOTAL})"
if [ "$SNAPS" != "0" ] && [ "$SNAPS" != "n/a" ]; then
  echo "Local snapshots: ${SNAPS} (can hold space) | color=orange"
fi
echo "---"

# "Open DiskSage" launches the bash CLI's local web UI, if the CLI is present.
DISKSAGE="$(resolve disksage "$DISKSAGE_BIN")"
if [ -n "$DISKSAGE" ]; then
  echo "Open DiskSage UI… | bash=\"$DISKSAGE\" param1=serve terminal=false"
fi
echo "Refresh | refresh=true"
