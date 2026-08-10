# Technote — importing a real public Figma file, end to end

    status   informative (technote) — as-built 2026-07-19
    scope    the "full real-file import" epic: dashc figma front end,
             the Deno importer, dashscene-typeset/-engine, goldens render tooling
    see      docs/decisions/unsupported-figma-constructs-refuse-the-compile.md
             (partial-emit), docs/decisions/figma-component-lowering.md (closure),
             docs/decisions/figma-text-lowering.md, docs/archive/2026-07-18-epic-*

This note records how the project took two real, public Figma Community files
(duplicated into drafts) all the way through `dashc` to a rendered `.dsb`, and
what remains. It is explanatory; the normative decisions live in the linked
records.

## Outcome

Two live targets now go end to end:

- **First-light** — the Auto Layout Playground section
  (`MRk9I5cYY6yJa8JhljzkBn`, root `2411:10795`): emits a ~5.3 KB `.dsb` and
  renders through Skia; only a `VECTOR` shape node and a corner-smoothing degrade
  remain, both skip-with-warning.
- **Hero** — the Landify landing page (`S30AJmYfnDKGeSQmzuXEUk`, root
  `1973:6580`): emits a ~185 KB `.dsb` and renders as a recognizable landing
  page — phone mockups decoded from embedded raster images, native fills and
  section bands, text wrapping within its boxes. Its remaining gaps are all named
  skip-with-warnings.

Both files are used live only, never committed
(`docs/decisions/figma-corpus-self-authored-only.md`).

## The decision that made it reachable

The hero began as "refuses every byte": one construct the document could not
express refused the whole file. The epic's first gate changed the emit policy to
**partial-emit — skip-and-diagnose, never approximate** (recorded in
`docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`): an
omission-class vocabulary gap becomes a named warning and its node is skipped, so
the document still emits with its covered majority; a construct that could only
ship approximately (a REJECT-band feature on a lowered node) still refuses. This
turned an unbounded all-or-nothing tail into a monotonic curve and is the single
change most responsible for the hero emitting.

## How it was run

An empirical loop rather than a guess. The `just reprobe <key> <root>` harness
rebuilds wasm, imports a live file, and prints the sorted unique blocker
diagnostics; each distinct blocker became a story, and the harness re-derived the
frontier after every merge. The stories, in the order they landed:

- **partial-emit mode** (`EmitPolicy`; importer defaults to Partial, `--strict`
  opts out) — PR #324.
- **reprobe harness** — PR #319.
- **component-closure auto-pull** — pull a declared root's buried local masters
  into the closure, and downgrade any unplaceable master (local or remote) to a
  warning so a baked instance renders from Figma's own baked children; this let
  the hero reach `dashc` — PR #325.
- **text vocabulary** — pixel line height, letter spacing, and horizontal/vertical
  alignment lower into `TextStyle` — PR #328.
- **parse robustness** — the three serde-strict Figma enums become tolerant
  `String`s with named catch-alls, so an unknown value (`scaleMode STRETCH`, a
  `PATTERN` paint) is a skip-with-warning, not a hard parse crash; this was the
  hero's last hard emit-blocker — PR #330.
- **render** — `render_dsb` + the `just render` recipe load an emitted `.dsb` and
  render it through the v0 Skia painter to a PNG — PR #331.
- **text render-wiring** — the engine measure seam and the render stager honor the
  lowered text axes, so imported text wraps in its box and honors its line height
  and alignment — PR #334.

The v0.9 E7 render-oracle exit gate was never touched (byte-identical, 15/15
throughout).

## What remains

- **Committed quantitative fidelity** (issue #332). The literal exit clause — the
  hero rendered inside the oracle's band vs Figma `GET /images` — cannot use the
  third-party targets, because the self-authored-corpus decision forbids
  committing their JSON or their `GET /images` render. The tractable, license-clean
  path is to measure two _self-authored_ fixtures (an image-fill fixture and a
  non-default-text-axes fixture) in a separate oracle manifest that never touches
  the E7 gate. This needs the fixtures authored in Figma. The actual targets'
  fidelity is checked live (`just render`, reviewed out of band).
- **The long tail** — all skip-with-warning, so emit-safe; fidelity only: `VECTOR`
  and other shape geometry, stacked fills, OpenType features, mixed-style text
  segments. Each is a candidate fidelity story, prioritized by what the oracle
  measures as the largest gap once #332 lands.
- **Debt** raised during the epic: #321, #326, #329, #333 (all minor).
