# Figma fixture corpus

Self-authored Figma fixtures for the dashscene importer. Every fixture
here is authored in the project's own Figma account — nothing captured
from a third-party file ever enters this directory
(`docs/decisions/figma-corpus-self-authored-only.md`).

`manifest.json` is the source of truth: it maps each fixture name to its
Figma file key and describes what it covers. This file is the narrative
companion — read it alongside `manifest.json`, not instead of it.

Manifest fields beyond `name`/`fileKey` (the only two the capture tool
parses) are documentary, and two of them track compile status against the
as-built compiler:

- `emits` — whether the **raw capture** compiles to a `.dsb` today. It is
  reconciled when the compiler widens, so a `true` here is a live claim,
  not a design intent.
- `derivedEmits` — present when the raw capture is blocked by exactly one
  out-of-scope construct and a **declared derivation** — a single minimal
  edit, stated in the named test file, either a node-kind swap (a `TEXT` or
  `ELLIPSE` leaf retyped to a `FRAME`) or a property-value swap (a refused
  attribute value replaced by a lowering one, e.g. `counterAxisAlignItems`
  `BASELINE` → `MIN`) — emits and is pinned by a committed golden. The raw
  blocker and the story that lifts it are in `note`.

## Tier 1 — committed static corpus

Small, focused files rather than one mega-file: a failure should
implicate one construct, not "the fixture". Each is authored with the
`fixture-author` development plugin
(`importers/figma/plugin/fixture-author/`) — one menu command per
fixture, so a fixture is regenerable rather than hand-built — and
captured as its `GET /file` JSON (`?plugin_data=shared`) via `deno task
capture` in `importers/figma/`.

A capture is the raw response minus its non-deterministic fields: the
top-level `thumbnailUrl` is a presigned URL that Figma regenerates on
every fetch, so the capture tool strips it before writing (issue #141).
Beside each capture sits `<name>.receipt.json` — the captured `version`,
the `refsContract` it was derived under, and the image refs that capture
resolved — which is what the capture tool's unchanged-fixture pre-check
reads instead of parsing the whole capture (issue #91). The receipt
caches `dashc`'s ref answer, keyed on the `REFS_CONTRACT` constant in
`importers/figma/src/capture.ts`: when the lowering widens what it can
name, that constant is bumped, every committed receipt stops matching,
and the next capture run re-derives them from the committed captures
without any `GET /file` spend.

After a full capture of a fixture, its `<name>.images/` directory is
pruned to exactly the image refs that capture resolved, so a re-authored
image fill does not leave its old asset committed (issue #156). A
skipped, unchanged, or failed fixture never has its images pruned — the
directory is only authoritative when the fixture was actually captured.

Nine tier-1 fixtures are authored and captured, committed under
this directory. A tenth, `real-file`, is registered in the manifest for
the v0.7 real-file import spike (story #37) and awaits its manual
authoring step — it is production-shaped (two pages, undeclared frames
beside the export root, a component set with an instance, a hidden
layer, an image fill) rather than single-construct, so the export
closure replays against a realistic response shape.

| fixture                     | covers                                                                                                                                                                                                                                                                                |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `v03-paint`                 | v0.3 paint vocabulary under fixed layout: solid fill, all four gradient kinds, an image fill with a scale mode, the three stroke aligns, uniform and per-corner corner radii, and a clipsContent frame with an overflowing child. Its image fill's bytes live in `v03-paint.images/`. |
| `grid-basic`                | GRID mode: row/column spans, fixed/flex/hug tracks, hug/fill/fixed children. Min/max constraints are expressed as child `minWidth`/`maxWidth`, not a track-level bound — `GridTrackSize` has no such field (`docs/technotes/figma-plugin-api-findings.md`).                           |
| `variables-bound`           | `boundVariables` on color and number props, bound across light/dark modes. Also the designated input for the token-resolution phase 1→2 work (`docs/decisions/token-resolution-phase-split.md`).                                                                                      |
| `effects-2025`              | REJECT-list diagnostics fixture (see "Diagnostic fixture" below): noise, texture, and a progressive blur are present; variable-width stroke is still pending (no Plugin API — see manual step below).                                                                                 |
| `lowering-wrap`             | WRAP auto-layout: fixed-width children wrapping at a fixed container width.                                                                                                                                                                                                           |
| `lowering-hug-in-fill`      | a HUG child inside a FILL container (Figma≠CSS sizing lowering).                                                                                                                                                                                                                      |
| `lowering-negative-gap`     | negative `itemSpacing` overlap row, lowered to margins.                                                                                                                                                                                                                               |
| `lowering-baseline`         | mixed-size BASELINE alignment row, plus an Arabic RTL run with Arabic-Indic numerals.                                                                                                                                                                                                 |
| `lowering-variant-topology` | a component set whose variants have different child counts (topology change), plus one instance.                                                                                                                                                                                      |

### Diagnostic fixture: `effects-2025`

`effects-2025` is a **diagnostic** fixture, not a rendering fixture:
under R6 it must never emit a `.dsb`. Its root frame carries
`layoutMode: HORIZONTAL`, which the v0.3 walk refused before reaching
the REJECT-list triage the fixture was authored to exercise — its
acceptance test stripped the `layoutMode` key to reach the three
effects underneath. Since story #140 lowers auto-layout, the raw
capture reaches its effects with no derivation, and the tests use it
as captured.

### Manual authoring step still open

`effects-2025`'s variable-width stroke has no Plugin API at all —
every node in the captured file is `strokeType: "BASIC"`. It must be
drawn by hand (a line with a variable-width profile via the Draw
tools) and re-captured; see
`importers/figma/plugin/fixture-author/README.md` for the step-by-step.

## Tier 2 — live-only validation, never captured or committed

Three public targets, run live against the importer with the
diagnostic report reviewed by hand — no JSON is stored, and none of
this tier is committed here. Not yet wired into any CI job.

- **Grid Playground** (official Figma Community account) — grid mode
  against Figma's real emitted shapes (spans, hug, fractional tracks),
  not the project's approximation of them.
- **Config 2025 feature-update playground** — the same, for the 2025
  Draw effects (progressive blur, noise/texture, pattern fills).
- **One design-system kit, still to be picked** (candidates: Material
  3, Polaris, Untitled UI) — `boundVariables` + nested auto-layout +
  variant sets at realistic production scale. None of the candidates
  shows adoption of grid or the 2025 effects yet, so this target buys
  scale, not construct coverage.

## Licensing

Nothing in either tier is a third-party Figma file's captured JSON —
see `docs/decisions/figma-corpus-self-authored-only.md` for the ruling
and why the corpus is shaped this way.
