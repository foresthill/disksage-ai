# 🩺 DiskSage

**AI が提案する、優しい容量管理 CLI / App.**

Your disk is full again. Which files are safe to delete? DiskSage tells you — it never deletes anything on its own.

> **“100MB空けたら100MB増える。CleanMyMacは勝手に判断しすぎる。
> でも手作業で `du -sh` と `find` を叩き続けるのもしんどい。”**
> 
> DiskSage は、その間を埋めるツールです。

---

## Why DiskSage?

| | CleanMyMac | DaisyDisk | `du`/`find` | **DiskSage** |
|---|---|---|---|---|
| Auto-delete | ✅ (怖い) | ❌ | ❌ | **❌ (Never)** |
| Visualization | Sizes only | Sunburst | — | Categorized by pattern |
| AI judgment | Rule-based | — | — | **Claude API (optional)** |
| Price | ¥5,900/yr | ¥1,500 | Free | **Free (OSS) + Pro** |
| Open Source | ❌ | ❌ | ✅ | **✅ Apache-2.0** |
| Cross-platform | macOS only | macOS only | macOS/Linux | **macOS + Linux (Win coming)** |

**Core philosophy:**

1. **Never delete.** DiskSage only *suggests*. You decide.
2. **Context over size.** Understand *why* a file is big, not just *that* it's big.
3. **Detect flows, not just stocks.** Catch the "60GB disappeared overnight" mystery.
4. **Transparent.** OSS. Your data never leaves your machine (unless you opt-in to AI mode, and even then only filenames are sent).

---

## Quick Start

```bash
# Clone
git clone https://github.com/<your-org>/disksage.git
cd disksage

# Install (just copy to PATH)
sudo cp disksage /usr/local/bin/
chmod +x /usr/local/bin/disksage

# Run your first scan
disksage scan

# Open the report
open ~/.disksage/scans/*.md
```

No dependencies beyond Bash + Python 3 (pre-installed on macOS).

---

## Commands

```bash
disksage scan              # Scan known heavy hitters, output Markdown report
disksage scan --ai         # Add a Claude AI assessment per finding (BYOK)
disksage scan --html       # Also write a styled, self-contained HTML report
disksage scan --quick      # Skip the $HOME flow-type pass (no TCC dialogs)
disksage serve             # Scan + open the HTML report as a local web UI
disksage snapshot          # Record current disk usage (for trend tracking)
disksage trend             # Show disk usage history over time
disksage help              # See all commands
```

### Local web UI (`serve`)

`disksage serve` runs a scan, then serves the HTML report at
`http://127.0.0.1:8765` and opens your browser. A **Re-scan** button re-runs the
scan and refreshes the page. It's the lightest step toward a GUI — bound to
`127.0.0.1` only (not reachable from the network), Python standard library only.

**Quick-delete panel** — tick the findings you want gone and **Move selected to
Trash** (recoverable — never `rm`). Only directly-deletable patterns are offered
(caches, DerivedData, Simulator data, iPhone backups, Ollama models); patterns
where the path isn't the right target (Electron app dirs, Docker.raw,
`node_modules` aggregate) or that need other tools (snapshots, swap) stay
manual. Higher-risk items show ⚠ and ask for confirmation twice. Consistent with
the core rule: DiskSage never deletes anything on its own.

```bash
disksage serve                 # full scan, serve, open browser
disksage serve --quick         # skip the slow $HOME flow-type pass
disksage serve --ai --yes      # include the AI assessment (--yes: pre-confirm sending)
disksage serve --port 9000     # custom port (or DISKSAGE_PORT)
```

Press Ctrl-C to stop. The first scan (and each Re-scan) takes as long as a normal
scan — on machines with large `node_modules` trees that can be ~30s.

### HTML report (`--html`)

`--html` writes a styled, **self-contained** `.html` next to the Markdown report
(color-coded severity cards, AI recommendation badges, and disk-usage bars). No
external assets — open it offline by double-clicking. Localized like the report
(`DISKSAGE_LANG`). A lightweight step toward the planned GUI.

```bash
disksage scan --ai --html
open ~/.disksage/scans/*.html
```

### AI mode (`--ai`)

`--ai` adds a contextual assessment from Claude to every finding — *safe to
delete*, *archive then delete*, *review first*, or *keep* — with a one-line
reason. It's **bring-your-own-key**, and works with either **Anthropic direct**
or **OpenRouter** (both speak the Anthropic Messages API).

