# Continuation prompt — dashscene (from a Cowork session, 2026-07-11)

You're continuing work on **dashscene** (private repo `driftsys/dashscene-staging`,
already checked out here). Read these first for full context:

- `specs/DESIGN_1.md` — seed architecture
- `specs/SCOPE_DECISIONS.md` — living decisions addendum
- `AGENTS.md` — build commands & conventions

## 1. Commit the uncommitted work (a Cowork session just left it untracked)

Two additions are in the working tree, not yet committed:

- `importers/figma/plugin/fixture-author/` — dev-only Figma plugin (`manifest.json`,
  `code.js`, `README.md`) that builds the 8 tier-1 fixtures programmatically, one menu
  command per fixture, regenerable (re-run rebuilds the frame).
- `corpus/figma-fixtures/manifest.json` — maps 8 fixture names → Figma file keys,
  with coverage notes and an `emits` flag.

Commit (conventional commits; git-std pre-commit auto-formats; valid scope `repo`,
e.g. `test(repo): add Figma fixture-author dev plugin and corpus manifest`), then
push. Confirm `just verify` passes.

## 2. Reconcile specs/SCOPE_DECISIONS.md — it has diverged

The claude.ai PROJECT copy (edited in Cowork) and this REPO copy forked after §9.
Merge them:

- KEEP the repo's §10 "v0 plan tracked as GitHub epics/stories".
- ADD three sections the repo is missing (real decisions from the Cowork session;
  renumber to follow the repo's §10). Pull exact prose from the project doc if
  available; summaries:
  - **PAT + rate limits:** Figma Professional + Full seat. PATs expire 90 days (hard
    cap) → ~75-day rotation, token as GH Actions secret, nightly smoke test = token
    canary, named 401/403 diagnostic. Scopes: `file_content:read` (covers
    sharedPluginData via `?plugin_data=shared`), `file_metadata:read`,
    `library_content:read`; `file_variables:read` is Enterprise-only (unavailable).
    `GET /file` is Tier-1 = 10/min on Pro (Starter ≈ 6/MONTH → paid PLAN required).
    Importer: metadata-version-check first, serialized limiter, honor `Retry-After`.
  - **Annotator plugin deferred to v1, contract frozen now:** sharedPluginData
    namespace `"dashscene"`, keys `role={placeholder|sample-content|redline|spec}` +
    `v="1"`, reserved keys `contribution-id` (placeholder nodes only) + `material-class`
    (`lit-opaque|lit-cutout|unlit-overlay`, Unity consumer). Trigger is event-based
    (first externally-authored file), not a version. Three-channel annotation
    inventory (native structure / repo-side export manifest / sharedPluginData last).
    Plugin is in-repo + unpublished (Pro can't publish privately; distribution =
    import-from-manifest). NOTE: the §8 fixture-author plugin is a DIFFERENT plugin —
    it only creates nodes, never writes roles.
  - **Token resolution split:** phase 1 = resolved literals in `.dsb` +
    `<out>.vars.json` sidecar (derivable, R7-safe receipt; single-theme by
    construction). phase 2 = id→name/collection/mode join. KEY FINDING: on Pro
    "naming convention" is NOT an alternative route — names/modes are
    Enterprise-REST-only, so the table MUST come from the Plugin API (one more command
    on the annotator plugin → token export is the plugin's first mandatory job).
    Table is source-agnostic (Enterprise REST = drop-in later), staleness-guarded by
    file version stamp, committed as `corpus/figma-fixtures/<file>.vartable.json` for
    fixtures.
- Update §8's status: all 8 tier-1 fixtures AUTHORED, in the `dashscene-corpus` Figma
  project, keys in `corpus/figma-fixtures/manifest.json`. `effects-2025` has 3/4
  REJECT constructs (texture via plugin API; noise + progressive blur applied manually
  via the Effects panel); the 4th, variable-width stroke, has no plugin API → pending
  manual step. Plugin gotchas worth recording: GRID uses `gridColumnGap`/`gridRowGap`
  (not `itemSpacing`); a WRAP frame needs `primaryAxisSizingMode="FIXED"` after
  `layoutMode` or it hugs into one row.

## 3. Fixtures — 8 authored, keys (also in the manifest)

| fixture                   | file key                 |
| ------------------------- | ------------------------ |
| grid-basic                | `BJ1sPobsTmNLQLWqtgjwJU` |
| variables-bound           | `VtbiQejcN6gYMmeWaaEy1b` |
| effects-2025              | `43LWWEUuYJuK8iCUP9EXqU` |
| lowering-wrap             | `bInPIUfkcxNKAzBOxOOpDZ` |
| lowering-hug-in-fill      | `48AJL2nXxEYAPC423aM124` |
| lowering-negative-gap     | `w8JdmcqdvZhD1qIB8GVK2u` |
| lowering-baseline         | `R1S4QqZtT54WVcYcFFuQoD` |
| lowering-variant-topology | `CAWMOi1aDE7gTpNb6zKnxv` |

## 4. Suggested next build step — Deno capture tooling

Under `importers/figma/src/`: read `corpus/figma-fixtures/manifest.json`, fetch each
file's `GET /file` JSON with `?plugin_data=shared`, apply the PAT-section rate-limit
rules (metadata-version-check-first, serialized limiter, `Retry-After`), and write
`corpus/figma-fixtures/<name>.json` for offline record-and-replay (DESIGN §6.1). Needs
a Figma PAT (`file_content:read` + `file_metadata:read` + `library_content:read`) via
`FIGMA_TOKEN` env / GH secret — never commit it.

## 5. Still-open user actions (not code)

- Reserve on crates.io: `dashscene-typeset`, `dashscene-skia`, `dashscene-validator`
- Check `jsr.io/@driftsys` is free for `@driftsys/dashscene-figma`
- In Figma: finish `effects-2025`'s variable-width stroke (draw a line, apply a
  tapered width profile)

---

_Environment note for whoever runs this: the Cowork session that produced the fixtures
could not commit/push (its Linux VM lacks the git-std/cargo/deno toolchain and has no
network). This Claude Code session, on the real machine, is the right place to commit,
reconcile, and push._
