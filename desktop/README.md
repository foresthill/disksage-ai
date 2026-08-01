# DiskSage Desktop (Tauri) — scaffold

A native macOS/Linux/Windows window that displays the DiskSage web UI. This is
**approach A**: instead of a browser tab, a Tauri window shows the exact same
page that `disksage serve` produces. On launch the app starts `disksage serve`
(with its browser auto-open suppressed) and loads `http://127.0.0.1:8765`
inside the window; on quit it stops the server.

> **Status: scaffold.** The project files are complete and config-validated, but
> the first `tauri build` has not been run yet (it downloads ~1–2 GB of Rust
> crates and takes 10+ minutes). Build it when you have disk/time headroom.
> The native, from-scratch UI (cards, no embedded web) is the later v0.4 goal —
> this shell is the fast first step.

## Prerequisites

- The **`disksage` CLI on your PATH** (the window stays on the loading screen
  without it — the app spawns `disksage serve`). Install it, e.g.
  `sudo cp ../disksage /usr/local/bin/`.
- **Rust** (`rustc`/`cargo`) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/).
- **Node** (or **bun**) for the Tauri CLI dev-dependency.
- Tauri CLI — either `cargo install tauri-cli --version '^2' --locked`, or use
  the bundled dev-dependency via `bun install` below.

## Run / build

```bash
cd desktop
bun install                 # installs @tauri-apps/cli (or: npm install)

bun run tauri dev           # dev window (first run compiles Rust — several minutes)
bun run tauri build         # release .app / installer under src-tauri/target/release/bundle/
```

The first build is the slow one (downloads and compiles the Rust dependency
tree). Afterwards, `src-tauri/target/` can be reclaimed with `cargo clean` if
you need the disk back — fittingly, `disksage top` will show it.

## How it works

- `src-tauri/src/lib.rs` — on `setup`, spawns `disksage serve` with
  `DISKSAGE_NO_BROWSER=1`; kills it on `RunEvent::Exit`.
- `src/index.html` — a loading page that polls `127.0.0.1:8765` and navigates to
  it once the server (and its first scan) is ready.
- `src-tauri/tauri.conf.json` — window `main`, product name **DiskSage**.

## Notes / next steps

- Port is currently fixed at `8765` (serve's default). Making it dynamic and
  handing it to the window would remove that assumption.
- Bundling the `disksage` CLI inside the app (sidecar) would drop the "on PATH"
  requirement — a good follow-up before distributing.