```bash
# Option A — Anthropic direct (default model: claude-opus-4-8)
export ANTHROPIC_API_KEY=sk-ant-...      # https://console.anthropic.com/
disksage scan --ai

# Option B — OpenRouter (default model: anthropic/claude-opus-4.8)
export OPENROUTER_API_KEY=sk-or-...      # https://openrouter.ai/keys
disksage scan --ai

# Non-interactive (e.g. cron): skip the send-confirmation prompt
disksage scan --ai --yes

# Use a cheaper/faster model
DISKSAGE_MODEL=claude-haiku-4-5            disksage scan --ai   # Anthropic
DISKSAGE_MODEL=anthropic/claude-haiku-4.5 disksage scan --ai   # OpenRouter
```

If both keys are set, Anthropic is used; force one with
`DISKSAGE_AI_PROVIDER=anthropic|openrouter`.

### Report language

The Markdown report is localized. It follows your system locale (`$LANG`), or
set it explicitly:

```bash
DISKSAGE_LANG=ja disksage scan        # 日本語レポート（--ai なしでも全文日本語）
DISKSAGE_LANG=en disksage scan        # English
```

Japanese is fully built in (headings, findings, actions, AI section, next
steps). Other language codes localize the AI reasoning text; report scaffolding
falls back to English. CLI messages and `--help` stay English for now.

**What's sent, and what isn't:**

- Paths are **masked** first: your username (`/Users/you/…` → `~/…`) and any
  folder *you* named (`~/Documents/Acme/Q3/…` → `~/Documents/<dir1>/<dir2>/…`)
  are anonymized. Known tool/vendor locations (`Library`, `iMobie`, `.ollama`,
  `node_modules`, …) are kept so the model can still judge accurately.
- Only the masked path, size, and DiskSage's own one-line description are sent.
  **File contents are never read or transmitted.**
- Before anything leaves your machine, DiskSage prints the **exact masked
  payload** and asks you to confirm. The report you get back shows the *real*
  local paths (for your eyes only); only the masked form was sent.
- If the key is missing or the request fails, the standard offline report is
  still produced.

**Audit the masking (`--ai-log`)** — to see *exactly* what left your machine and
how well masking worked, add `--ai-log` (or set `DISKSAGE_AI_LOG=1`):

```bash
disksage scan --ai --yes --ai-log
# → ~/.disksage/ai-logs/<time>/
```

| File | Contents |
|---|---|
| `request.json` | The exact request body sent to the API (already masked) |
| `response.json` | The raw API response |
| `masking.tsv` | `real_path → masked_path` per finding + an `anonymized` flag |

`masking.tsv` is a before/after table — e.g. `~/Documents/Acme/Q3/… → ~/Documents/<dir2>/<dir3>/…` — so you can quantify how much was anonymized. **`masking.tsv` contains real local paths — it's a local audit file, never shared.**

### Track "sudden growth" events

Set up a cron job to snapshot every hour — next time your disk fills up mysteriously, you'll know when and by how much.

```bash
(crontab -l 2>/dev/null; echo "0 * * * * /usr/local/bin/disksage snapshot") | crontab -

# Later, view the trend
disksage trend
```

---

## What DiskSage Detects (v0.1)

Known patterns bundled in the initial release:

