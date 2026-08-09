# Figma fixture corpus

Self-authored Figma fixtures for the dashscene importer. Every fixture
here is authored in the project's own Figma account — nothing captured
from a third-party file ever enters this directory
(`docs/decisions/figma-corpus-self-authored-only.md`). A CC0 raster
payload may sit inside such a fixture, under the conditions in
"Licensing" below.

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
- `vartable` — present when a fixture has a committed token-export table
  (`<name>.vartable.json`), the annotator plugin's id → name/collection/mode
  output that #167 joins the phase-1 sidecar against
  (`docs/decisions/token-resolution-phase-split.md`). Only `variables-bound`
  carries one today.

## Tier 1 — committed static corpus

Small, focused files rather than one mega-file: a failure should
implicate one construct, not "the fixture". Each is authored with the
`fixture-author` development plugin
(`importers/figma/plugins/fixture-author/`) — one menu command per
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

Every fixture the manifest registers is authored and captured: 32 entries,
32 committed captures, none holding a placeholder file key. (This paragraph
said "nine ... four more await a manual authoring step" until story #773; it
had been stale for several slices, and two of the four it named as pending —
`real-file` and `trim-demo` — had committed captures at the time.)

Four of them still needed a manual step at authoring time, and the notes
matter if any is ever re-authored:

