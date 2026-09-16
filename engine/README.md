# disksage-engine (Rust port — PoC)

Phase B, step 1 of DiskSage: begin porting the macOS-only bash engine to Rust so
it can run natively on **macOS, Windows and Linux** — and eventually be called
in-process by the Tauri desktop app instead of shelling out to `disksage serve`.

This is a **proof of concept**, not a replacement yet. It reproduces one command
(`df`) so its output can be diffed against the bash implementation and the port
can grow one command at a time while staying verifiable.

## Try it

```bash
cargo run --release -- df            # instant disk view, like `disksage df`
cargo run --release -- df --json
cargo run --release -- scan          # directory-size findings, like `disksage scan`
cargo run --release -- scan --json
```

## What works

- **Cross-platform disk enumeration** via [`sysinfo`](https://crates.io/crates/sysinfo)
  — one code path for all three OSes (the whole reason for the Rust port).
- Container-level "X% full" (volumes grouped by total size, like the bash
  `render_disk_usage`).
- APFS local snapshot count on macOS (behind a `cfg(target_os = "macos")` gate;
  other platforms report "n/a" rather than shelling out).
- **`scan`** — directory/file-size patterns, emitting findings (id, path, size,
  severity, description, action) as text or JSON. Cross-platform first, then
  macOS paths that are simply skipped when absent (so a Linux run just won't see
  them):
  - `ollama_models` (`~/.ollama/models` > 10 GB) — cross-platform
  - `user_cache` (`~/.cache` > 5 GB) — cross-platform
  - `node_modules_aggregate` (`~/Development` > 10 GB) — cross-platform
  - `library_caches` (`~/Library/Caches` > 5 GB) — macOS
  - `xcode_derived_data` (`…/Xcode/DerivedData` > 5 GB) — macOS
  - `ios_devicesupport` (`…/Xcode/iOS DeviceSupport` > 3 GB) — macOS
  - `coresimulator_caches` (`…/CoreSimulator/Caches` > 1 GB) — macOS
  - `docker_raw` (`…/com.docker.docker/…/Docker.raw` > 10 GB) — macOS
  - New patterns are one `dir_pattern(...)` / `finding_if_over(...)` line each.
  - Directory sizing counts allocated blocks (`st_blocks`) on Unix to match `du`,
    and falls back to logical length on Windows; symlinks are not followed and
    unreadable entries are skipped (the bash engine's error tolerance).

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

For `scan`, the first sizing attempt (logical file length) came in ~1.3 GiB under
`du`; switching to allocated blocks (`st_blocks × 512`) on Unix closed the gap —
Rust 17.6 GiB vs bash `du` 17.4 GiB, the ~0.17 GiB residual being live changes in
`~/Development` between the two measurements, not a systematic error.

## Status

`0.0.x` — PoC. Not wired into the CLI or the desktop app. Next candidates to
port: more `scan` patterns — the remaining OS-agnostic ones, then macOS-specific
ones behind `cfg` gates (iOS backups, CoreSimulator, tmutil snapshots).
