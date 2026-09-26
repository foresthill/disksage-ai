# DiskSage patterns

Simple "a directory over a size threshold" detection rules live here as data, not
code. `builtin.json` is bundled into the engine at build time; you can also add
your own without rebuilding by creating **`$DISKSAGE_HOME/patterns.json`**
(default `~/.disksage/patterns.json`) — same format, merged after the built-ins.

## Format

A JSON array of objects:

```json
{
  "id": "ollama_models",
  "path": "~/.ollama/models",
  "threshold_gib": 10,
  "severity": "medium",
  "label_en": "Ollama models",
  "label_ja": "Ollama モデル",
  "action_en": "Remove unused models with 'ollama rm <model>'.",
  "action_ja": "未使用モデルを 'ollama rm <model>' で削除。"
}
```

| field | meaning |
|---|---|
| `id` | unique id (snake_case) |
| `path` | a single `~`-relative path (sugar for a 1-element `paths`) |
| `paths` | alternative paths, tried in order; the first existing directory wins |
| `threshold_gib` | flag when the directory is larger than this many GiB |
| `severity` | `high` \| `medium` \| `low` \| `safe` \| `info` (anything else → `info`) |
| `label_en` / `label_ja` | short name; the finding text is `"<label>: <size>"` |
| `action_en` / `action_ja` | one-line advice |

Notes:
- A path that doesn't exist is simply skipped (so macOS-only paths are inert on
  Linux/Windows).
- Only the directory *size* is measured; DiskSage never deletes automatically.
- Patterns that need real logic (backup-corruption checks, per-app cache sums,
  snapshots, swap, aggregate walks) are implemented in the engine, not here.

## Contributing

Add a rule to `builtin.json` and open a PR. Pattern definitions are licensed
**CC-BY-SA 4.0** (the code is Apache-2.0).
