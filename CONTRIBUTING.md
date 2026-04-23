# Contributing to DiskSage

Thanks for your interest in DiskSage. This project grows largely from
community-contributed detection patterns — the "oh god, *that's* what was
eating my disk" moments people hit in the wild. If you've hit one, please
share it.

---

## Ways to Contribute

- **Report bugs** via [GitHub Issues](https://github.com/foresthill/disksage-ai/issues)
- **Propose a new detection pattern** (see below — the highest-impact contribution)
- **Improve an existing pattern** (reduce false positives, sharpen the action text)
- **Port to Linux / Windows** (Phase 2+ — pattern library work welcome now)
- **Translate reports** (i18n for ja, zh, ko, es, fr, de, …)

---

## Core Principles (non-negotiable)

Any PR that violates these will be closed:

1. **Never auto-delete.** DiskSage only *suggests*. The human decides.
2. **Privacy first.** Scans never upload file *contents*. Filenames, paths, and
   metadata only — and only when `--ai` mode is explicitly enabled.
3. **Transparent.** Detection logic must be readable. No opaque binaries, no
   obfuscated rules.
4. **Safer than CleanMyMac.** Suggested deletions go through the Trash first.
   Hard-delete is an additional, explicit step.

---

## Development Setup

```bash
git clone https://github.com/foresthill/disksage-ai.git
cd disksage-ai
chmod +x disksage
./disksage version
./disksage scan --quick   # fast pass; no TCC permission dialogs
```

Requirements: Bash 4+ and Python 3 (both pre-installed on modern macOS and
most Linux distros). No other dependencies.

### Running ShellCheck locally

```bash
brew install shellcheck      # macOS
sudo apt install shellcheck  # Debian / Ubuntu
shellcheck --severity=warning disksage
```

CI runs ShellCheck at `--severity=warning`, so style nits won't block a PR
but real warnings will.

---

## Contributing a Detection Pattern

This is the most valuable kind of contribution. Each pattern is a small
function that looks at one known storage hog and emits a finding.

### Anatomy

Patterns live inside `disksage` for v0.1 (they'll move to `patterns/*.json`
in v0.2+). Each follows this shape:

```bash
check_<pattern_id>() {
  local f="$1"                      # findings file from cmd_scan
  local path size
  path="$HOME/.some/location"
  [[ -d "$path" ]] || return 0
  size=$(dir_size_bytes "$path")
  if (( size > THRESHOLD )); then
    add_finding "$f" "<pattern_id>" "$path" "$size" "<severity>" \
      "<short human description including size>" \
      "<specific command or UI step the user should take>"
  fi
}
```

Then register it in `cmd_scan`:

```bash
  check_<pattern_id>  "$findings_file"
```

### Severity Guidelines

| Severity | Symbol | When to use |
|----------|--------|-------------|
| `high`   | 🔴 | Broken or orphaned data with real risk (e.g. unrestorable backup) |
| `medium` | 🟡 | Large but legitimate items that need human review |
| `low`    | 🔵 | Developer chaff that's reinstallable (e.g. `node_modules`) |
| `safe`   | 🟢 | Caches that regenerate on demand, zero risk to delete |
| `info`   | ℹ️ | Informational / flow-type, no action suggested |

### Submission Checklist

Before opening a PR for a new pattern:

- [ ] The path(s) the pattern targets exist only when the relevant tool is installed
- [ ] The threshold is high enough to avoid false positives on small installs
- [ ] The **action** field is specific — an exact command or UI step, not "delete this"
- [ ] You tested the pattern on your own machine and pasted the resulting
      report snippet into the PR description
- [ ] You added a row to the "What DiskSage Detects" table in `README.md`

### Patterns We'd Love

- 🪟 **Windows**: Docker Desktop WSL2 VHDX, Adobe caches, game launchers
  (Steam, Epic, Battle.net), OneDrive local copies
- 🐧 **Linux**: systemd journals, Flatpak / Snap caches, Docker overlay2
- 🍎 **macOS advanced**: Xcode CoreSimulator, iOS DeviceSupport,
  `Photos.photoslibrary` originals, Mail / Messages attachments
- 🤖 **AI dev**: Hugging Face cache, Stable Diffusion checkpoints, Whisper
  model files

---

## Code Style

- Bash: Google Shell Style Guide, 2-space indent
- Reuse the existing helpers (`human_size`, `dir_size_bytes`,
  `file_size_bytes`, `tildify`, `add_finding`) rather than reimplementing
- `set -uo pipefail` **only** — not `-e` (permission errors from `find` / `du`
  are expected and should not abort the scan)
- Quote every path: `"$path"`, never `$path`
- Guard pipelines that can hit SIGPIPE: `find ... | head -1 || true`
- Function / variable names in English; comments in Japanese are welcome for
  pattern descriptions

---

## Commit / PR Conventions

- One logical change per commit (new pattern, bug fix, doc update)
- Commit subject in imperative mood, ≤ 72 chars:
  - ✅ `add pattern for Xcode CoreSimulator caches`
  - ❌ `added some new detection stuff`
- Link the related issue if one exists

---

## License

By submitting a PR you agree that:

- Code contributions are licensed under **Apache License 2.0** (project default)
- Pattern contributions (once patterns move to JSON in v0.2+) will be
  licensed under **CC-BY-SA 4.0** to keep the pattern library a community
  commons

If you're not comfortable with these terms, say so in the PR — we can discuss.

---

## Questions

- Design discussions → [GitHub Discussions](https://github.com/foresthill/disksage-ai/discussions)
- Bugs / feature requests → [GitHub Issues](https://github.com/foresthill/disksage-ai/issues)

Thanks for making DiskSage better for everyone.
