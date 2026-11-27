# Design: Deletion know-how as explicit, layered knowledge

Status: **design draft** (no code). Author: Claude + foresthill, 2026-11-27.
Scope: how DiskSage should represent "what is safe to delete and why" as explicit,
layered knowledge — **guardrails + collective + personal** — without violating the
project's hard rules.

> This doc exists because the next step is a *design decision*, not code. The hard
> part is not the feature; it's making layered knowledge coexist with:
> 1. **Never auto-delete** (propose only; Trash, never `rm`).
> 2. **Privacy-first** (metadata only, masked, shown before send; no file contents).
> 3. The global rule **"don't build auto-generate + frequent-human-review middle
>    designs"** — swing to an extreme (full-auto-safe OR on-demand pull), never the
>    middle. (Source: `~/.claude/CLAUDE.md`; incidents: nexus-pipeline, AI-Rules.)

---

## 1. Problem

Today, "deletion know-how" is **scattered across three places** that don't know
about each other:

- **Hardcoded in patterns** — `patterns/builtin.json` carries each pattern's
  `severity` + `action` text (e.g. "run `npm cache clean --force`"). ~23 patterns.
- **In the AI's head** — when the user asks for a judgment, the model reasons from
  masked metadata and whatever it knows. Ephemeral, not inspectable afterward.
- **In the user's notes** (#140) — per-finding free-text the user wrote; handed to
  the AI as masked context at judgment time (pull-only, never trained).

The gaps:
- The *reasoning* behind a safety rating isn't written down anywhere the user can
  read ("why is this `safe` and that `medium`?").
- There's no clean way for the community to contribute *deletion recipes* (not just
  "this dir is big" but "here's the safe way to reclaim it"), separate from code.
- Personal decisions ("I always keep my Xcode simulators") have nowhere to live
  except a free-text note.

## 2. Goal

Make deletion know-how **explicit, layered, and provenance-tagged**, so every
suggestion the user sees can answer: *where did this advice come from, and can I
trust/override it?* — while keeping all three hard rules intact.

Non-goal: making DiskSage smarter at deleting. It still never deletes; it gets
better at *explaining and sourcing* its proposals.

## 3. The three layers

Ordered from most authoritative (can't be overridden) to most personal.

### Layer 0 — Guardrails (built-in, immutable, transparent)

The safety floor. These are **rules no other layer can soften**:

- Never auto-delete; every deletion is a user click, to the Trash.
- Hard "never delete" classes: photo libraries, `com.apple.os.update-*` snapshots,
  running Docker.raw, app bundles, etc. (today these are `info`/non-`DELETABLE`).
- The MCP server exposes **no delete tool**, ever.

**Transparency requirement:** the user must be able to *read* these rules in the
app (a "Safety rules" page), not just trust them. They're already in code; this
layer is about surfacing them, not adding logic.

**Design stance:** Layer 0 is **full-auto-safe** (one extreme). It's fixed,
reviewed at the source (the repo), and applies with no per-use review. No churn.

### Layer 1 — Collective knowledge (community, opt-in, curated)

Community-contributed **patterns + deletion recipes**, building on what already
exists: `patterns/builtin.json` (bundled) + `$DISKSAGE_HOME/patterns.json` (user
/ community), licensed CC-BY-SA.

Extend the pattern schema with an optional, structured `recipe`:
```jsonc
{
  "id": "npm_cache",
  "severity": "safe",
  "action_en": "...",            // existing: short advice
  "recipe": {                    // proposed: structured, optional
    "why_safe": "download cache; your project node_modules is unaffected",
    "reclaim": "npm cache clean --force",   // a tool's OWN command, shown not run
    "caveat": "re-downloads on next install",
    "source": "https://docs.npmjs.com/cli/commands/npm-cache"  // cite it
  }
}
```

**How churn is avoided (the key point):** the collective layer is **curated at the
library level, once, via PR** — a human reviews each pattern/recipe when it's
*merged into the shared library*, not per-user or per-scan. There is **no
per-user review queue**. So this is the other extreme: **a reviewed artifact**, not
a stream of auto-generated items awaiting triage. DiskSage never auto-writes into
this layer; it only reads it and never runs a `recipe.reclaim` command itself (it's
displayed; the user runs it, or uses the Trash flow).

**Rejected alternative:** letting the AI *generate* new community patterns that get
queued for review. That's exactly the banned middle design (and a self-improving
loop where the generator and the reviewer are the same kind of model). Not doing it.

### Layer 2 — Personal knowledge (user-authored, pull-only, highest priority in advice)