- `real-file` — the v0.7 real-file import spike (story #37):
  production-shaped (two pages, undeclared frames beside the export
  root, a component set with an instance, a hidden layer, an image fill)
  rather than single-construct, so the export closure replays against a
  realistic response shape.
- `trim-demo` — the trim-path exercise (story #39): one declared root
  holding a placeholder slot, a redline overlay, a spec note, a
  `_`-prefixed scratch layer, and a hidden layer. Its authoring is **two
  steps** (the real-file precedent): run the fixture-author `trim-demo`
  command to build the scene, then run the **separate** dashscene
  annotator plugin to write the roles (the fixture author never writes
  roles), then capture. The trim rules themselves are covered offline by
  `importers/figma/src/trim_test.ts`; this fixture replays annotate →
  trim → named record against a real `?plugin_data=shared` response.
- `xfile-library` / `xfile-consumer` — the cross-file library-resolution
  pair (story #38, `docs/decisions/figma-cross-file-library-resolution.md`).
  A **two-file** manual step, because no Plugin API publishes a team
  library or instances a component across files: author `xfile-library`
  with a component set and **publish** it as a team library, then author
  `xfile-consumer` instancing a variant of that published set, paste both
  file keys into the manifest, and capture. The consumer's `components`
  map then carries the instanced component as `remote: true` with the
  library key — the shape resolution matches on. The mechanism is covered
  offline by `importers/figma/src/closure_test.ts` and `import_test.ts`;
  this pair replays remote-instance resolution against real
  `?plugin_data=shared` responses.

Story #773 added the last two, `prototype-smart-animate` and
`prototype-refused` — the mapping and diagnostic halves of Figma's prototype
vocabulary. They are the first fixtures here to carry a prototype interaction
at all: every capture committed before them reports
`prototypeStartNodeID: null` and an empty `interactions` array on every node,
which is why nothing in this repository pinned the shape. What the captures
then showed is recorded in
`docs/technotes/figma-rest-shapes-the-capture-pinned.md`, including a units
error in Figma's own published REST spec.

Both are written through the Plugin API's `setReactionsAsync`, so each is one
menu command rather than a hand-authoring job. **Three reaction writes were
refused and are still outstanding**, so neither capture is complete:
`CUSTOM_SPRING` in the first, and `SCROLL_ANIMATE` and `MOUSE_ENTER` in the
second. Each capture carries a `_manual-checklist` node naming its own. The
payloads have since been revised, but the revisions have never been run — so
re-running either command produces a file that no longer matches its
committed capture and must be re-captured.

| fixture                     | covers                                                                                                                                                                                                                                                                                |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `v03-paint`                 | v0.3 paint vocabulary under fixed layout: solid fill, all four gradient kinds, an image fill with a scale mode, the three stroke aligns, uniform and per-corner corner radii, and a clipsContent frame with an overflowing child. Its image fill's bytes live in `v03-paint.images/`. |
| `grid-basic`                | GRID mode: row/column spans, fixed/flex/hug tracks, hug/fill/fixed children. Min/max constraints are expressed as child `minWidth`/`maxWidth`, not a track-level bound — `GridTrackSize` has no such field (`docs/technotes/figma-plugin-api-findings.md`).                           |
| `variables-bound`           | `boundVariables` on color and number props, bound across light/dark modes. Also the designated input for the token-resolution phase 1→2 work (`docs/decisions/token-resolution-phase-split.md`).                                                                                      |
| `effects-2025`              | REJECT-list diagnostics fixture (see "Diagnostic fixture" below): noise, texture, and a progressive blur are present; variable-width stroke is still pending (no Plugin API — see manual step below).                                                                                 |
| `lowering-wrap`             | WRAP auto-layout: fixed-width children wrapping at a fixed container width.                                                                                                                                                                                                           |
| `lowering-hug-in-fill`      | a HUG child inside a FILL container (Figma≠CSS sizing lowering).                                                                                                                                                                                                                      |
| `lowering-negative-gap`     | negative `itemSpacing` overlap row of full ellipses, lowered to margins. Also the designated input for the `ELLIPSE`→circle shape lowering (`docs/decisions/figma-ellipse-as-circle.md`, #239).                                                                                       |
| `lowering-baseline`         | mixed-size BASELINE alignment row, plus an Arabic RTL run with Arabic-Indic numerals.                                                                                                                                                                                                 |
| `lowering-variant-topology` | a component set whose variants have different child counts (topology change), plus one instance. Also the designated input for local component/instance lowering (`docs/decisions/figma-component-lowering.md`, #242).                                                                |
| `prototype-smart-animate`   | the prototype vocabulary that maps onto `dashcue` (#773): `ON_CLICK` → `NODE`/`CHANGE_TO` → `SMART_ANIMATE` over a two-variant set differing in rect props only, one instance per mappable easing arm, and a non-null `prototypeStartNodeID`.                                         |
| `prototype-refused`         | the prototype vocabulary that cannot reach `dashcue` (#773), one construct per node — see "Second diagnostic fixture" below.                                                                                                                                                          |

### Diagnostic fixture: `effects-2025`

`effects-2025` is a **diagnostic** fixture, not a rendering fixture:
under R6 it must never emit a `.dsb`. Its root frame carries
`layoutMode: HORIZONTAL`, which the v0.3 walk refused before reaching
the REJECT-list triage the fixture was authored to exercise — its
acceptance test stripped the `layoutMode` key to reach the three
effects underneath. Since story #140 lowers auto-layout, the raw
capture reaches its effects with no derivation, and the tests use it
as captured.

### Second diagnostic fixture: `prototype-refused`

`prototype-refused` is the second **diagnostic** fixture, and it exists for
the same R6 reason `effects-2025` does: a fixture carrying an error emits no
`.dsb`, so the prototype vocabulary that maps onto `dashcue` and the
vocabulary refused by name cannot share a file without the mapping case
losing its emission test.

It carries one node per refused construct, named for what it holds, so a
diagnostic bisects to a name. What the committed capture actually carries is
transitions `DISSOLVE` and `PUSH` (the `DirectionalTransition` arm, which adds
`direction` and `matchLayers`); easings `CUSTOM_CUBIC_BEZIER` and
`EASE_OUT_BACK`, neither of which any of `dashcue`'s four fixed cubics
expresses; triggers `AFTER_TIMEOUT` and `ON_KEY_DOWN`; and actions `URL`,
`SET_VARIABLE`, `OVERLAY` and `CONDITIONAL`, whose `conditionalBlocks` nest
`Action[]` recursively.

Two nodes the command builds carry **no** interaction in this capture:
`refused-scroll-animate` and `refused-mouse-enter`, both refused by
`setReactionsAsync`. A test asserting a diagnostic on either name will find
nothing there. The capture's `_manual-checklist` node names both.

The last cell is the one worth reading twice. Its reaction maps perfectly —
`ON_CLICK`, `CHANGE_TO`, `SMART_ANIMATE` — but its two variants differ in
**fill**, so the tracks that diff fans out to are `FillR`/`FillG`/`FillB`,
which `dashscene_validator`'s `TRANSITION_CHANNEL_NOT_A_RECT` rule refuses: a
variant transition animates rect channels only. Smart Animate interpolates
colour happily, so this is the case every real Figma file hits. Whether it is
an error or a warning that drops the colour tracks is the lowering story's
call; the fixture only has to carry the case so the call is made against
captured data.

### Manual authoring step still open

`effects-2025`'s variable-width stroke has no Plugin API at all —
every node in the captured file is `strokeType: "BASIC"`. It must be
drawn by hand (a line with a variable-width profile via the Draw
tools) and re-captured; see
`importers/figma/plugins/fixture-author/README.md` for the step-by-step.

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

### Third-party raster payloads

The rule scopes to the Figma document, not to every byte inside it
(ruled 2026-07-28, raised by issue #455). A **CC0** image may sit in a
self-authored fixture's image fill, because the Figma licence the ruling
routes around does not reach a raster payload that carries its own.

CC0 only — not CC-BY, and not the Unsplash or Pexels licences, which are
bespoke instruments rather than CC0. Wikimedia Commons and Poly Haven
publish genuine CC0.

**Every such payload is listed in the table below before it is
committed.** CC0 obliges no attribution, so this table is not a licence
condition; it is this repository's audit trail, and an unlisted
third-party payload is a defect regardless of how it is licensed.

| payload | fixture | source URL | licence as stated at source | retrieved | what it is |
| ------- | ------- | ---------- | --------------------------- | --------- | ---------- |

The table is empty: no CC0 payload sits inside a Figma fixture yet. The
provenance row lives beside the payload, so a payload in another `corpus/`
subdirectory is listed in that directory's own README — `corpus/photo/`
holds the first four (issue #455). Adding a row is part of the change that
adds the asset, never a follow-up.

Three things CC0 does not cover, confirmed per asset before it is
committed: trademark, rights held by third parties depicted _in_ the
work (a recognisable person needs a release; artworks and some
buildings carry separate rights), and whether the uploader had the right
to apply CC0 at all.