| Pattern | Description | Severity |
|---|---|---|
| APFS snapshots | macOS system snapshots that keep deleted files alive | 🔴 High |
| iPhone backups (broken) | AnyTrans/iMobie backups without `Manifest.db` (can't restore) | 🔴 High |
| iPhone backups (valid) | Full device backups occupying GBs | 🟡 Medium |
| Docker.raw | Docker Desktop's virtual disk (never shrinks automatically) | 🟡 Medium |
| Ollama models | Local LLM model blobs (often forgotten) | 🟡 Medium |
| node_modules (aggregate) | Total size across all projects in `~/Development` | 🔵 Low |
| macOS VM swap | Swap files (reboot to reclaim) | 🟡 Medium |
| Electron caches | Browser/Electron app caches (safe to delete) | 🟢 Safe |
| Xcode DerivedData | Xcode build artifacts (safe to delete) | 🟢 Safe |
| Xcode Simulator caches | CoreSimulator caches (regenerated on demand) | 🟢 Safe |
| Xcode Simulator devices | Old/unused simulator devices (deleting wipes their state) | 🟡 Medium |
| Xcode iOS DeviceSupport | Debug symbols, re-downloaded on device connect | 🟢 Safe |
| Flow-type (>500MB, <30d) | Files that grew recently — identify ongoing patterns | (report) |

Patterns are declarative JSON — community contributions welcome. See [CONTRIBUTING.md](./CONTRIBUTING.md).

---

## Roadmap

- [x] **v0.1** — Bash CLI, 10 bundled patterns, Markdown report, snapshot/trend
- [x] **v0.2** — Claude API integration (BYOK), context-aware judgments (`--ai`)
- [ ] **v0.3** — Rust core rewrite for speed + Windows support
- [ ] **v0.4** — Tauri-based GUI (Tinder-style cards for approval)
- [ ] **v0.5** — Background watcher, Sudden Growth Detector
- [ ] **v0.6** — Pattern marketplace / community contributions
- [ ] **v1.0** — Pro tier (managed API, multi-device sync, analytics dashboard)

---

## Example Report

```markdown
# 🩺 DiskSage Report
Generated: 2026-04-20 14:23:10

## 📊 Current Disk Usage
Filesystem       Size    Used   Avail   Capacity
/dev/disk3s1s1   460Gi   415Gi  45Gi    92%

## 🎯 Findings

### 🔴 High Priority (1 item)
- **iPhone backup: 39.2 GB (BROKEN: no Manifest.db found, cannot restore)**
  - Path: `~/Library/Application Support/iMobie`
  - Size: 39.2 GB
  - Action: Review backup, archive to external drive if needed, then delete

### 🟡 Medium Priority (2 items)
- **Ollama models total 10.7 GB**
  - Action: Run 'ollama list' and 'ollama rm <model>' for unused ones
- **macOS swap files: 22.6 GB**
  - Action: Reboot your Mac to reclaim swap space

### 🟢 Safe to Delete (3 items)
- **Claude / Cache: 1.2 GB** — Safe to delete (will be regenerated)
- **Code / Code Cache: 680 MB** — Safe to delete (will be regenerated)
- **Google / GoogleUpdater cache: 670 MB** — Safe to delete

## 🤖 AI Assessment   (only with --ai)
_Contextual judgment from Claude. Metadata only was sent; you decide._

- 🟡 **Archive, then delete** (high confidence) — iPhone backup: 39.2 GB (BROKEN: no Manifest.db)
  - AI reasoning: Without Manifest.db this backup can't be restored, but copy out any wanted photos before reclaiming the space.
- 🟢 **Safe to delete** (high confidence) — Claude / Cache: 1.2 GB
  - AI reasoning: Application cache, regenerated automatically on next launch.

## 📈 Recently Grown Files (Flow-type, last 30 days)
8.9G  ~/.ollama/models/blobs/sha256-4c27...
7.7G  ~/Library/Application Support/Claude/vm_bundles/claudevm.bundle/rootfs.img
4.3G  ~/Documents/IGREK/HHMS/4_撮影/...

## 💡 Next Steps
1. Review the findings above
...
```

---

## Privacy & Data

- **Without `--ai`**: 100% offline. No network calls. Nothing leaves your machine.
- **With `--ai`**: Only **masked paths, sizes, and DiskSage's own descriptions** are sent to the Claude API. Paths are anonymized first (username and your own folder names removed; known tool/vendor names kept). **File contents are never read or uploaded.** You see the exact masked payload and confirm before anything is sent.
- **API key**: bring-your-own-key via `ANTHROPIC_API_KEY` (Anthropic direct) or `OPENROUTER_API_KEY` (OpenRouter). DiskSage never writes your key to disk. (OS keychain integration is on the roadmap.)
- **Snapshots** (trend data) are stored locally in `~/.disksage/trends/`.

---

## Contributing

DiskSage grows through community patterns. If you've hit a storage hog that DiskSage didn't catch, please add a pattern!

See [CONTRIBUTING.md](./CONTRIBUTING.md) for the pattern format and PR flow.

Priority contributions:
- 🪟 **Windows pattern library** (Docker Desktop WSL2 VHDX, Adobe caches, game stores)
- 🐧 **Linux pattern library** (systemd journals, Flatpak, Docker)
- 🍎 **macOS advanced** (Xcode CoreSimulator, Photos.photoslibrary optimization)
- 🌏 **Localized reports** (i18n for ja, zh, ko, es, fr, de...)

---

## License

Apache License 2.0 — see [LICENSE](./LICENSE).

Pattern library is CC-BY-SA 4.0 to encourage sharing as a community commons.

---

## Born from real pain

DiskSage was built after its author spent an afternoon with Claude, discovering:

- 39 GB orphaned iPhone backup from a Chinese app installed 6 months ago and forgotten
- 22 GB of macOS swap because the Mac hadn't been rebooted in weeks
- "60GB freed, 60GB gone overnight" — turned out to be Electron caches + LLM model blobs

Every detected pattern comes from a real "oh god, *that's* what was eating my disk" moment.

If DiskSage saves you that afternoon, that's the whole point.

— *[Founder name]*
