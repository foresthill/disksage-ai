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
disksage scan --ai         # (v0.2) Use Claude API for contextual judgment
disksage snapshot          # Record current disk usage (for trend tracking)
disksage trend             # Show disk usage history over time
disksage help              # See all commands
```

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
| Flow-type (>500MB, <30d) | Files that grew recently — identify ongoing patterns | (report) |

Patterns are declarative JSON — community contributions welcome. See [CONTRIBUTING.md](./CONTRIBUTING.md).

---

## Roadmap

- [x] **v0.1** — Bash CLI, 10 bundled patterns, Markdown report, snapshot/trend
- [ ] **v0.2** — Claude API integration (BYOK), context-aware judgments
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
- **With `--ai`** (v0.2+): Only **filenames, paths, and metadata** are sent to Claude API. **File contents are never uploaded.** Your API key is stored in macOS Keychain / Linux secret storage, not plaintext.
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
