# Radial placement is an absolute box plus a transform, never a layout mode

    status   accepted (2026-07-17); resolves
             docs/technotes/producers-and-ir.md §6's open item
    scope    the dashscene layout vocabulary, the animation and binding
             vocabulary (dashcue per-prop smoothing + the binding table),
             and the validator profile

## Context

Circular gauges, arced menus, needle pivots, and telltales at regulator-mandated
positions recur in automotive clusters. None of the box models the project
builds on — CSS flex and grid, Figma auto-layout — has a radial placement mode.
`docs/technotes/producers-and-ir.md` §6 left the question open on purpose: is
radial or anchored placement ever a first-class dashscene layout mode, or
forever an absolute box plus a transform whose angle a producer computes?

## Options

1. **A first-class radial / anchored layout mode** — a new `LayoutMode` beside
   flex and grid that the solver resolves into angular positions.
2. **Absolute box plus transform** — placement stays absolute (or normal flex);
   the radial part is a transform or paint computed from data; the gauge
   vocabulary is bound-prop animation data; everything else radial is
   producer-side repeater math at authoring time.

## Choice

Option 2. Radial is not a layout mode, and will not become one. Three parts.

1. **Placement stays an absolute box plus a transform.** Gauges, arced menus,
   and needle pivots place the box absolutely (or by normal flex rules), and the
   radial part is a transform or paint computed from data, not a solved
   position. Safety-regulated fixed regions (telltales at mandated positions)
   are absolute boxes; a `fixed-region` validator attribute asserts that the
   resolved rect equals the authored rect. That is a check, not a layout mode.
   Absolute placement is already authored intent, not a result
   ([fixed-position-authoring.md](fixed-position-authoring.md), P1).

2. **The gauge vocabulary is first-class bound-prop data** in the animation
   vocabulary's per-prop smoothing row. A bound scalar in the range 0 to 1
   declaratively drives one of: a rotation about a declared pivot (start and end
   angle, with optional discrete detents); an arc sweep on an arc-capable shape
   (start and end, inner radius, cap, corner radius); a size or position lerp
   between two authored endpoints; or progress along a path. The scalar is an
   ordinary binding — data into one prop on one node
   ([bindings-are-explicit-and-flat.md](bindings-are-explicit-and-flat.md)) — so
   the runtime owns time and nothing producer-side runs in the frame loop (P3),
   and the animation is reproducible in tests (R4). Schema note: a sweep of 1.0
   is defined as a closed ring, so the full-circle case is never degenerate.

3. **Everything else radial is producer-side, at authoring time.** Tick marks,
   arced label rings, and curved menus are repeater math that emits absolute
   boxes at compile time. There is no runtime radial solver and no new IR
   concept.

## Why

- A radial layout mode would put an angular solver on the frame-critical path
  and add an IR concept that only the gauge node kind needs, when the same
  result is a bound scalar over an absolute box. Shipped automotive HMI practice
  implements gauges as node-level parameters driven by a bound scalar over
  absolute placement, with layout uninvolved.
- Keeping the gauge parameters in the bound-prop vocabulary rather than the
  layout table preserves P2 (one solver, and it is Taffy) and P3 (descriptive
  animation, the runtime owns time), and keeps a gauge reproducible in tests the
  way every other bound prop is (R4).
- A `fixed-region` check, rather than a mandated-position layout mode, keeps
  safety-regulated placement inside the existing absolute-box model: the
  document still carries intent, and the validator proves the resolved rect
  matches it.

This decision binds two follow-ups, tracked in
`docs/technotes/producers-and-ir.md` §7: the gauge parameter set folds into the
animation spec when the per-prop smoothing vocabulary is written
([dashcue.md](../design/dashcue.md)'s deferred §6.3 rows), and the
`fixed-region` attribute is added to the validator spec. The arc-capable shape
the sweep parameter needs is the dedicated shape construct already deferred to
v1 ([figma-ellipse-as-circle.md](figma-ellipse-as-circle.md)).

Scheduling: this record is decision-only. The gauge and radial vocabulary is
post-v0 and rides on dashcue's per-prop smoothing row, which no v0 slice adds;
it is a candidate for v1's full feature set (the animation-vocabulary expansion,
[`../roadmap.md`](../roadmap.md), "v1"), not v2, which is remote/streaming.
Nothing here is built ahead of the plan.
