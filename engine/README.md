# disksage-engine (Rust port — PoC)

Phase B, step 1 of DiskSage: begin porting the macOS-only bash engine to Rust so
it can run natively on **macOS, Windows and Linux** — and eventually be called
in-process by the Tauri desktop app instead of shelling out to `disksage serve`.

This is a **proof of concept**, not a replacement yet. It reproduces one command
(`df`) so its output can be diffed against the bash implementation and the port
can grow one command at a time while staying verifiable.

## Try it

```bash
cargo run --release -- df          # human-readable, like `disksage df`
cargo run --release -- df --json   # machine-readable, for diffing
```

## What works

- **Cross-platform disk enumeration** via [`sysinfo`](https://crates.io/crates/sysinfo)
  — one code path for all three OSes (the whole reason for the Rust port).
- Container-level "X% full" (volumes grouped by total size, like the bash
  `render_disk_usage`).
- APFS local snapshot count on macOS (behind a `cfg(target_os = "macos")` gate;
  other platforms report "n/a" rather than shelling out).

## Verified findings (why the PoC matters)

Diffed against `disksage df` on a real Mac:

| | Rust (`sysinfo`) | bash (`df -Pk`) |
| --- | --- | --- |
| Container fullness | ✅ close (timing-driven differences) | reference |
| Per-volume breakdown | ⚠️ `used` collapses to the container total; some volumes omitted | accurate per-volume |
| Second (xarts) container | ⚠️ not enumerated | listed |
| Snapshot count | ✅ matches | reference |

**Takeaway for the port:** `sysinfo` gives a correct headline "X% full" on every
OS, but on macOS APFS it does not reproduce `df -Pk`'s faithful per-volume
breakdown. Where that detail matters, the Unix path should read `df` / `statvfs`
per mount; `sysinfo` remains the portable fallback (and the Windows path).

## Status

`0.0.x` — PoC. Not wired into the CLI or the desktop app. Next candidates to
port: `scan` pattern checks (starting with the OS-agnostic ones).
