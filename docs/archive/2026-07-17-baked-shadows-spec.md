# Story #45 — drop and inner shadows (v0.8)

Working memory for the effects-vocabulary slice. Gardened into
`docs/decisions/effects-vocabulary-shadows.md` and the as-built design
records when the work lands; this file is archived, not deleted.

## Goal

Teach the pipeline drop and inner shadows as authored intent, rendered
live in the Skia painter. Compile-time baking and `profile:core`
enforcement stay v1 (so the content-addressed asset model #107 is not a
dependency). Folds debt #144 (`Dsb` had no effects vocabulary).

Success criteria:

- `just build`, `just verify`, `just wasm` green.
- A drop-shadow golden and an inner-shadow golden, each with a
  demonstrated sensitivity guard (a broken variant that exceeds the
  budget).
- The frozen r7 fixture regenerated with a shadow at non-default values.
- The dashc DROP_SHADOW/INNER_SHADOW refusal un-pinned; noise, texture,
  and progressive blur stay REJECT.
- Shadow params domain-checked at the load and paint gates, with tests.

## Design

A shadow is a per-node visual property with no cross-node relation — it
depends only on the node's own box, corners, and shadow params. That
makes it exactly the corners case, not the masks/opacity case: it needs
no commit-time resolution against siblings or ancestors, so it rides on
the deduplicated paint-pool entry and reaches the painter through the
existing `paints` table. No new `Painter::paint` parameter.

Data flow (mirrors corners end to end):

    Figma effects  ─dashc─►  Paint.shadows (dashbuf)
                                   │ load
                                   ▼
    Prop::Shadows ─►  NodeData.shadows ─commit─► PaintEntry.shadows (dashpaint)
                                                        │
                                                        ▼ Skia painter draws

### Schema shape — a shadow list vs fixed slots (alternatives)

Chosen: a `shadows: [Shadow]` list on the `Paint` pool entry. `Shadow`
carries `kind` (Drop/Inner), `offset`, `blur`, `spread`, `color`.

1. **A list (chosen).** Figma's `effects` is an ordered array, and a real
   design routinely stacks several drop shadows for layered elevation.
   A list carries them all in order.
2. **Fixed slots** — one drop-shadow field plus one inner-shadow field on
   `Paint`. Simpler, but it re-creates the #146 gap for effects: a node
   with two drop shadows would have to be refused, and the refusal band
   would grow rather than shrink. Rejected.

The list is also what keeps #146 out of this story: shadows never touch
`Paint.fill`/`.stroke` arity, so those stay single-valued and #146
remains open and unexercised (recommend re-anchoring it at the next
revision).

### Inner-shadow rendering technique (alternatives)

Chosen: clip to the node's shape, then fill the complement of the
(offset, spread-inset) inner rounded rect with the shadow color under a
Gaussian blur mask filter (an even-odd path: outer bounds minus the inner
rrect). The blur bleeds inward from the shape's edge, thicker on the
offset side — the inner-shadow look.

1. **Clip + inverse-fill + blur (chosen).** Pure geometry, deterministic
   for a pinned skia, reuses the same rounded-rect machinery as the drop
   shadow and the existing clip/stroke code. No offscreen surface.
2. **Offscreen image-filter** — render the shape's complement into a
   layer, blur it, composite source-in. Heavier (an RT round-trip that
   R-T1 discourages), and skia image-filter output is less bit-stable
   across versions than a plain path fill. Rejected.

### Shadow-spread math (seed §8.1)

Skia has no native spread, so spread is a geometry lowering. For a
rounded box with per-corner radius r:

- Drop: box outset by `spread` on all sides, offset by `(dx, dy)`, corner
  radius `r > 0 ? max(0, r + spread) : 0` (a sharp corner stays sharp).
- Inner: the lit hole is the box **inset** by `spread`, offset by
  `(dx, dy)`, corner `r > 0 ? max(0, r - spread) : 0`.

Blur radius → Skia sigma is `sigma = blur / 2` (the CSS/browser
convention); a zero-blur shadow uses no mask filter (a hard edge).

### Group-opacity and clip interplay (#44)

Each shadow's paint alpha is modulated by `RectEntry::opacity` (the
free-path group alpha), and every shadow draw sits inside the rect's
clip-region save/restore and inside the render-target `save_layer` that
the group-opacity walk opens. So: a shadowed node under folded opacity
dims with it; under a render-target group its shadows composite inside
the layer; a clipped node's drop shadow is clipped to its ancestor clip
region.

### dashc lowering

`shadows_of(node)` reads visible DROP_SHADOW/INNER_SHADOW effects into
`dashpaint::Shadow`s (color/offset/radius/spread). The triage un-pins the
DROP_SHADOW/INNER_SHADOW refusal — a lowered shadow is no diagnostic at
all. Noise, texture, and progressive blur stay REJECT. A shadow with a
non-NORMAL blend mode is an advanced-blend diagnostic (same posture as a
paint blend mode); a shadow with no color is a named refusal (like a
SOLID with no color). Hidden (`visible: false`) effects are skipped, like
hidden paints. `showShadowBehindNode` is not modeled (the REST subset is
deliberately partial; a documented limitation, not a silent drop of an
expressible field).

### Validator

`check_shadows` runs on both gates (load `Paint.shadows`, paint
`PaintEntry.shadows`), following the `check_corners`/`check_stroke_width`
pattern: offset x/y finite; blur finite and non-negative (a negative
Gaussian is meaningless); spread finite (any sign — CSS/Figma spread may
be negative); color channels finite and in `[0, 1]`. New rule ids under
`paint.shadow.*`.

## Alternatives considered — placement (why core, not a hand-built table)

The goldens could hand-build boundary-B tables directly, avoiding
dashscene-core. But then `Paint.shadows` (schema) and the dashc lowering
would feed nothing at runtime — the loader would not populate
`PaintEntry.shadows`. Carrying shadows through core's `Prop::Shadows`
makes the slice a complete vertical (`.dsb` → core → painter) and reuses
the corners plumbing verbatim. The goldens then author through core, like
the #44 mask/opacity goldens.

## Plan

1. Schema: `Shadow` table + `ShadowKind` enum, append `shadows` to
   `Paint`. → verify: `cargo build -p dashbuf`.
2. r7 fixture: add a Drop shadow to the fixture writer, a decode
   assertion, regenerate under `UPDATE_DSB_FIXTURE=1`. → verify:
   `cargo test -p dashbuf`.
3. dashpaint: `Shadow`, `ShadowKind`, `PaintEntry.shadows`. → verify:
   `cargo build -p dashpaint`.
4. core: `Prop::Shadows`, `NodeData.shadows`, set_prop + prop_class
   (Paint), commit emit, paint_key fold, load.rs read. → verify:
   `cargo test -p dashscene-core`.
5. skia: draw drop + inner shadows with spread math, under opacity/clip.
   → verify: painter unit tests + build.
6. dashc: `shadows_of`, emit `Paint.shadows`, un-pin triage, Effect REST
   fields. → verify: `cargo test -p dashc`.
7. validator: `check_shadows` on both gates + rule ids + tests. →
   verify: `cargo test -p dashscene-validator`.
8. goldens: drop + inner scenes, sensitivity guards, regenerate under
   `UPDATE_GOLDENS=1`. → verify: `cargo test -p goldens`.
9. Garden: decision record + as-built updates; un-pin the
   unsupported-constructs baked-shadow row. → verify: `docs/wip/` empty.
10. Gates: `just build`, `just verify`, `just wasm`; squash; draft PR.
