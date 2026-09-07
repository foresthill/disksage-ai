# DiskSage menu bar plugin (SwiftBar / xbar)

Keeps your startup disk's free space **always visible in the macOS menu bar**, so
you notice space vanishing *before* it hits zero — the reason DiskSage exists.

```
💾 16.5 GB        ← turns amber at 85% used, red at 95%
```

Click it for the free/used breakdown, a local-snapshot warning when relevant, and
"Open DiskSage UI…" (launches `disksage serve`).

## How it works

The plugin calls the DiskSage engine (`disksage-engine df --json`) every 30s and
renders the startup container's free space. The engine is the same cross-platform
core being built under [`../engine`](../engine); the menu bar is its ambient layer,
the `serve` window its detail layer.

## Install

1. Install [SwiftBar](https://github.com/swiftbar/SwiftBar) (`brew install swiftbar`)
   or [xbar](https://xbarapp.com/). Both use the same plugin format.
2. Build the engine (once):
   ```bash
   cd engine && cargo build --release
   ```
3. Symlink this plugin into your SwiftBar plugin folder and make it executable:
   ```bash
   ln -s "$PWD/menubar/disksage.30s.sh" ~/SwiftBar/disksage.30s.sh
   chmod +x menubar/disksage.30s.sh
   ```
4. Refresh SwiftBar. `💾 <free>` appears in the menu bar.

The `30s` in the filename is the refresh interval — rename (e.g. `disksage.1m.sh`)
to change it.

## Binary resolution

It finds the engine (and the `disksage` CLI for "Open DiskSage UI") from, in order:
`$DISKSAGE_ENGINE` / `$DISKSAGE_BIN`, your `PATH`, the repo's release build, then
`/usr/local/bin` and `/opt/homebrew/bin`. Set the env vars in SwiftBar's plugin
environment if yours live elsewhere.

## Status

`0.0.1` — PoC. The plugin's output, engine integration and binary resolution are
verified (it prints valid SwiftBar markup and correct numbers); the menu-bar
rendering itself depends on you having SwiftBar/xbar installed. The productized
form is a Tauri tray (`TrayIcon::set_title`, macOS-supported) that embeds the
engine in-process — this shell-script plugin validates the idea first.
