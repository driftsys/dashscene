# S2 — Figma REST parse robustness: unknown enum → named diagnostic, not a crash

    status   design (working memory); human-approved 2026-07-18
    story    S2 / #311 of the "full real-file import" epic
             (ledger .superpowers/sdd/epic-progress.md)
    scope    crates/dashc/src/figma (rest.rs parse, mod.rs walk)
    base     main 63954d0

## Why

The hero now reaches `dashc` (after S3), but dies with a HARD serde crash:
`unknown variant STRETCH, expected FILL/FIT/CROP/TILE` (an image paint
scaleMode). This aborts the whole compile (`CompileError::Parse`) and masks the
rest of the hero's frontier. Under partial-emit a construct the document cannot
express must be a **skip-with-warning**, never a hard crash — so an unknown enum
variant must degrade to a named `figma.unsupported`, not abort the parse.

## The crash surface (grounded, cite before editing)

`crates/dashc/src/figma/rest.rs` models exactly **three** serde-strict enums —
fieldless, `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]`, crash on any unknown
value. Everything else is already `String` (tolerant, diagnosed at the walk —
the file's dominant idiom, ~20 fields):

| Enum          | rest.rs  | Variants                                            | Field               | Crashes on    |
| ------------- | -------- | --------------------------------------------------- | ------------------- | ------------- |
| `PaintTag`    | ~409-418 | Solid, GradientLinear/Radial/Angular/Diamond, Image | `Paint.kind`        | `PATTERN`     |
| `ScaleMode`   | ~420-427 | Fill, Fit, Crop, Tile                               | `Paint.scale_mode`  | `STRETCH`     |
| `StrokeAlign` | ~429-435 | Inside, Center, Outside                             | `Node.stroke_align` | any new align |

## Design

### 1. Convert the three enums to tolerant `String`

Delete the three enum definitions; make `Paint.kind`, `Paint.scale_mode`, and
`Node.stroke_align` `String` (matching the file's ~20 other open-vocabulary
`String` fields). Update the module doc at rest.rs:8-10, which currently argues
_for_ strict enums ("an unknown value fails the parse rather than silently
lowering to a default") — that rationale is superseded: the walk's **named**
catch-all is not a silent default (P4).

Rejected alternatives: `#[serde(other)]` (loses the value → the diagnostic can't
say "STRETCH", failing P4); a custom `Deserialize`/`Unknown(String)` (introduces
the first serde catch-all in the codebase — no precedent — and drops `Copy`).
`String` + walk-catch-all is the repo's established pattern.

### 2. A named catch-all arm at each of the three walk sites

Parse keeps the string; the walk decides the verdict (P5 — the producer owns its
diagnostics). Add one match arm per enum in `crates/dashc/src/figma/mod.rs`
(re-verify exact lines against the current tree):

- **paint type** — `Walk::paint_kind` (`match paint.kind`, ~998): known strings
  map as today; an unknown → `CompileError::Unsupported { what: "a PATTERN paint" }`
  (name the actual value).
- **scaleMode** — `Walk::paint_kind` (`match paint.scale_mode`, ~1069): unknown
  → `Unsupported { what: "an image scaleMode STRETCH" }`.
- **strokeAlign** — `Walk::stroke_of` (`match node.stroke_align`, ~1161):
  unknown → `Unsupported { what: "a {other} stroke alignment" }`.

The `== "IMAGE"` / `== PaintTag::Image` comparison at mod.rs:380 becomes a string
compare. The `Unsupported` propagates through `fill_of`/`paint_of`/`stroke_of` →
`visit`, where the existing arm converts it to a blocker
(`Err(CompileError::Unsupported { what, .. }) => { blockers.push(what); None }`)
→ `unsupported_at`, which under `EmitPolicy::Partial` mints a `figma.unsupported`
at `Severity::Warning` (a skip that no longer blocks). No new plumbing — reuse
the existing Unsupported→blocker→unsupported_at path.

### 3. STRETCH and PATTERN → diagnose, never model

`dashpaint::ScaleMode` is `Fill/Fit/Crop/Tile` only; `dashpaint::PaintKind` is
`Solid/Gradient/Image` only. STRETCH (non-uniform scale-to-fill) and PATTERN (a
repeating source-node tile) each need a new `dashpaint` variant + a `dashbuf`
schema field + painter support — a boundary-B + schema story, not parse
robustness, and past "trivial." Diagnose both (skip-with-warning), consistent
with the epic's minimal-safe default and "never approximate."

## Guardrails

- **R7 / ABI unchanged:** the change is parse-side only. Emit deals in
  `dashpaint`/`dashbuf` types an `Unknown` value never reaches (the node is
  skipped). No status/wire-format/schema change; every known-variant input parses
  to the same value and emits byte-identically. Incidental win:
  `image_refs_response` also calls `parse_file`, so the importer's pre-fetch ref
  scan stops crashing on STRETCH too.
- **Do NOT touch** triage.rs (reads only String fields), emit.rs, the wasm ABI,
  the `.dsb` schema, or the E7 render oracle (TypeScript, Deno-side; v0.9 exit
  gate — untouched).
- **P4:** every unknown enum is a named diagnostic carrying its value. **P5:** the
  producer owns the diagnostic. **P1:** unaffected.
- **Fixtures:** `v03-paint.json` (FIT/IMAGE/SOLID/INSIDE/OUTSIDE/CENTER),
  `effects-2025.json` (String-keyed) carry only known values → unchanged. No test
  pins the crash on an unknown variant, so none needs removal.

## In-flight note

S1 (#310, merged) edited `text_of`; S2 edits `paint_kind`/`stroke_of` — different
regions of mod.rs, no conflict (S1 is already on the base).

## Predicted post-S2 frontier

S2 removes the _entire_ unknown-enum hard-crash class (3 enums). The remaining
hard (non-partial-able) aborts are: malformed JSON / depth overflow
(`CompileError::Parse` — genuine bad input), no-root (`Unsupported`), and
**`UnresolvedImage`** (an image fill whose ref the importer failed to fetch —
aborts even under Partial; the most likely remaining hard blocker for a
media-rich hero, a caller-contract issue). Every remaining _vocabulary_ gap is
already a warning under Partial. So after S2 the hero should **emit** under
Partial _provided its images all fetch_; the post-S2 re-probe confirms this and
reveals the (previously masked) per-node frontier. If `UnresolvedImage` bites,
that is the next small story (skip an unfetched-image node with a warning).

## Test strategy (TDD — detail in the plan)

- A synthetic file with `scaleMode: "STRETCH"` on an image paint: under Strict →
  refuses with a `figma.unsupported` naming "STRETCH"; under Partial → emits with
  the node skipped + a warning naming "STRETCH". (Was a hard parse crash.)
- Same for `type: "PATTERN"` paint and an unknown `strokeAlign`.
- A known-variant file (mirror v03-paint) still lowers to the same
  `dashpaint` values (no regression).
- The parse itself no longer errors on the unknown variant (the value reaches the
  walk as a string).
