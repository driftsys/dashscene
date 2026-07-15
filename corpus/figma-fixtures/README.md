# Figma fixture corpus

Self-authored Figma fixtures for the dashscene importer. Every fixture
here is authored in the project's own Figma account — nothing captured
from a third-party file ever enters this directory
(`docs/decisions/figma-corpus-self-authored-only.md`).

`manifest.json` is the source of truth: it maps each fixture name to its
Figma file key and describes what it covers. This file is the narrative
companion — read it alongside `manifest.json`, not instead of it.

## Tier 1 — committed static corpus

Small, focused files rather than one mega-file: a failure should
implicate one construct, not "the fixture". Each is authored with the
`fixture-author` development plugin
(`importers/figma/plugin/fixture-author/`) — one menu command per
fixture, so a fixture is regenerable rather than hand-built — and
captured as its `GET /file` JSON (`?plugin_data=shared`) via `deno task
capture` in `importers/figma/`.

All nine tier-1 fixtures are authored and captured, committed under
this directory:

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
`layoutMode: HORIZONTAL`, so as built, the compile actually stops on
auto-layout refusal
(`docs/decisions/figma-auto-layout-refused-on-two-grounds.md`) before
reaching the REJECT-list triage the fixture was authored to exercise —
its acceptance test strips the `layoutMode` key to reach the three
effects underneath. A future diagnostic fixture must set `layoutMode:
NONE`, or the auto-layout refusal masks the constructs it exists to
test.

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