The user's own context and standing decisions. **個人優先**: when personal knowledge
and collective knowledge disagree about *advice* (never about Layer 0 safety),
personal wins.

- **Notes (built, #140):** per-finding free text, handed to the AI as masked
  context at judgment time. Pull-only: read fresh, never stored back, never trained,
  shown in `/ai-preview` before send.
- **Proposed: standing decisions.** A small, explicit, user-authored set like
  "always keep: `coresimulator_devices`" or "I handle Docker myself." Stored as
  plain data (like notes), **authored by the user**, surfaced as a chip on matching
  findings ("you chose to keep this kind").

**How churn is avoided:** personal knowledge is **only ever written by the user**
(a note they type, a "keep this" they click). DiskSage **never auto-generates** a
personal rule from the user's behaviour — no "we noticed you deleted X three times,
shall we make a rule?" backlog. Pull-only, user-authored = no review queue, no
drift. (If we ever want behaviour-derived suggestions, that's a separate proposal
and must swing full-auto-safe or stay out.)

## 4. How the layers assemble into advice

When the user asks for an AI judgment (opt-in, as today), the prompt is assembled
**transparently and in priority order**:

```
[Layer 0] Guardrails:      always included, stated as hard constraints.
[Layer 1] Collective:      the matched pattern's recipe (why_safe/caveat/source),
                           only from patterns the user has (bundled + their own).
[Layer 2] Personal:        the finding's note + any standing decision (masked).
```

The AI advises; **the user decides**; deletion is Trash-only. Every rendered
suggestion carries a **provenance tag**: `built-in` / `community pattern vX` /
`your note` / `AI`. This is the "明文化" — the know-how is written down and sourced,
not implicit.

Privacy is unchanged: still masked metadata only, still shown in `/ai-preview`
before anything is sent, still BYOK / local-Ollama / Claude-CLI.

## 5. Guardrail transparency (a concrete deliverable)

A **"Safety rules" page** (`/safety` or a Settings section) that lists, in plain
language:
- The immutable Layer-0 rules and *which pattern classes* are never one-click
  deletable and why.
- For the current findings, the **provenance** of each piece of advice.
- A statement that the MCP server has no delete tool.

This turns "trust us, we're safe" into "here, read the rules." It's low-risk
(read-only rendering of rules that already exist) and high-trust — a good first
increment.

## 6. What exists vs. what's proposed

| Piece | Status |
|---|---|
| Never-auto-delete, Trash-only, safety ratings | **built** |
| Patterns externalized to JSON (+ user patterns.json, CC-BY-SA) | **built** |
| Personal notes → masked AI context, pull-only, shown in preview | **built (#140/#141)** |
| MCP read-only (no delete tool) | **built (#139/#144)** |
| `recipe` block in pattern schema (why_safe/caveat/source) | proposed (L1) |
| Standing "keep this kind" decisions | proposed (L2) |
| Provenance tags on suggestions | proposed |
| "Safety rules" transparency page | proposed (good first step) |

## 7. Hard-rule compliance checklist

- [x] **Never auto-delete** — no layer deletes; recipes are shown, not run; Trash-only.
- [x] **Privacy-first** — assembly uses masked metadata only; preview before send; no contents.
- [x] **No middle design** — L0 fixed (auto-safe extreme); L1 curated-by-PR (reviewed-artifact extreme); L2 user-authored pull-only (no review queue). Nothing auto-generates a triage backlog.
- [x] **No self-improving loop** — the AI never writes rules it will later follow; L1 is human-curated, L2 is user-authored.
- [x] **Transparent** — provenance tags + a readable Safety-rules page.

## 8. Suggested increments (smallest first)

1. **Safety-rules transparency page** — render existing Layer-0 rules + per-finding
   provenance. Read-only, no new data model. Highest trust-per-effort.
2. **`recipe` block** in the pattern schema (optional) + render `why_safe/caveat/
   source` on findings and in the AI prompt. Backward-compatible JSON.
3. **Standing "keep this kind" decisions** — user-authored, plain data, like notes.
4. (Later, only if justified) collective recipe contributions via PR to the pattern
   library — never auto-generated.

## 9. Open questions for foresthill

- Is **個人優先** the right call when personal advice contradicts a community recipe
  (never contradicts Layer 0)? I think yes — your machine, your call — but flagging.
- Should the collective layer ship recipes **in-repo** (reviewed via PR, versioned
  with the app) or support a separate community pattern feed? In-repo is simpler and
  keeps the "reviewed artifact" property; a live feed reintroduces churn risk.
- Is the "Safety rules" page worth doing as the next shipped feature (v0.1.29-ish)?
