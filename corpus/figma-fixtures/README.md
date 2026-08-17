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
Beside each capture sits `<name>.receipt.json` — the captured `version`
and `lastTouchedAt`, the `refsContract` it was derived under, and the
image refs that capture resolved — which is what the capture tool's
unchanged-fixture pre-check reads instead of parsing the whole capture
(issue #91). The receipt caches `dashc`'s ref answer, keyed on the
`REFS_CONTRACT` constant in `importers/figma/src/capture.ts`: when the
lowering widens what it can name, that constant is bumped, every
committed receipt stops matching, and the next capture run re-derives
them from the committed captures without any `GET /file` spend.

**The skip needs `lastTouchedAt` as well as `version`, and the committed
receipts do not carry it yet** (issue #965). A `version` that matched
across a real change is what that issue records, so a version-only skip
reported a fixture current while it was two frames behind, permanently.
Every receipt here predates the field, so **the next capture run
re-fetches each fixture once** — that is the migration, and it is a
one-off. Deleting a receipt does not shortcut it and makes it worse: the
observed pair goes with it and cannot be re-derived from a capture, which
carries a `version` field and no timestamp. Bump `REFS_CONTRACT` when the
refs need re-deriving; that invalidates the refs and only the refs.

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
`docs/technotes/figma-rest-shapes.md`, including a units
error in Figma's own published REST spec.

Both are written through the Plugin API's `setReactionsAsync`, so each is one
menu command rather than a hand-authoring job. **Two reaction writes were
refused and are still outstanding**: `CUSTOM_SPRING` in the first, and
`MOUSE_ENTER` in the second. Each capture carries a `_manual-checklist` node
naming its own.

`SCROLL_ANIMATE` was a third until the second capture was re-run, which landed
it and added a `refused-mouse-down` cell. That re-capture is what this branch
carries, and it moved every count below — the first capture's revisions have
still never been run, so re-running that command produces a file that no longer
matches its committed capture.

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

Since story #773 that is true of the as-built compiler and not only of the
intent: under `EmitPolicy::Strict` its twelve interaction-carrying nodes earn
**21** error-severity `figma.prototype.unsupported-interaction` findings and R6
withholds the bytes, so `manifest.json` now records `emits: false`. Twenty-one
rather than twelve because every finding survives one pass (debt #149): nine of
those nodes carry two independent gaps, a refused navigation beside a refused
transition or trigger. Under `EmitPolicy::Partial` —
the importer's default — they downgrade to warnings and the file emits
unchanged, because an interaction is not paint and no node is skipped over
one. The split it was built to protect held: `prototype-smart-animate` still
emits, which is why a refused _easing_ is a warning and a refused _navigation_
is an error.

It carries one node per refused construct, named for what it holds, so a
diagnostic bisects to a name. What the committed capture actually carries is
transitions `DISSOLVE`, `PUSH` (the `DirectionalTransition` arm, which adds
`direction` and `matchLayers`) and `SCROLL_ANIMATE`; easings
`CUSTOM_CUBIC_BEZIER` and `EASE_OUT_BACK`, neither of which any of `dashcue`'s
four fixed cubics expresses; triggers `AFTER_TIMEOUT`, `MOUSE_DOWN` and
`ON_KEY_DOWN`; navigations `SCROLL_TO` and `OVERLAY`; and actions `URL`,
`SET_VARIABLE` and `CONDITIONAL`, whose `conditionalBlocks` nest `Action[]`
recursively. Those are the thirteen names
`the_refused_capture_withholds_the_bytes_and_names_every_construct` asserts on
— thirteen names across twelve nodes, because `refused-scroll-animate` is the
one cell contributing two, its transition and its navigation.

`SCROLL_TO` and `OVERLAY` are listed as navigations and not as actions because
that is what the diagnostic calls them: both arrive as the `navigation` of a
`NODE` action, so a test grepping the report for them under the word "action"
is reading the wrong field.

A `SCROLL_TO` needs somewhere to scroll to, so the re-capture also added
`scroll-anchor` (`1:112`): a plain `FRAME` carrying no interaction, named
outside the `refused-*` convention because it is a destination rather than a
refused construct. It lowers into the emitted document like any other frame,
and it is half of what moved the `EmitPolicy::Partial` emission from 1768 to
1832 bytes — the `refused-mouse-down` cell is the other half, and between them
they grew the root frame from 256 to 384 px.

One node the command builds to carry an interaction carries **none**:
`refused-mouse-enter`, refused by `setReactionsAsync`. A test asserting a
diagnostic on that name will find nothing there. The capture's
`_manual-checklist` node names it.

`refused-scroll-animate` was the other until the re-capture landed its write.
It now carries `SCROLL_ANIMATE` + `SCROLL_TO` like every other cell, which is
why it is no longer usable as the no-interaction example a test picks.

The last cell is the one worth reading twice. Its reaction maps perfectly —
`ON_CLICK`, `CHANGE_TO`, `SMART_ANIMATE` — but its two variants differ in
**fill**, and a variant transition animates rect channels only. Smart Animate
interpolates colour happily, so this is the case every real Figma file hits.

**The lowering made that call on 2026-08-11 (story #773): a warning, and the
producer makes it rather than the load gate.** `dashc` never emits a fill
track, so `dashscene_validator`'s `TRANSITION_CHANNEL_NOT_A_RECT` is never
reached from this path. The fill difference itself still lowers, as a
`VariantFill` override, so the switch carries the colour and changes it in one
frame; only the animation of it is dropped, under
`figma.prototype.unsupported-motion`. A warning rather than an error because
the picture is right — the reasoning is in
`docs/decisions/figma-component-lowering.md` ("Amendment, 2026-08-11").

This cell carries no `INSTANCE`, which turned out to matter: a variant table
is emitted per instance, so a lowering that only diffed members when an
instance asked for one would leave this construct unexercised. The member diff
is computed at the `COMPONENT_SET` for that reason.

**How many constructs this file actually exercises: thirteen, not sixteen.**
Of its sixteen `refused-*` nodes, four carry no interaction:
`refused-destination` and `refused-overlay-target` are navigation targets,
`refused-mouse-enter`'s write is still refused (above), and `refused-fill-diff`
is a `COMPONENT_SET` holding a variant diff rather than a reaction. The
remaining twelve interaction cells plus that fill diff are what a test can
assert on. Counted by construct _name_ the figure is thirteen as well, for the
unrelated reason given further up — `refused-scroll-animate` contributes two
names — so do not try to reconcile the two thirteens into one.

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

## Publishing the file keys

`manifest.json` carries a Figma **file key** for every fixture, and those keys
publish with this repository. A key is the identifier in a Figma URL and it
cannot be rotated: once published it is public permanently.

**The ruling is
[`docs/decisions/figma-file-keys-are-published.md`](../../docs/decisions/figma-file-keys-are-published.md).**
Nothing is restated here, so there is no second copy to go stale when it is
revised. `just figma-sharing` checks the live setting against the ruling.

One caveat that belongs beside the fixtures rather than in the ruling: the
`linkAccess` value inside a captured `.json` is a snapshot from capture time,
not the live setting. Check Figma, not the fixture, when the answer matters.
