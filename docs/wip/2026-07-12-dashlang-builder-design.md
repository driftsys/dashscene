# dashlang v0.1 — minimal builder DSL (design)

    story    #5 (epic #1, v0.1 walking skeleton, stage 2)
    branch   story/dashlang
    date     2026-07-12
    status   working memory — garden before the PR lands

## Goal

The minimal Rust DSL skin over `dashscene-core`'s staged-mutation API
(DESIGN_1.md §6.2, SCOPE_DECISIONS.md §9): construct a test scene
(fixed rects + solid fills) without touching `Arena`/`Txn` calls
directly, in code that reads as a declaration.

Acceptance (issue #5): a small test scene built via the DSL produces
the same committed output as the same scene built by hand through
`dashscene-core` directly; `just build` green.

## Scope boundaries

- v0.1 vocabulary only: authored offset, fixed size, solid fill,
  names, nesting. No variants, no animation, no tokens, no
  stress-corpus generation (all later slices).
- `dashlang` depends on `dashscene-core` only (the dependency
  direction DESIGN §6.2 and SCOPE_DECISIONS §9 fix:
  `dashlang → dashscene-core → dashbuf`-shape, no `dashcue` until
  v0.4).
- The crate stub's description ("skin over the dashcue producer
  surface") predates SCOPE_DECISIONS §9's resolution and is corrected
  to name `dashscene-core`.

## Decisions (alternatives considered)

### D1 — Shape: an inert value tree with one `build` step

- **Chosen:** the DSL constructs a plain value tree (`Node` with
  offset/size/fill/children), and `Scene::build(&mut Arena) -> u64`
  walks it through one `open`/`add_node`/`set_prop`/`commit` cycle.
  Why: components become plain functions returning `Node` values
  (DESIGN §6.2: "components are fns"); loops are iterators feeding
  `children` ("loops are repeaters"); the description is inert and
  testable without an arena; and one `build` = one commit maps the
  declaration onto P3's batched-visibility model exactly like the
  future C# describe-buffer skin (one commit across the seam).
- Rejected — closure/callback builder mutating a live `Txn`
  (`scene(&mut arena, |s| { s.node("bg")... })`): couples user code to
  transaction lifetimes for no v0.1 gain, and a panic mid-closure
  leaves a half-staged intent model (core's documented no-rollback
  staging makes that pending state publish with the _next_ commit).
  The inert tree stages nothing until `build`.
- Rejected — a `scene!{}` macro: better surface syntax, real
  implementation and diagnostics cost, nothing in v0.1 needs it; a
  macro can wrap the value tree later without breaking anyone.

### D2 — Surface: free functions + consuming chainable setters

    use dashlang::{node, rgba, scene};

    fn badge(i: u32) -> dashlang::Node {
        node("badge").at(10.0 + 30.0 * i as f32, 10.0)
            .size(24.0, 24.0)
            .fill(rgba(1.0, 0.0, 0.0, 1.0))
    }

    let mut arena = dashscene_core::Arena::new();
    let generation = scene([
        node("bg").size(320.0, 240.0)
            .fill(rgba(0.1, 0.2, 0.3, 1.0))
            .children((0..3).map(badge)),
    ])
    .build(&mut arena);

- `node(name) -> Node` (and `anon() -> Node` for unnamed nodes);
  consuming `at(x, y)`, `size(w, h)`, `fill(Color)`, `child(Node)`,
  `children(impl IntoIterator<Item = Node>)` — declaration order =
  document order (core pins sibling order to creation order).
- `scene(roots) -> Scene`; `Scene::build(&mut Arena) -> u64` returns
  the commit's generation. `build` _adds_ its roots to whatever the
  arena already holds (it is a producer, not an owner) and commits
  once.
- `rgba(r, g, b, a) -> Color` is a plain constructor for
  `dashscene_core::Color`, re-exported so DSL users need one import
  path.
- Rejected — `Node::build` directly (no `Scene`): loses the
  multi-root case the arena supports; `scene([...])` costs one word.

### D3 — No validation, no defaults beyond zero

Unset offset/size stay `0.0` and unset fill stays unfilled, exactly
core's defaults — the DSL adds vocabulary, never semantics (P5's
argument applied to a producer skin: the skin's limitations must not
redefine the format, and its conveniences must not fork core's
meaning). Anything the DSL can express is expressible by hand against
core; the acceptance test asserts output equality on that basis.

## Module layout

    crates/dashlang/src/lib.rs       crate docs + the whole DSL
                                     (node/anon/rgba/Node/scene/Scene)
    crates/dashlang/tests/builder.rs acceptance: DSL output ==
                                     hand-built output; repeater
                                     children; multi-root; add-to-
                                     existing-arena behavior

One file: the v0.1 surface is ~120 lines; splitting modules would be
structure without content.

## Testing

TDD; the acceptance criterion is literally a test:

1. The issue's scene (nested fills + offsets) built via the DSL
   equals the hand-built `Arena` output: same `rects()`, same
   `paints()`, same NodeId↔index correspondence observable via names.
2. `children` accepts an iterator (repeater) and preserves order.
3. Multi-root `scene([...])` matches hand-built root order.
4. `build` on a non-empty arena appends roots and commits once
   (generation increments by exactly 1).
5. An unset fill stays `NO_PAINT`; unset geometry stays zero —
   equality with the hand-built equivalent.
