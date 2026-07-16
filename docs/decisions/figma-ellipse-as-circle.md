# A full-circle ELLIPSE lowers to a rounded rect; every other ellipse is refused

    status   accepted (story #239, 2026-07-16)
    scope    crates/dashc (the figma module)
    binds    #242 (components lower shape children), #38, and the v1 slice
             that first needs a true shape construct

## Context

Story #140 widened the walk to auto-layout but lowered only `FRAME` (and,
since #160, `TEXT`). Every other Figma node kind is a named
`figma.unsupported` diagnostic. The captured fixtures carry `ELLIPSE`
nodes — `lowering-negative-gap.json` holds five — so a real-file import
(#37) reaches them at once, and no story lowered them. This is that story.

The scope note gave two representations to weigh:

- **Option A** — a dedicated shape construct in the `.dsb` vocabulary. A
  dashbuf schema change, so the frozen fixture regenerates deliberately
  (`docs/decisions/dsb-frozen-fixture-r7-guard.md`), the wasm ABI and the
  Deno byte-identity re-pin, and `dashpaint` plus the reference painter gain
  a shape rasterization with its own paint-level test.
- **Option B** — lower a full ellipse to the rounded-rect vocabulary the
  paint entry already carries (`docs/decisions/paint-entry-composition.md`),
  corner radius = half the extent. No schema change; the painter is
  untouched.

## The geometric fact that decides it

`dashpaint::CornerRadii` carries **one scalar per corner** (`top_left`,
`top_right`, `bottom_right`, `bottom_left`), and the reference painter maps
each corner to `Point::new(r, r)` — a circular arc with equal x and y radii
(`crates/dashscene-skia/src/lib.rs`, `rrect_of`). There is no per-axis
`(rx, ry)` corner radius anywhere in the vocabulary or the painter.

An ellipse of extents `w × h` is a rounded rect whose straight edges vanish
and whose corners are quarter-ellipses of radii `(rx = w/2, ry = h/2)`.

- **`w == h` (a circle)** — `rx == ry == w/2`, so the single scalar
  `r = w/2` expresses it exactly. Skia's `RRect` with all four corners at
  `w/2` on a `w × w` box has zero straight edge and forms a true circle. A
  circle is an ellipse with equal axes, so this is exact, not an
  approximation.
- **`w != h` (a non-circular ellipse)** — the corners need `rx != ry`, which
  one scalar per corner cannot express. The closest a single radius reaches
  is a stadium (`r = min(w, h) / 2`) or a clamped rounded rect — never an
  ellipse. Painting that would render a picture the designer never authored,
  which is a silent-drop (P1, P4) failure.

So option B is exact only for circles. That is the decisive limitation, and
it is honest to state it: the rounded-rect vocabulary is geometrically
incapable of a non-circular ellipse, and no lowering can hide that. When v1
first needs true non-circular ellipses (or arcs and rings), option A — a
dedicated shape construct, or per-axis corner radii — is the additive schema
evolution that carries them; this record is edited in place at that point.

## Choice

Option B, restricted to what it can render exactly. A full-sweep `ELLIPSE`
with equal, fixed extents lowers to a leaf whose paint entry is the frame's
fill and stroke with all four corners set to half the extent. Every other
ellipse is a named `figma.unsupported` diagnostic (P4), collected in the
same one-pass walk as any other finding:

- **A non-circular ellipse** (`w != h`) — the vocabulary carries no per-axis
  corner radius, per the argument above.
- **A non-fixed-size ellipse** (a `HUG` or `FILL` axis) — the corner radius
  is a static paint parameter, but a solver-owned extent is unknown at
  lowering time (P1 forbids reading it), so the radius could not track the
  solved size. A `Fixed`/`Fixed` ellipse is the only one whose radius is
  authored.
- **An elliptical arc** (`arcData` sweep `!= 2π`) and **a ring**
  (`arcData.innerRadius != 0`) — a pie or donut has no rounded-rect lowering.
  Absent `arcData` is Figma's full-ellipse default.

The three geometry gates — extents equality, sweep, inner radius — share one
fractional tolerance (`ELLIPSE_GEOMETRY_TOLERANCE`, `1e-3`), not exact
`==`/`!= 0.0` comparisons. A real capture composes transforms up the tree and
reports decimal extents, so an authored circle arrives as `56.0 × 55.99998`, a
full sweep as `2π` minus a rounding bit, and a solid ellipse's `innerRadius` a
hair above zero — the exact real-file shape #37 targets. An exact gate would
refuse those genuine circles. `1e-3` (of the larger extent, of a full turn, of
the outer radius) sits far above that float noise and far below any ellipse the
painter would render visibly non-circular: it admits at most `0.056 px` of
extent difference on a `56 px` circle, while a `56 × 50` ellipse differs by
`11 %` — over a hundred times the tolerance — so it cannot admit a genuine
non-circular ellipse, arc, or ring.

The other shape kinds the fixtures carry (`LINE`, `VECTOR`, `STAR`,
`POLYGON`, `REGULAR_POLYGON`) keep the walk's existing per-kind
`node type X` diagnostic — each distinct, none dropped. Lowering them stays
out of scope.

## Why B over A for this slice

- v0.7 is importer catch-up, and its stated posture is that this slice
  "widens what crosses [the wasm ABI], not the contract" (`docs/roadmap.md`).
  Option B widens what crosses — a previously-refused fixture now compiles —
  with no change to the schema or the ABI. Option A changes the contract.
- The captured fixtures carry only full circles (five `56 × 56` ellipses,
  full sweep, `innerRadius 0`), which B lowers exactly. Building a shape
  construct now would be speculative vocabulary for a case no fixture and no
  v0/v1 requirement yet needs.
- B reuses the rounded-rect vocabulary that already round-trips through the
  format (`v03-paint.json`'s corner nodes) and renders through the painter,
  so nothing downstream of the walk changes.

## Consequences

- `lowering-negative-gap.json` compiles: its five circles lower, so the
  fixture emits (`corpus/figma-fixtures/manifest.json` moves it to
  `emits: true`). Its root still hug-collapses under Taffy 0.12 (engine debt
  #236, unchanged by this story); the render golden lifts the root's `HUG`
  width to `FIXED` so the collapse does not clip the circles — a declared
  derivation, the same mechanism the baseline and hug-in-fill goldens use
  (`goldens/dsb/README.md`).
- The pre-existing `v07-negative-gap-derived.dsb` golden (the ellipses
  retyped to frames) stays as-is: it is font-free and byte-compared by both
  the native and Deno suites for cross-language ABI parity, and a frame and a
  circle solve to the same box, so it remains a valid solve-fidelity check.

## Alternatives considered

- **Option A (a dedicated shape construct)** — deferred, not rejected. It is
  the correct home for non-circular ellipses, arcs, and rings, and the record
  above names it as the v1 path. It is out of scope here because no captured
  fixture and no v0/v1 requirement needs a non-circular ellipse, and the
  schema/ABI/painter surface it touches is disproportionate to a catch-up
  slice.
- **Lowering a non-circular ellipse to a stadium or clamped rounded rect** —
  rejected. It renders a shape the designer never drew, in silence (P1, P4).
  A named refusal is the only honest option until A lands.

## Trace

- Satisfies: issue #239 (lower shape nodes, ellipse first), P1/P4/P5.
- Verified by: `crates/dashc/tests/flex_lowering.rs` (ellipse-to-circle
  lowering, the arc/ring/non-circular/non-fixed refusals, the other shape
  kinds' per-kind diagnostics, the raw golden `.dsb`),
  `goldens/tooling/tests/v07_ellipse.rs` (lowered → solved → painted golden
  with a calibrated budget and a circle-versus-square sensitivity guard).
- Related: `docs/decisions/figma-flex-lowering.md`,
  `docs/decisions/paint-entry-composition.md`,
  `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`,
  `docs/decisions/dsb-frozen-fixture-r7-guard.md` (the guard option A would
  trip).
