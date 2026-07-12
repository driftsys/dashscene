# dashbuf + dashscene-core v0.2 — flex layout fields (design)

    story    #8 (epic #7, v0.2 flex core)
    branch   story/flex-schema-core
    date     2026-07-12
    status   working memory — garden before the PR lands

## Goal

Carry the v0.2 flex-layout vocabulary end-to-end through the document
schema and the semantic model (DESIGN_1.md §5, R2; issue #8): layout
mode NONE/H/V, hug/fill/fixed sizing per axis, gap, padding,
alignment, min/max — additive schema evolution, mirrored in
`dashscene-core`'s intent model and staged-mutation props. No solving:
Taffy is story #9; `commit` keeps resolving fixed geometry only, and
the new fields are stored intent the engine will consume.

Acceptance (issue #8): round-trip test covers every new field; the
arena exposes the new layout properties through the mutation API;
`just build` green.

## Scope boundaries

- **No Taffy, no resolution change.** The committed rect table is
  computed exactly as in v0.1 (authored offset + fixed size); mode/
  sizing/gap/padding/align/min/max are stored intent. Story #9 maps
  them to Taffy and rewrites the resolve step.
- **No `.dsb` load path work.** The issue lists it, but no load path
  exists anywhere yet (no crate outside `dashbuf` links the generated
  code — verified). The fields are mirrored in schema and arena so the
  future loader has nothing missing; building the loader itself stays
  with the importer slice (recorded as a decision).
- **No `dashlang` setters.** The DSL grows flex vocabulary when a
  consumer needs it (corpus stories); #9's tests can drive core
  directly.
- **Field-id coordination with #13:** the new `Node` fields append
  after `paint_entry` (the last field story #13 added); new types are
  self-contained. Noted on issue #8.

## Decisions (alternatives considered)

### D1 — Schema shape: container table + constraints table on `Node`

- **Chosen:** two new optional table fields on `Node`, mirroring the
  container/child split the vocabulary actually has (and Taffy's
  style model):

      enum LayoutMode : uint8 { None=0, Horizontal, Vertical }
      enum AxisSizing : uint8 { Fixed=0, Hug, Fill }
      enum MainAxisAlign : uint8 { Start=0, Center, End, SpaceBetween }
      enum CrossAxisAlign : uint8 { Start=0, Center, End }
      struct EdgeInsets { left, top, right, bottom: float32 }

      table FlexContainer {        // container-side (mode != None)
        mode: LayoutMode;
        gap: float32;
        padding: EdgeInsets;
        main_align: MainAxisAlign;
        cross_align: CrossAxisAlign;
      }
      table LayoutConstraints {    // child-side, any node
        sizing_h: AxisSizing;
        sizing_v: AxisSizing;
        min_width: float32 = null;   // absent = unconstrained
        max_width: float32 = null;
        min_height: float32 = null;
        max_height: float32 = null;
      }
      // Node gains: flex: FlexContainer; constraints: LayoutConstraints;

  Wrap and Grid append to `LayoutMode` at v0.8; `Baseline` appends to
  `CrossAxisAlign` (Q-4) — enum-value appends are additive. Optional
  scalars (`= null`) represent "unconstrained" as absence, not a
  sentinel value (P1: absence of intent is not a value of intent).
- Rejected — flat fields directly on `Node`: works, but smears
  container and child vocabulary into one namespace and puts eight
  rarely-set scalars on every node's table; the two tables keep the
  vocabulary grouped the way both Figma and Taffy group it.
- Rejected — replacing `FixedSizeLayout` with a layout union: not
  additive (existing writers break), and fixed geometry is not an
  alternative to flex — it is the datum flex modes reinterpret
  (width/height feed `Fixed` sizing; authored x/y apply under a
  `None` parent).

### D2 — Semantics of the existing fixed fields under flex

`FixedSizeLayout` stays the only geometry carrier: `width`/`height`
are used per axis when that axis's sizing is `Fixed` (and as the
basis Taffy needs for fixed-size children); authored `x`/`y` apply
when the parent's mode is `None` (absolute positioning) and are
ignored under H/V (the solver owns placement — P1/P2). Recorded in
schema comments; enforcement is #9's (solver) and the validator
slice's concern, not a v0.2 store-side rule.

### D3 — Core mirror: layout intent struct + granular props + one getter

- Core defines its own mirror vocabulary (no `dashbuf` dependency,
  same rationale as `docs/decisions/core-committed-output-shape.md`):
  `LayoutMode`, `AxisSizing`, `MainAxisAlign`, `CrossAxisAlign`
  enums, and a public `Layout` snapshot struct:

      pub struct Layout {
        pub x, y, width, height: f32,
        pub mode: LayoutMode,             // default None
        pub gap: f32,
        pub padding: [f32; 4],            // left, top, right, bottom
        pub main_align: MainAxisAlign,    // default Start
        pub cross_align: CrossAxisAlign,  // default Start
        pub sizing_h, sizing_v: AxisSizing, // default Fixed
        pub min_width, max_width, min_height, max_height: Option<f32>,
      }

- `Prop` gains granular variants (matching the existing per-prop
  style): `Mode`, `Gap`, `Padding { left, top, right, bottom }`,
  `MainAlign`, `CrossAlign`, `SizingH`, `SizingV`, `MinWidth`,
  `MaxWidth`, `MinHeight`, `MaxHeight` (min/max set `Some`; clearing
  a constraint is out of v0.2 scope, same precedent as fill-clearing
  in `docs/decisions/staged-mutation-v01-scope.md`).
- `Arena::layout(NodeId) -> Layout` returns the full layout intent by
  value (`Layout` is `Copy`) — the read surface the acceptance test
  needs and the minimal seam story #9's Taffy mapping will consume.
  Tree traversal accessors (roots/children) are deliberately left to
  #9, which knows the exact shape its Taffy walk wants.
- Rejected — `Prop::Constraints(...)` and `Prop::Flex(...)` composite
  setters mirroring the schema tables: the staged API's existing
  grain is one property per call (X, Y, Width…); composites would
  introduce a second grain and force read-modify-write semantics.

## Module/file impact

    crates/dashbuf/schema/dashbuf.fbs       new enums/structs/tables,
                                            two Node fields (appended)
    crates/dashbuf/tests/roundtrip.rs       every new field round-trips
    crates/dashscene-core/src/arena.rs      NodeData + Prop + Layout +
                                            Arena::layout()
    crates/dashscene-core/src/lib.rs        re-exports
    crates/dashscene-core/tests/arena.rs    prop set/read-back tests,
                                            commit-unchanged tests

## Testing

1. dashbuf round-trip: a node carrying every new field (H mode, gap,
   padding, aligns, sizings, all four constraints) reads back
   field-for-field; a node without the new tables reads back absent
   (`None`) — old documents stay valid.
2. Core: every new `Prop` variant sets its `Layout` field and reads
   back via `Arena::layout`; defaults match the schema defaults
   (mode None, sizing Fixed, aligns Start, no constraints).
3. Core: setting flex props does not change the committed rect table
   (resolution is fixed-only until #9) and marks nothing dirty.
4. Existing suites stay green (no behavior change).
